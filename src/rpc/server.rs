use std::net::SocketAddr;

use tokio::sync::{mpsc, oneshot};
use tonic::{Request, Response, Status};
use tonic::transport::Server;
use tracing::{error, info};

use raft::{AppendEntriesResponse, RequestVoteResponse};
use raft::raft_grpc_server::RaftGrpc;
use raft::raft_grpc_server::RaftGrpcServer;

use crate::errors::{RaftError, RaftResult};
use crate::errors::RaftError::InternalServerErrorWithContext;
use crate::rpc::{RaftEvent};
use crate::rpc::RaftEvent::{PeerAppendEntriesRequestEvent, PeerVotesRequestEvent};
use crate::rpc::server::raft::{AppendEntriesRequest, RequestVoteRequest};

pub mod raft {
    tonic::include_proto!("raft");
}

#[derive(Debug)]
pub struct RaftGrpcServerStub {
    /*Once the message is received by the server, we need to forward the message to the RaftNode - for which we need a channel as well
      So, we send both the payload that we received as well a "oneshot sender" with which the Node can send information back to the server.
     */
    server_to_node_tx: mpsc::UnboundedSender<(RaftEvent, Option<oneshot::Sender<RaftResult<RaftEvent>>>)>,
}

impl RaftGrpcServerStub {
    pub fn new(tx: mpsc::UnboundedSender<(RaftEvent, Option<oneshot::Sender<RaftResult<RaftEvent>>>)>) -> Self {
        Self {
            server_to_node_tx: tx
        }
    }

    pub async fn run(self, address: SocketAddr) -> Result<(), RaftError> {
        //Start the tonic RPC server
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
        info!("Request received is {:?}", request);
        let (node_to_server_tx, server_from_node_rx) = tokio::sync::oneshot::channel();
        self.server_to_node_tx.send((PeerAppendEntriesRequestEvent(request.into_inner()), Some(node_to_server_tx))).expect("Should be able to forward the request to node");
        match server_from_node_rx.await {
            Ok(Ok(RaftEvent::AppendEntriesResponseEvent(response))) => {
                Ok(Response::new(response))
            }
            Err(e) => {
                error!("Unable to send back append_entries response back to callee. Error is : {e:?}");
                Err(Status::internal(format!("Unable to send back append_entries response back to callee. Error is : {e:?}")))
            }
            _ => {
                error!("Some unexpected match pattern came up in append_entries response stream");
                Err(Status::internal("Some unexpected match pattern came up in append_entries response stream"))
            }
        }
    }
    async fn request_vote(&self, request: Request<RequestVoteRequest>) -> Result<Response<RequestVoteResponse>, Status> {
        info!("Request received is {:?}", request);
        let (node_to_server_tx, server_from_node_rx) = oneshot::channel();
        self.server_to_node_tx.send((PeerVotesRequestEvent(request.into_inner()), Some(node_to_server_tx))).expect("Should be able to forward the request to node");
        match server_from_node_rx.await {
            Ok(Ok(RaftEvent::RequestVoteResponseEvent(response))) => {
                Ok(Response::new(response))
            }
            Err(e) => {
                error!("Unable to send back request_vote response back to callee. Error is : {e:?}");
                Err(Status::internal(format!("Unable to send back request_vote response back to callee. Error is : {e:?}")))
            }
            _ => {
                error!("Some unexpected match pattern came up in request_vote response stream");
                Err(Status::internal("Some unexpected match pattern came up in request_vote response stream"))
            }
        }
    }
}
