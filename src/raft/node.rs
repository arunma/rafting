use tokio::sync::{mpsc, oneshot};
use tokio::sync::mpsc::UnboundedSender;

use crate::errors::RaftResult;
use crate::rpc::{RaftRequest, RaftResponse};

pub struct RaftNode {
    id: String,
    current_term: u32,
    voted_for: String,
    //log: Vec<LogEntry>,
    commit_index: i64,
    last_applied_index: i64,
    // next_index: Vec<i64>,
    // match_index: Vec<i64>
    state: NodeState,
    server_rx: mpsc::UnboundedReceiver<(RaftRequest, oneshot::Sender<RaftResult<RaftResponse>>)>,
    elapsed_ticks: u64,
}

pub enum NodeState {
    Follower,
    Candidate,
    Leader,
}

//TODO - Let's come to this later
//Reference: https://hoverbear.org/blog/rust-state-machine-pattern/

/*
struct Leader {}

struct Candidate {}

struct Follower {}

//server_rx: mpsc::Receiver<(RaftRequest, oneshot::Sender<RaftResponse>)>

impl RaftNode<Leader> {
    fn new() -> Self {
        RaftNode {
            id: "",
            current_term: 0,
            voted_for: "",
            commit_index: 0,
            last_applied_index: 0,
            state: Leader,
        }
    }
}*/

impl RaftNode {
    pub fn new(id: String, server_rx: mpsc::UnboundedReceiver<(RaftRequest, oneshot::Sender<RaftResult<RaftResponse>>)>, peer_tx: UnboundedSender<RaftRequest>) -> Self {
        Self {
            id,
            current_term: 0,
            voted_for: "".into(),
            commit_index: 0,
            last_applied_index: 0,
            state: NodeState::Follower,
            server_rx,
            elapsed_ticks: 0,
        }
    }

    pub async fn tick(mut self) -> RaftNode {
        self //TODO - Fill in with timer logic
    }
}