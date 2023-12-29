use anyhow::anyhow;
use rand::Rng;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};

use crate::errors::{RaftError, RaftResult};
use crate::raft::node::NodeState::Leader;
use crate::raft::{ELECTION_MAX_TIMEOUT, ELECTION_MIN_TIMEOUT, HEARTBEAT_TICKS};
use crate::rpc::server::raft::{AppendEntriesRequest, AppendEntriesResponse, RequestVoteRequest, RequestVoteResponse};
use crate::rpc::RaftEvent;
use crate::rpc::RaftEvent::{AppendEntriesResponseEvent, RequestVoteResponseEvent};

pub struct RaftNode {
    //TODO Expose this as a function
    pub id: String,
    peers: Vec<String>,
    current_term: u32,
    voted_for: Option<String>,
    //log: Vec<LogEntry>,
    commit_index: i64,
    last_applied_index: i64,
    // next_index: Vec<i64>,
    // match_index: Vec<i64>
    state: NodeState,
    //node_from_server_rx: mpsc::UnboundedReceiver<(RaftEvent, Option<oneshot::Sender<RaftResult<RaftEvent>>>)>,
    node_to_peers_tx: mpsc::UnboundedSender<RaftEvent>,
    //node_from_peers_rx: mpsc::UnboundedReceiver<RaftEvent>,
    elapsed_ticks_since_last_heartbeat: u64,
    election_timeout_ticks: u64,
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
    pub fn new(
        id: String,
        peers: Vec<String>,
        //node_from_server_rx: mpsc::UnboundedReceiver<(RaftEvent, Option<oneshot::Sender<RaftResult<RaftEvent>>>)>,
        node_to_peers_tx: mpsc::UnboundedSender<RaftEvent>,
        //node_from_peers_rx: mpsc::UnboundedReceiver<RaftEvent>,
    ) -> Self {
        Self {
            id,
            peers,
            current_term: 0,
            voted_for: None,
            commit_index: 0,
            last_applied_index: 0,
            state: NodeState::Follower,
            //node_from_server_rx,
            node_to_peers_tx,
            //node_from_peers_rx,
            elapsed_ticks_since_last_heartbeat: 0,
            election_timeout_ticks: rand::thread_rng()
                .gen_range(ELECTION_MIN_TIMEOUT..=ELECTION_MAX_TIMEOUT),
        }
    }

    pub fn tick(mut self) -> Result<RaftNode, RaftError> {
        //info!("ticking");
        let mut node = self;
        node.elapsed_ticks_since_last_heartbeat += 1;
        if node.state == Leader {
            if node.elapsed_ticks_since_last_heartbeat >= HEARTBEAT_TICKS {
                node.elapsed_ticks_since_last_heartbeat = 0;
                &node
                    .node_to_peers_tx
                    .send(RaftEvent::PeerAppendEntriesRequestEvent(AppendEntriesRequest {
                        term: node.current_term,
                        leader_id: (&node.id).to_string(),
                        prev_log_index: 0,
                        prev_log_term: 0,
                        entries: vec![],
                        leader_commit_index: 0,
                    }));
            }
            return Ok(node);
        } else {
            info!("Current elapsed ticks for node: {} is {}. Election timeout is : {}", node.id, node.elapsed_ticks_since_last_heartbeat, node.election_timeout_ticks);
            if node.elapsed_ticks_since_last_heartbeat >= node.election_timeout_ticks {
                let node = node.become_candidate()?; //TODO - This needs to start election
                return Ok(node);
            }
        }
        Ok(node)
    }

