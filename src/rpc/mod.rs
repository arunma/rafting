use crate::rpc::rpc_server::raft::{AppendEntriesRequest, AppendEntriesResponse, RequestVoteRequest, RequestVoteResponse};

pub mod rpc_server;

pub mod rpc_client;
pub mod rpc_peer_network;


#[derive(Debug, Clone)]
pub enum RaftEvent {
    PeerVotesRequestEvent(RequestVoteRequest),
    PeerAppendEntriesRequestEvent(AppendEntriesRequest),
    PeerVotesResponseEvent(Vec<RequestVoteResponse>),
    PeerAppendEntriesResponseEvent(Vec<AppendEntriesResponse>),

    /*
    Encapsulates individual Raft messages that are sent through the oneshot senders.
    These are the messages that an individual node sends to the client stubs via the peer network (the request is received through the server).
    The client stubs get back these individual responses and collects them into a collection before sending it back to the caller node.
    */
    RequestVoteResponseEvent(RequestVoteResponse),
    AppendEntriesResponseEvent(AppendEntriesResponse),
}

