use std::error::Error;
use std::net::SocketAddr;

use anyhow::Context;
use tokio::sync::{mpsc, oneshot};
use tonic::{Request, Response, Status};
use tonic::transport::Server;
use tracing::{error, info};

use raft::{AppendEntriesRequest, AppendEntriesResponse, RequestVoteRequest, RequestVoteResponse};
use raft::raft_grpc_server::RaftGrpc;
use raft::raft_grpc_server::RaftGrpcServer;
use RaftRequest::{AppendEntries, RequestVote};

use crate::errors::{RaftError, RaftResult};
use crate::errors::RaftError::InternalServerErrorWithContext;
use crate::rpc::{RaftRequest, RaftResponse};

pub mod raft {
    tonic::include_proto!("raft");
}


#[derive(Debug)]
pub struct RaftGrpcServerStub {
    /*Once the message is received by the server, we need to forward the message to the RaftNode - for which we need a channel as well
      So, we send both the payload that we received as well a "oneshot sender" with which the Node can send information back to the server.
     */
    node_sender: mpsc::UnboundedSender<(RaftRequest, oneshot::Sender<RaftResult<RaftResponse>>)>,
}

impl RaftGrpcServerStub {
    pub fn new(tx: mpsc::UnboundedSender<(RaftRequest, oneshot::Sender<RaftResult<RaftResponse>>)>) -> Self {
        Self {
            node_sender: tx
        }
    }

    pub async fn run(self, address: SocketAddr) -> Result<(), RaftError> {
        //Start the tonic RPC server
        info!("Starting tonic server");
        Server::builder()
            .add_service(RaftGrpcServer::new(self))
            .serve(address)
            .await
            .map_err(|e| InternalServerErrorWithContext(format!("Unable to start tonic grpc server at {:?}. Error reported was {:?}", address, e)))
    }
}

#[tonic::async_trait]
impl RaftGrpc for RaftGrpcServerStub {
    async fn append_entries(&self, request: Request<AppendEntriesRequest>) -> Result<Response<AppendEntriesResponse>, Status> {
        println!("Request received is {:?}", request);
        //Need to send the received message to the channel
        let (server_tx, node_rx) = tokio::sync::oneshot::channel();
        //let (server_tx, node_rx) = tokio::sync::mpsc::channel(1);
        match self.node_sender.send((AppendEntries(request.into_inner()), server_tx)) {
            Ok(_) => {
                info!("Getting into the Ok section");
                Err(Status::internal("Pending implementation"))
            }
            Err(e) => {
                error!("Unable to send request to the Node {}", e);
                Err(Status::internal(format!("Unable to send request to the Node from Server: {}", e)))
            }
        }
    }

    async fn request_vote(&self, request: Request<RequestVoteRequest>) -> Result<Response<RequestVoteResponse>, Status> {
        println!("Request received is {:?}", request);
        //let (server_tx, node_rx) = mpsc::channel(1);
        let (server_tx, node_rx) = tokio::sync::oneshot::channel();
        match self.node_sender.send((RequestVote(request.into_inner()), server_tx)) {
            Ok(_) => {
                info!("Getting into the Ok section");
                Err(Status::internal("Pending implementation"))
            }
            Err(e) => {
                error!("Unable to send request to the Node :{:?}", e);
                Err(Status::internal(format!("Unable to send request to the Node from Server: {}", e)))
            }
        }
    }
}