    pub async fn step(
        mut self,
        event: (RaftEvent, Option<oneshot::Sender<RaftResult<RaftEvent>>>),
    ) -> RaftResult<RaftNode> {
        //Process requests and responses
        let node_id = (&self.id).to_string();
        let mut node = self;
        match event {
            (RaftEvent::PeerVotesRequestEvent(req), None) => todo!(),
            (RaftEvent::PeerVotesRequestEvent(_), Some(sender)) => todo!(),
            (RaftEvent::PeerAppendEntriesRequestEvent(_), None) => todo!(),
            (RaftEvent::PeerAppendEntriesRequestEvent(_), Some(_)) => todo!(),
            (RaftEvent::PeerVotesResponseEvent(responses), Some(_)) => todo!(),
            (RaftEvent::PeerAppendEntriesResponseEvent(responses), None) => {
                //TODO - Handle the responses by updating the logs
                info!("Received AppendEntriesResponse from peers: {responses:?}");
            }
            (RaftEvent::PeerAppendEntriesResponseEvent(_), Some(_)) => todo!(),
            (RaftEvent::RequestVoteRequestEvent(req), None) => todo!(),
            (RaftEvent::RequestVoteRequestEvent(req), Some(sender)) => {
                //As a candidate or a follower, if we get a RequestVoteRequest and if the incoming term is more than the current term,
                //then vote, become a follower and bail out.
                if node.current_term < req.term {
                    let _x = sender.send(Ok(RequestVoteResponseEvent(RequestVoteResponse {
                        from: node_id,
                        term: req.term,
                        vote_granted: true,
                    })));
                }
                node = node.become_follower()?;
            }
            (RaftEvent::PeerVotesResponseEvent(responses), None) => {
                let quorum_size = (node.peers.len() + 1) / 2;
                if responses.len() >= quorum_size {
                    node = node.become_leader()?;
                }
            }
            (RaftEvent::AppendEntriesRequestEvent(_), None) => todo!(),
            (RaftEvent::AppendEntriesRequestEvent(req), Some(sender)) => {
                //If we get an AppendEntriesRequest with an incoming term more than the current term,
                //then become a follower and bail out.
                info!("Processing AppendEntriesRequestEvent from {}", req.leader_id);
                //FIXME - Ignore log entries for now
                node.elapsed_ticks_since_last_heartbeat = 0;
                let _x = sender.send(Ok(AppendEntriesResponseEvent(AppendEntriesResponse {
                    from: node.id.to_string(),
                    term: req.term,
                    success: true,
                    last_applied_index: 0,
                })));

                if node.current_term < req.term {
                    node = node.become_follower()?;
                }
            }
            (RaftEvent::RequestVoteResponseEvent(_), None) => todo!(),
            (RaftEvent::RequestVoteResponseEvent(_), Some(_)) => todo!(),
            (RaftEvent::AppendEntriesResponseEvent(_), None) => todo!(),
            (RaftEvent::AppendEntriesResponseEvent(_), Some(_)) => todo!(),
        }
        Ok(node)
    }

    pub fn become_candidate(mut self) -> RaftResult<RaftNode> {
        let mut node = self;
        node.state = NodeState::Candidate;
        node.current_term += 1;
        node.voted_for = Some(node.id.to_string());
        node.elapsed_ticks_since_last_heartbeat = 0;

        //TODO - Consider starting election here or during the next `step` call
        let request_vote = RequestVoteRequest {
            term: node.current_term,
            candidate_id: node.id.to_string(),
            last_log_index: 0, //TODO - Fill from logs later
            last_log_term: 0,
        };
        &node
            .node_to_peers_tx
            .send(RaftEvent::PeerVotesRequestEvent(request_vote))
            .map_err(|e| anyhow!(format!("Unable to send request vote to peers: {:?}", e)))?;
        Ok(node)
    }

    pub fn become_leader(mut self) -> RaftResult<RaftNode> {
        let mut node = self;
        info!("Node {} is promoted to be the LEADER", node.id);
        node.state = NodeState::Leader;
        &node
            .node_to_peers_tx
            .send(RaftEvent::PeerAppendEntriesRequestEvent(AppendEntriesRequest {
                term: node.current_term,
                leader_id: (&node.id).to_string(),
                prev_log_index: 0,
                prev_log_term: 0,
                entries: vec![],
                leader_commit_index: 0,
            }));
        Ok(node)
    }

    pub fn become_follower(mut self) -> RaftResult<RaftNode> {
        let mut node = self;
        info!("Node {} is becoming a FOLLOWER", node.id);
        node.state = NodeState::Follower;
        node.elapsed_ticks_since_last_heartbeat = 0;
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
    use crate::rpc::server::raft::RequestVoteRequest;
    use crate::rpc::RaftEvent;
    use crate::rpc::RaftEvent::PeerVotesRequestEvent;

    #[test]
    fn test_candidate_tick() -> Result<(), RaftError> {
        let (mut node_to_peers_tx, mut peers_from_node_tx) = mpsc::unbounded_channel();
        let (server_to_node_tx, node_from_server_rx) = mpsc::unbounded_channel();
        let (peers_to_node_tx, node_from_peers_rx) = mpsc::unbounded_channel();

        let mut candidate = RaftNode {
            id: "node1".to_string(),
            peers: vec!["node2".to_string()],
            current_term: 1,
            voted_for: None,
            commit_index: 0,
            last_applied_index: 0,
            state: NodeState::Candidate,
            //node_from_server_rx,
            //node_from_peers_rx,
            node_to_peers_tx,
            elapsed_ticks_since_last_heartbeat: 0,
            election_timeout_ticks: 0,
        };
        let timeout = candidate.election_timeout_ticks;
        for i in 0..timeout {
            candidate = candidate.tick()?;
        }
        let node = candidate.tick()?;
        while let Some(Some(request)) = peers_from_node_tx.recv().now_or_never() {
            if let PeerVotesRequestEvent(request) = request {
                assert_eq!(
                    request,
                    RequestVoteRequest {
                        term: 2,
                        candidate_id: "node1".to_string(),
                        last_log_index: 0,
                        last_log_term: 0,
                    }
                );
            } else {
                panic!("Unexpected failure in testcase")
            }
        }
        Ok(())
    }
}
