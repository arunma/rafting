use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::time::sleep;
use tonic::Status;
use tracing::info;

use crate::errors::RaftError;
use crate::rpc::client::RaftGrpcClientStub;
use crate::rpc::server::raft::{AppendEntriesRequest, AppendEntriesResponse, RequestVoteRequest, RequestVoteResponse};

#[derive(Clone, Default)]
pub struct PeerNetwork {
    peers: Arc<Mutex<HashMap<String, RaftGrpcClientStub>>>,
}

impl PeerNetwork {
    pub async fn append_entries(&self, request: AppendEntriesRequest) -> Vec<Result<AppendEntriesResponse, Status>> {
        let peers = self.peers.lock().await;
        let mut handles = Vec::with_capacity(peers.len());
        for (_id, client) in peers.iter() {
            let future = client.append_entries(request.clone());
            handles.push(future)
        }
        let joined = futures::future::join_all(handles).await;
        joined.into_iter().collect::<Vec<Result<AppendEntriesResponse, Status>>>()
    }

    pub async fn request_vote(&self, request: RequestVoteRequest) -> Vec<Result<RequestVoteResponse, Status>> {
        let peers = self.peers.lock().await;
        let mut handles = Vec::with_capacity(peers.len());
        for (_id, client) in peers.iter() {
            let future = client.request_vote(request.clone());
            handles.push(future)
        }
        let joined = futures::future::join_all(handles).await;
        joined.into_iter().collect::<Vec<Result<RequestVoteResponse, Status>>>()
    }


    pub async fn wait_for_peers(&mut self, peers: HashMap<String, String>) -> Result<(), RaftError> {
        //TODO - Optimize for clients whose link has already been established.
        loop {
            info!("Reconnecting to peers");
            let mut peer_clients = self.peers.lock().await;
            for (id, addr) in peers.iter() {
                info!("Establishing connectivity with peer: {id} at address {addr}");
                let grpc_client_result = RaftGrpcClientStub::new(&addr).await;
                //let grpc_client_result = Err(RaftError::ApplicationStartup("Startup error".to_string()));
                match grpc_client_result {
                    Ok(grpc_client) => {
                        info!("Adding peer {id} with addr: {addr} to the peer_clients");
                        peer_clients.insert(id.to_string(), grpc_client);
                    }
                    Err(e) => {
                        info!("Not all peers have joined. Retrying in 3 seconds. Last attempted error for connecting to {id} with {addr} is {}", e.to_string());
                        break;
                    }
                }
            }
            info!("Initialized peer clients are now: {}", peer_clients.len());
            if peer_clients.len() == peers.len() {
                info!("Peer map is : {:?}", peers);
                info!("Peer handle count is equal to peers count. Breaking. Keys are : {:?}", peer_clients.keys().collect::<Vec<&String>>());
                return Ok(());
            }
            sleep(tokio::time::Duration::from_secs(5)).await;
        }
    }
}


/*   pub async fn initialize_peer_clients(&mut self, peers: &HashMap<String, String>) -> Result<(), Box<dyn Error>> {
       let mut peer_clients = self.peers.lock().await;
       for (id, addr) in peers.iter() {
           info!("Establishing connectivity with peer: {id} at address {addr}");
           let grpc_client = RaftGrpcClientStub::new(&addr).await?;
           peer_clients.insert(id.to_string(), grpc_client);
       }
       info!("Initialized peer clients are now: {}", peer_clients.len());
       Ok(())
   }*/





