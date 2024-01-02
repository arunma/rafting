use std::collections::HashMap;
use std::sync::{Arc};
use anyhow::Context;

use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot::Sender;
use tokio::time::sleep;
use tracing::{debug, error, info};

use crate::errors::{RaftError, RaftResult};
use crate::rpc::RaftEvent;
use crate::rpc::rpc_client::RaftGrpcClientStub;
use crate::rpc::rpc_server::raft::{AppendEntriesRequest, AppendEntriesResponse, RequestVoteRequest, RequestVoteResponse};

#[derive(Clone)]
pub struct PeerNetwork {
    node_id: String,
    peers: HashMap<String, String>,
    peer_clients: Arc<Mutex<HashMap<String, RaftGrpcClientStub>>>,
    cluster_notifier_tx: UnboundedSender<(RaftEvent, Option<Sender<RaftResult<RaftEvent>>>)>,
}

impl PeerNetwork {
    pub fn new(node_id: String, peers: HashMap<String, String>, cluster_notifier_tx: UnboundedSender<(RaftEvent, Option<Sender<RaftResult<RaftEvent>>>)>) -> Self {
        Self {
            node_id,
            peers,
            peer_clients: Arc::new(Default::default()),
            cluster_notifier_tx,
        }
    }

    pub async fn request_vote(&self, request: RequestVoteRequest) -> RaftResult<Vec<RequestVoteResponse>> {
        let peers = self.peer_clients.lock().await;
        let mut handles = Vec::with_capacity(peers.len());
        for (_id, client) in peers.iter() {
            let future = client.request_vote(request.clone());
            handles.push(future)
        }
        let joined = futures::future::join_all(handles).await;
        let responses = joined.into_iter().filter_map(|result| {
            match result {
                Ok(resp) => {
                    debug!("Received RequestVoteResponse on node_id: {} -> :{resp:?}", self.node_id);
                    Some(resp)
                }
                Err(e) => {
                    error!("Error received at {} while sending RequestVote to the peers. Tonic error is {:?}", self.node_id, e);
                    None
                }
            }
        }).collect::<Vec<RequestVoteResponse>>();
        Ok(responses)
    }

    pub async fn append_entries(&self, request: AppendEntriesRequest) -> RaftResult<AppendEntriesResponse> {
        let peers = self.peer_clients.lock().await;
        let client = peers.get(&request.to).context(format!("Peer client for peer {:?} does not exist", &request.to))?;
        let result = client.append_entries(request).await;
        match result {
            Ok(resp) => {
                debug!("Received AppendEntriesResponse on node_id: {} -> :{resp:?}", self.node_id);
                Ok(resp)
            }
            Err(e) => {
                error!("Error received at {} while sending AppendEntry to the peers. Tonic error is {:?}", self.node_id, e);
                Err(RaftError::InternalServerErrorWithContext(format!("Error received at {} while sending AppendEntry to the peers. Tonic error is {:?}", self.node_id, e)))
            }
        }
    }

    pub async fn wait_for_peers(&mut self) -> RaftResult<()> {
        loop {
            info!("Connecting to peers");
            //let mut peer_clients = self.peer_clients.lock().await;
            let mut peer_handles = vec![];
            for (id, addr) in self.peers.clone().into_iter() {
                info!("Establishing connectivity with peer: {id} at address {addr}");
                let grpc_peer_client_handle = tokio::spawn(async move {
                    (id, RaftGrpcClientStub::new(addr).await)
                });
                peer_handles.push(grpc_peer_client_handle);
            }
            let mut peer_clients = self.peer_clients.lock().await;
            futures::future::join_all(peer_handles).await.into_iter().for_each(|result| {
                match result {
                    Ok((id, Ok(client))) => {
                        peer_clients.insert(id.to_string(), client);
                    }
                    Ok((_id, Err(e))) => {
                        error!("Unable to establish connectivity with peer: {e:?}");
                    }
                    _ => {
                        error!("Some unexpected match pattern came up in wait_for_peers");
                    }
                }
            });

            if peer_clients.len() == self.peers.len() {
                info!("All peer connections are established");
                let _x = self.cluster_notifier_tx.send((RaftEvent::ClusterNodesUpEvent, None));
                return Ok(());
            }
            info!("Not all peers have joined. Retrying in 3 seconds.");
            sleep(tokio::time::Duration::from_secs(3)).await;
        }
    }
}





