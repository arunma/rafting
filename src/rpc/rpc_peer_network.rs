use std::collections::HashMap;
use std::sync::{Arc};
use std::time::Duration;
use anyhow::Context;

use tokio::sync::{Mutex};
use tokio::time::sleep;
use tonic::transport::{Channel};
use tracing::{debug, error, info};

use crate::errors::{RaftError, RaftResult};
use crate::rpc::rpc_client::RaftGrpcClientStub;
use crate::rpc::rpc_server::raft::{AppendEntriesRequest, AppendEntriesResponse, RequestVoteRequest, RequestVoteResponse};

#[derive(Clone)]
pub struct PeerNetwork {
    node_id: String,
    peer_clients: Arc<Mutex<HashMap<String, RaftGrpcClientStub>>>,
}

impl PeerNetwork {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            peer_clients: Arc::new(Default::default()),
        }
    }

    /*    pub async fn request_vote(&self, request: RequestVoteRequest) -> RaftResult<Vec<RequestVoteResponse>> {
            let mut peer_clients = self.peer_clients.lock().await;
            let mut handles = Vec::with_capacity(peer_clients.len());
            //Initialize if the client is not already initialized
            loop {
                info!("({}) Initializing peer_clients if they don't exist already.", self.node_id);
                for (peer_id, peer_addr) in self.peers.iter() {
                    if !peer_clients.contains_key(peer_id) {
                        let client = RaftGrpcClientStub::new(peer_addr).await?;
                        peer_clients.insert(peer_id.to_string(), client);
                    }
                }
                if peer_clients.len() == self.peers.len() {
                    info!("({}) All peer connections are established", self.node_id);
                    break;
                } else {
                    info!("({}) Not all peers have joined. Retrying in 3 seconds.", self.node_id);
                    sleep(tokio::time::Duration::from_secs(3)).await;
                }
            }

            for (_id, client) in peer_clients.iter() {
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
        }*/

    pub async fn request_vote(&self, request: RequestVoteRequest) -> RaftResult<RequestVoteResponse> {
        let mut peer_clients = self.peer_clients.lock().await;
        /*        let peer_id = request.to.clone();
                if !peer_clients.contains_key(&peer_id) {
                    let client = RaftGrpcClientStub::new(self.peers.get(&peer_id)
                        .expect(&format!("GRPC configuration for peer {} does not exist in configuration", &peer_id))).await?;
                    peer_clients.insert(peer_id, client);
                }*/

        /* loop {
             info!("({}) Initializing peer_clients if they don't exist already.", self.node_id);
             for (peer_id, peer_addr) in self.peers.iter() {
                 if !peer_clients.contains_key(peer_id) {
                     info!("({}) Initializing peer_client for peer {} at address {}", self.node_id, peer_id, peer_addr);
                     let client = RaftGrpcClientStub::new(peer_addr).await;
                     match client {
                         Ok(client) => {
                             info!("({}) Adding peer_client for peer {} at address {}", self.node_id, peer_id, peer_addr);
                             peer_clients.insert(peer_id.to_string(), client);
                         }
                         Err(e) => {
                             error!("({}) Error initializing peer_client for peer {} at address {}. Error is {}", self.node_id, peer_id, peer_addr, e.to_string());
                             break;
                         }
                     }
                 }
             }
             if peer_clients.len() == self.peers.len() {
                 info!("({}) All peer connections are established", self.node_id);
                 break;
             } else {
                 info!("({}) Not all peers have joined. Retrying in 3 seconds.", self.node_id);
                 sleep(tokio::time::Duration::from_secs(3)).await;
             }
         }*/

        let client = peer_clients.get(&request.to).context(format!("Peer client for peer {:?} does not exist", &request.to))?;
        let result = client.request_vote(request).await;
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
        //TODO - Optimize for clients whose link has already been established.
        loop {
            info!("Connecting to peers");
            let mut peer_clients = self.peer_clients.lock().await;
            let mut peer_handles = vec![];
            for (id, addr) in self.peer_clients.iter() {
                info!("Establishing connectivity with peer: {id} at address {addr}");

                let grpc_peer_client = tokio::spawn(async move {
                    RaftGrpcClientStub::new(addr).await
                });
                peer_handles.push(grpc_peer_client);
            }

            let _x = futures::future::join_all(peer_handles).await.into_iter().for_each(|result| {
                match result {
                    Ok(Ok(resp)) => {
                        peer_clients.lock().inpush(resp);
                    }
                    _ => {
                        error!("Error received at wait_for_peers while resolving peer");
                    }
                }
            });


            info!("Initialized peer clients are now: {}", peer_clients.len());
            if peer_clients.len() == self.peer_clients.len() {
                info!("Peer handle count is equal to peers count. Breaking. Peers are : {:?}", peer_clients.keys().collect::<Vec<&String>>());
                return Ok(());
            }
            sleep(tokio::time::Duration::from_secs(5)).await;
        }
    }
}





