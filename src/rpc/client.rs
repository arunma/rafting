use std::sync::Arc;

use tokio::sync::Mutex;
use tonic::Status;
use tonic::transport::Channel;
use tracing::{debug};

use crate::errors::{RaftError, RaftResult};
use crate::rpc::server::raft::{AppendEntriesRequest, AppendEntriesResponse, RequestVoteRequest, RequestVoteResponse};
use crate::rpc::server::raft::raft_grpc_client::RaftGrpcClient;

pub struct RaftGrpcClientStub {
    //Need to protect the grpc client due to concurrent access within the node for various messages
    grpc_peer_sender: Arc<Mutex<RaftGrpcClient<Channel>>>,
}


impl RaftGrpcClientStub {
    //TODO - Clean up all these Result to RaftResult
    pub async fn new(addr: &str) -> RaftResult<Self> {
        debug!("Constructing new stub for address {addr}");
        //let channel = Channel::builder(addr.parse()?).connect().await?;
        let client = RaftGrpcClient::connect(addr.to_string())
            .await
            .map_err(|_e| RaftError::InternalServerErrorWithContext(format!("Error establishing connectivity with node : {addr}")))?;
        debug!("Stub constructed for address {addr}");
        let stub = RaftGrpcClientStub {
            grpc_peer_sender: Arc::new(Mutex::new(client))
        };
        Ok(stub)
    }

    pub async fn append_entries(&self, request: AppendEntriesRequest) -> Result<AppendEntriesResponse, Status> {
        let mut client = self.grpc_peer_sender.lock().await;
        let response = client.append_entries(request).await?;
        Ok(response.into_inner())
    }

    pub async fn request_vote(&self, request: RequestVoteRequest) -> Result<RequestVoteResponse, Status> {
        let mut client = self.grpc_peer_sender.lock().await;
        let response = client.request_vote(request).await?;
        Ok(response.into_inner())
    }
}

/*#[cfg(test)]
mod tests {
    use crate::rpc::client::RaftGrpcClientStub;

    use super::*;

    #[tokio::test]
    async fn test_send_receive() {
        let addr = "http://[::1]:7070";
        /* let (tx, rx) = mpsc::channel(1);
         tokio::spawn(async move {
             let address = addr.parse().expect("should be able to parse");
             let stub = RaftGrpcServerStub::new(tx);
             stub.run(address)
         });
         sleep(Duration::from_secs(3)).await;*/

        let request = AppendEntriesRequest {
            term: 1,
            leader_id: "test_leader".to_string(),
            prev_log_index: 0,
            prev_log_term: 0,
            entries: vec![],
            leader_commit_index: 0,
        };

        let client = RaftGrpcClientStub::new(addr).await.expect("Should be able to instantiate client");
        let response = client.append_entries(request).await.expect("Should have gotten back the response");
        assert_eq!(response.from, "hello");
    }
}*/
