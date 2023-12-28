use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;

use tokio::time::sleep;
use tonic::Status;
use tracing::info;

use crate::rpc::client::RaftGrpcClientStub;
use crate::rpc::server::raft::{AppendEntriesRequest, AppendEntriesResponse, RequestVoteRequest, RequestVoteResponse};

pub struct PeerNetwork {
    peers: HashMap<String, RaftGrpcClientStub>,
}

impl PeerNetwork {
    pub async fn append_entries(&self, request: AppendEntriesRequest) -> Vec<Result<AppendEntriesResponse, Status>> {
        let mut handles = Vec::with_capacity(self.peers.len());
        for (_id, client) in self.peers.iter() {
            let future = client.append_entries(request.clone());
            handles.push(future)
        }
        let joined = futures::future::join_all(handles).await;
        joined.into_iter().collect::<Vec<Result<AppendEntriesResponse, Status>>>()
    }

    pub async fn request_vote(&self, request: RequestVoteRequest) -> Vec<Result<RequestVoteResponse, Status>> {
        let mut handles = Vec::with_capacity(self.peers.len());
        for (_id, client) in self.peers.iter() {
            let future = client.request_vote(request.clone());
            handles.push(future)
        }
        let joined = futures::future::join_all(handles).await;
        joined.into_iter().collect::<Vec<Result<RequestVoteResponse, Status>>>()
    }
}

pub async fn initialize_peer_clients(peers: &HashMap<&str, &str>) -> Result<PeerNetwork, Box<dyn Error>> {
    let mut peer_clients = HashMap::new();

    for (&id, &addr) in peers.iter() {
        let grpc_client = RaftGrpcClientStub::new(addr).await?;
        peer_clients.insert(id.to_string(), grpc_client);
    }
    Ok(PeerNetwork { peers: peer_clients })
}

pub async fn wait_for_peers(peers: &HashMap<&str, &str>) -> PeerNetwork {
    loop {
        let result = initialize_peer_clients(peers).await;
        match result {
            Ok(peer_clients) => return peer_clients,
            Err(e) => {
                info!("Not all peers have joined. Retrying in 5 seconds");
                sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

