use std::collections::HashMap;
use std::sync::{Arc};
use tokio::sync::mpsc::UnboundedSender;

use tokio::sync::{Mutex, MutexGuard, oneshot};
use tokio::time::sleep;
use tonic::Status;
use tracing::{debug, error, info};

use crate::errors::{RaftError, RaftResult};
use crate::rpc::client::RaftGrpcClientStub;
use crate::rpc::RaftEvent;
use crate::rpc::server::raft::{AppendEntriesRequest, AppendEntriesResponse, RequestVoteRequest, RequestVoteResponse};

#[derive(Clone)]
pub struct PeerNetwork {
    node_id: String,
    peers: Arc<Mutex<HashMap<String, RaftGrpcClientStub>>>,
    server_to_node_tx: UnboundedSender<(RaftEvent, Option<oneshot::Sender<RaftResult<RaftEvent>>>)>,
}

impl PeerNetwork {
    pub fn new(node_id: String, server_to_node_tx: UnboundedSender<(RaftEvent, Option<oneshot::Sender<RaftResult<RaftEvent>>>)>) -> Self {
        Self {
            node_id,
            peers: Arc::new(Default::default()),
            server_to_node_tx,
        }
    }
    pub async fn append_entries(&self, request: AppendEntriesRequest) -> RaftResult<Vec<AppendEntriesResponse>> {
        let peers = self.peers.lock().await;
        let mut handles = Vec::with_capacity(peers.len());
        for (_id, client) in peers.iter() {
            let future = client.append_entries(request.clone());
            handles.push(future)
        }
        let joined = futures::future::join_all(handles).await;
        let responses = joined.into_iter().filter_map(|result| {
            match result {
                Ok(resp) => {
                    info!("Received AppendEntriesResponse on node_id: {} -> :{resp:?}", self.node_id);
                    Some(resp)
                    //self.server_to_node_tx.send((RaftEvent::RequestVoteResponseEvent(resp), None)).expect("Should be able to forward message received from peer to node");
                }
                Err(e) => {
                    error!("Error received at {} while sending AppendEntry to the peers. Tonic error is {:?}", self.node_id, e);
                    None
                }
            }
        }).collect::<Vec<AppendEntriesResponse>>();
        Ok(responses)
    }

    pub async fn request_vote(&self, request: RequestVoteRequest) -> RaftResult<Vec<RequestVoteResponse>> {
        let peers = self.peers.lock().await;
        let mut handles = Vec::with_capacity(peers.len());
        for (_id, client) in peers.iter() {
            let future = client.request_vote(request.clone());
            handles.push(future)
        }
        let joined = futures::future::join_all(handles).await;
        let responses = joined.into_iter().filter_map(|result| {
            match result {
                Ok(resp) => {
                    info!("Received RequestVoteResponse on node_id: {} -> :{resp:?}", self.node_id);
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


    pub async fn wait_for_peers(&mut self, peers: HashMap<String, String>) -> RaftResult<()> {
        //TODO - Optimize for clients whose link has already been established.
        loop {
            info!("Connecting to peers");
            let mut peer_clients = self.peers.lock().await;
            self.initialize_peer_clients(&peers, peer_clients).await?;
            if peer_clients.len() == peers.len() {
                debug!("Peer map is : {:?}", peers);
                debug!("Peer handle count is equal to peers count. Breaking. Peers are : {:?}", peer_clients.keys().collect::<Vec<&String>>());
                return Ok(());
            }
            sleep(tokio::time::Duration::from_secs(5)).await;
        }
    }
    pub async fn initialize_peer_clients(&mut self, peers: &HashMap<String, String>, mut peer_clients: MutexGuard<HashMap<String, RaftGrpcClientStub>>) -> RaftResult<()> {
        for (id, addr) in peers.iter() {
            info!("Establishing connectivity with peer: {id} at address {addr}");
            let grpc_client_result = RaftGrpcClientStub::new(&addr).await;
            match grpc_client_result {
                Ok(grpc_client) => {
                    info!("Adding node with {id} and addr {addr} as peer");
                    peer_clients.insert(id.to_string(), grpc_client);
                }
                Err(e) => {
                    info!("Not all peers have joined. Retrying in 3 seconds. Last attempted error for connecting to {id} with {addr} is {}", e.to_string());
                    break;
                }
            }
        }
        debug!("Initialized peer clients are now: {}", peer_clients.len());
        Ok(())
    }
}








