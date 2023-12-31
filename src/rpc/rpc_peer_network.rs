use std::collections::HashMap;
use std::sync::{Arc};
use anyhow::Context;

use tokio::sync::{Mutex};
use tokio::time::sleep;
use tracing::{debug, error, info};

use crate::errors::{RaftError, RaftResult};
use crate::rpc::rpc_client::RaftGrpcClientStub;
use crate::rpc::rpc_server::raft::{AppendEntriesRequest, AppendEntriesResponse, RequestVoteRequest, RequestVoteResponse};

#[derive(Clone)]
pub struct PeerNetwork {
    node_id: String,
    peers: Arc<Mutex<HashMap<String, RaftGrpcClientStub>>>,
}

impl PeerNetwork {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            peers: Arc::new(Default::default()),
        }
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
        let peers = self.peers.lock().await;
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


    pub async fn wait_for_peers(&mut self, peers: HashMap<String, String>) -> RaftResult<()> {
        //TODO - Optimize for clients whose link has already been established.
        loop {
            info!("Connecting to peers");
            let mut peer_clients = self.peers.lock().await;
            for (id, addr) in peers.iter() {
                info!("Establishing connectivity with peer: {id} at address {addr}");
                let grpc_client_result = RaftGrpcClientStub::new(addr).await;
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
            if peer_clients.len() == peers.len() {
                debug!("Peer map is : {:?}", peers);
                debug!("Peer handle count is equal to peers count. Breaking. Peers are : {:?}", peer_clients.keys().collect::<Vec<&String>>());
                return Ok(());
            }
            sleep(tokio::time::Duration::from_secs(5)).await;
        }
    }
}





