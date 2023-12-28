use crate::rpc::server::raft::{AppendEntriesRequest, AppendEntriesResponse, RequestVoteRequest, RequestVoteResponse};

pub mod server;

pub mod client;
pub mod peer;


#[derive(Debug)]
pub enum RaftRequest {
    AppendEntries(AppendEntriesRequest),
    RequestVote(RequestVoteRequest),
}

#[derive(Debug)]
pub enum RaftResponse {
    AppendEntries(AppendEntriesResponse),
    RequestVote(RequestVoteResponse),
}
