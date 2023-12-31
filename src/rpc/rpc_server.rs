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
use crate::rpc::RaftEvent::{AppendEntriesRequestEvent, PeerVotesRequestEvent};
use crate::rpc::rpc_server::raft::{AppendEntriesRequest, RequestVoteRequest};

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
    async fn request_vote(&self, request: Request<RequestVoteRequest>) -> Result<Response<RequestVoteResponse>, Status> {
        info!("RequestVote received : {:?}", request);
        let (node_to_server_tx, server_from_node_rx) = oneshot::channel();
        self.server_to_node_tx
            .send((PeerVotesRequestEvent(request.into_inner()), Some(node_to_server_tx)))
            .map_err(|e| Status::internal(format!("Unable to forward request_vote to node. Error is : {e:?}")))?;
        info!("RequestVote forwarded to node");
        match server_from_node_rx.await {
            Ok(Ok(RaftEvent::RequestVoteResponseEvent(response))) => {
                info!("Sending response back to callee : {:?}", response);
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

    async fn append_entries(&self, request: Request<AppendEntriesRequest>) -> Result<Response<AppendEntriesResponse>, Status> {
        let request = request.into_inner();
        if !request.entries.is_empty() {
            info!("AppendEntries received : {:?}", request);
        }
        let (node_to_server_tx, server_from_node_rx) = tokio::sync::oneshot::channel();
        self.server_to_node_tx
            .send((AppendEntriesRequestEvent(request), Some(node_to_server_tx)))
            .map_err(|e| Status::internal(format!("Unable to forward request_vote to node. Error is : {e:?}")))?;
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
}
