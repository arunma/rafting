use anyhow::anyhow;
use rand::Rng;
use tokio::sync::{mpsc, oneshot};
use tokio::sync::mpsc::UnboundedSender;

use crate::errors::{RaftError, RaftResult};
use crate::raft::{ELECTION_MAX_TIMEOUT, ELECTION_MIN_TIMEOUT};
use crate::raft::node::NodeState::Leader;
use crate::rpc::{RaftRequest, RaftResponse};
use crate::rpc::RaftRequest::RequestVote;
use crate::rpc::server::raft::RequestVoteRequest;

pub struct RaftNode {
    id: String,
    current_term: u32,
    voted_for: Option<String>,
    //log: Vec<LogEntry>,
    commit_index: i64,
    last_applied_index: i64,
    // next_index: Vec<i64>,
    // match_index: Vec<i64>
    state: NodeState,
    server_rx: mpsc::UnboundedReceiver<(RaftRequest, oneshot::Sender<RaftResult<RaftResponse>>)>,
    peer_tx: mpsc::UnboundedSender<RaftRequest>,
    elapsed_ticks: u64,
    election_timeout: u64,
}

#[derive(Debug, PartialEq, Eq)]
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
            voted_for: None,
            commit_index: 0,
            last_applied_index: 0,
            state: NodeState::Follower,
            server_rx,
            peer_tx,
            elapsed_ticks: 0,
            election_timeout: rand::thread_rng().gen_range(ELECTION_MIN_TIMEOUT..=ELECTION_MAX_TIMEOUT),
        }
    }

    pub fn tick(mut self) -> Result<RaftNode, RaftError> {
        //info!("ticking");
        if self.state == Leader {
            return Ok(self);
        }
        self.elapsed_ticks += 1;
        if self.elapsed_ticks >= self.election_timeout {
            let node = self.become_candidate()?; //TODO - This needs to start election
            return Ok(node);
        }
        Ok(self)
    }

    pub async fn step(mut self) -> RaftNode {
        //Receive votes
        todo!()
    }


    pub fn become_candidate(mut self) -> Result<RaftNode, RaftError> {
        let mut node = self;
        node.state = NodeState::Candidate;
        node.current_term += 1;
        node.voted_for = Some(node.id.to_string());
        node.elapsed_ticks = 0;

        //TODO - Consider starting election here or during the next `step` call
        let request_vote = RequestVoteRequest {
            term: node.current_term,
            candidate_id: node.id.to_string(),
            last_log_index: 0, //TODO - Fill from logs later
            last_log_term: 0,
        };
        &node.peer_tx
            .send(RequestVote(request_vote))
            .map_err(|e| anyhow!(format!("Unable to send request vote to peers: {:?}", e)))?;
        Ok(node)
    }
}

#[cfg(test)]
mod tests {
    use futures_util::FutureExt;
    use test_log::test;
    use tokio::sync::mpsc;

    use crate::errors::RaftError;
    use crate::raft::node::{NodeState, RaftNode};
    use crate::rpc::RaftRequest::RequestVote;
    use crate::rpc::server::raft::RequestVoteRequest;

    #[test]
    fn test_candidate_tick() -> Result<(), RaftError> {
        let (mut peer_tx, mut peer_rx) = mpsc::unbounded_channel();
        let (server_tx, server_rx) = mpsc::unbounded_channel();
        let mut candidate = RaftNode {
            id: "node1".to_string(),
            current_term: 1,
            voted_for: None,
            commit_index: 0,
            last_applied_index: 0,
            state: NodeState::Candidate,
            server_rx,
            peer_tx,
            elapsed_ticks: 0,
            election_timeout: 0,
        };
        let timeout = candidate.election_timeout;
        for i in 0..timeout {
            candidate = candidate.tick()?;
        }
        let node = candidate.tick()?;
        while let Some(Some(request)) = peer_rx.recv().now_or_never() {
            if let RequestVote(request) = request {
                assert_eq!(request, RequestVoteRequest {
                    term: 2,
                    candidate_id: "node1".to_string(),
                    last_log_index: 0,
                    last_log_term: 0,
                });
            } else {
                panic!("Unexpected failure in testcase")
            }
        }
        Ok(())
    }
}