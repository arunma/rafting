use crate::rpc::server::raft::{AppendEntriesRequest, AppendEntriesResponse, RequestVoteRequest, RequestVoteResponse};

pub mod server;

pub mod client;
pub mod peer;


#[derive(Debug, Clone)]
pub enum RaftEvent {
    PeerVotesRequestEvent(RequestVoteRequest),
    PeerAppendEntriesRequestEvent(AppendEntriesRequest),
    PeerVotesResponseEvent(Vec<RequestVoteResponse>),
    PeerAppendEntriesResponseEvent(Vec<AppendEntriesResponse>),
    //Individual request responses
    RequestVoteRequestEvent(RequestVoteRequest),
    AppendEntriesRequestEvent(AppendEntriesRequest),
    RequestVoteResponseEvent(RequestVoteResponse),
    AppendEntriesResponseEvent(AppendEntriesResponse),
}

