use std::cmp::max;
use std::collections::HashMap;
use anyhow::{anyhow, Context};
use rand::Rng;
use tabled::Table;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info};

use crate::errors::{RaftError, RaftResult};
use crate::raft::raft_log::{RaftLog};
use crate::raft::raft_node::NodeState::{Follower, Leader};
use crate::raft::{ELECTION_MAX_TIMEOUT_TICKS, ELECTION_MIN_TIMEOUT_TICKS, HEARTBEAT_TICKS};
use crate::raft::display::DisplayableLogEntry;
use crate::rpc::rpc_server::raft::{AppendEntriesRequest, AppendEntriesResponse, LogEntry, RequestVoteRequest, RequestVoteResponse};
use crate::rpc::RaftEvent;
use crate::rpc::RaftEvent::{AppendEntriesResponseEvent, RequestVoteResponseEvent};
use crate::web::ClientEvent;

pub struct RaftNode {
    //TODO Expose id as a function
    id: String,
    //Persistent state
    current_term: i32,
    voted_for: Option<String>,
    log: RaftLog,
    peers: Vec<String>,
    //Volatile state for all servers
    commit_index: i64,
    commit_term: i32,
    //Volatile state for leader
    next_index: HashMap<String, i64>,
    match_index: HashMap<String, i64>,

    state: NodeState,
    node_to_peers_tx: mpsc::UnboundedSender<RaftEvent>,
    elapsed_ticks_since_last_heartbeat: u64,
    election_timeout_ticks: u64,
    votes: usize,
    cluster_ready: bool, //This is a hack. Need to revisit
}

#[derive(Debug, PartialEq, Eq)]
pub enum NodeState {
    Follower,
    Candidate,
    Leader,
}

impl RaftNode {
    pub fn new(
        id: String,
        peers: Vec<String>,
        node_to_peers_tx: mpsc::UnboundedSender<RaftEvent>,
    ) -> Self {
        Self {
            id,
            current_term: 0,
            voted_for: None,
            log: RaftLog::new(),
            peers,
            commit_index: -1,
            commit_term: -1,
            next_index: Default::default(),
            match_index: Default::default(),
            state: NodeState::Follower,
            node_to_peers_tx,
            elapsed_ticks_since_last_heartbeat: 0,
            election_timeout_ticks: rand::thread_rng()
                .gen_range(ELECTION_MIN_TIMEOUT_TICKS..=ELECTION_MAX_TIMEOUT_TICKS),
            votes: 0,
            cluster_ready: false,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn last_applied_index(&self) -> i64 {
        self.log.last_log_index()
    }

    pub fn tick(self) -> Result<RaftNode, RaftError> {
        let mut node = self;
        if !node.cluster_ready {
            info!("Cluster is not ready. Skipping tick");
            return Ok(node);
        }

        node.elapsed_ticks_since_last_heartbeat += 1;
        if node.state == Leader {
            // As a leader, send heartbeats to all peers
            if node.elapsed_ticks_since_last_heartbeat >= HEARTBEAT_TICKS {
                node.elapsed_ticks_since_last_heartbeat = 0;
                for peer in node.peers.iter() {
                    let append_request = node.build_append_request_for_peer(peer);
                    let _ = &node.node_to_peers_tx.send(RaftEvent::AppendEntriesRequestEvent(append_request));
                }
            }
            return Ok(node);
        } else {
            // As a follower, if we don't hear from the leader within the election timeout, then become a candidate
            debug!(
                "Current elapsed ticks for node: {} is {}. Election timeout is : {}",
                node.id, node.elapsed_ticks_since_last_heartbeat, node.election_timeout_ticks
            );
            if node.elapsed_ticks_since_last_heartbeat >= node.election_timeout_ticks {
                let node = node.become_candidate()?; //TODO - This needs to start election
                return Ok(node);
            }
        }
        Ok(node)
    }

    pub fn build_append_request_for_peer(&self, peer: &str) -> AppendEntriesRequest {
        let node = self;
        let next_index = *self.next_index.get(peer).unwrap_or(&0);
        let (prev_log_index, prev_log_term, entries) = if next_index == 0 {
            let entries = self.log.get_from(0);
            (-1, -1, entries)
        } else {
            let (prev_log_index, prev_log_term) = self
                .log
                .get((next_index - 1) as usize)
                .map(|entry| (entry.index, entry.term))
                .unwrap();
            let entries = if self.log.last_log_index() >= next_index {
                self.log.get_from(next_index as usize)
            } else {
                vec![]
            };
            (prev_log_index, prev_log_term, entries)
        };

        debug!("Entries propaged from leader {} to peer {} are: {:?}", node.id, peer, entries);
        AppendEntriesRequest {
            to: peer.to_string(),
            term: node.current_term,
            leader_id: node.id.to_string(),
            prev_log_index,
            prev_log_term,
            entries,
            leader_commit_index: node.commit_index,
            leader_commit_term: node.commit_term,
        }
    }


    pub fn step(
        self,
        event: (RaftEvent, Option<oneshot::Sender<RaftResult<RaftEvent>>>),
    ) -> RaftResult<RaftNode> {
        //Process requests and responses
        let node_id = self.id.to_string();
        let mut node = self;
        match event {
            (RaftEvent::PeerVotesRequestEvent(req), Some(sender)) => {
                info!("Peer votes request event: {req:?}");
                /*Sometimes (especially during the first election), two candidates are competing for the same term. In this case, we need to
                make sure that the follower votes only for one of them.*/
                if req.term == node.current_term && node.voted_for.is_some() && node.voted_for.as_ref().unwrap() != &req.candidate_id {
                    let _ = sender.send(Ok(RequestVoteResponseEvent(RequestVoteResponse {
                        from: node_id,
                        term: req.term,
                        vote_granted: false,
                    })));
                    return Ok(node);
                }

                /*As a candidate or a follower, if we get a RequestVoteRequest and if the incoming term is more than the current term,
                then vote, become a follower and bail out.*/
                if node.current_term < req.term {
                    let _ = sender.send(Ok(RequestVoteResponseEvent(RequestVoteResponse {
                        from: node_id,
                        term: req.term,
                        vote_granted: true,
                    })));
                    node = node.become_follower(req.term)?;
                    node.voted_for = Some(req.candidate_id.clone());
                } else {
                    let _ = sender.send(Ok(RequestVoteResponseEvent(RequestVoteResponse {
                        from: node_id,
                        term: req.term,
                        vote_granted: false,
                    })));
                }
            }
            (RaftEvent::AppendEntriesRequestEvent(req), Some(sender)) => {
                //If we get an AppendEntriesRequest with an incoming term more than the current term,
                //then become a follower and bail out.
                debug!("Processing AppendEntriesRequestEvent {}",req.leader_id );
                if node.current_term < req.term && node.state != Follower {
                    node = node.become_follower(req.term)?;
                    node.voted_for = Some(req.leader_id.clone()); //
                }

                if !req.entries.is_empty() {
                    info!("Received entries from leader {} for peer {} : {:?}", req.leader_id, node.id, req.entries);
                    /*Several validation needs to be done at this point.
                        1. If the prev_log_index is -1, then the leader is sending the first entry to the follower.
                            In this case, we need to check if the term of the entry is the same as the current term.
                            If not, then we need to truncate the log and append the entry.
                        2. If the prev_log_index is not -1, then we need to check if the entry at prev_log_index matches the prev_log_term.
                           If not, then the incoming AppendEntryRequest has over-reached. We need to send back a failure AppendEntryResponse
                            so that we could retry with the correct prev_log_index.
                        3. If the prev_log_index is not -1 and the request entry's prev_log_index and prev_log_term matches with the
                           node's last_log_index and term, then we need to append the entries.
                        4. If the leader_commit_index is greater than the node's commit_index, then we need to update the node's commit_index
                           to the minimum of leader_commit_index and the last_log_index.
                     */

                    if req.prev_log_index == -1 {
                        //1. First entry from Leader
                        node.log.truncate(0); //Just to be sure that the log is empty
                        node.log.append_all(req.entries);
                    } else {
                        let node_term_for_index = node.log.term_for_index(req.prev_log_index as usize);
                        //2. Overreached AppendEntryRequest
                        if req.prev_log_index > node.last_applied_index() || req.prev_log_term != node_term_for_index {
                            let _ = sender.send(Ok(AppendEntriesResponseEvent(AppendEntriesResponse {
                                from: node.id.to_string(),
                                term: node.current_term,
                                success: false,
                                last_applied_index: node.last_applied_index() - 1,
                            })));
                            return Ok(node);
                        }

                        //3. All good. Append the entries
                        let (last_log_index, last_log_term) = node.log.last_log_index_and_term();
                        if req.prev_log_index <= last_log_index && req.prev_log_term == last_log_term {
                            node.log.append_all_from(req.entries, req.prev_log_index as usize + 1);
                        }
                    }
                    let table = Table::new(DisplayableLogEntry::formatted(node.log.inner())).to_string();
                    info!("Entries in the FOLLOWER {} after appending log from LEADER: {} are: \n {table}", node.id, req.leader_id);
                }

                //4. Check if the leader_commit_index received is greater than what the current node has.
                if req.leader_commit_index > node.commit_index {
                    let min_commit_index = std::cmp::min(req.leader_commit_index, node.last_applied_index());
                    node.commit_index = min_commit_index;
                }
                //if node.last_applied_index() > req.leader_commit_index
                node.elapsed_ticks_since_last_heartbeat = 0;
                let _ = sender.send(Ok(AppendEntriesResponseEvent(AppendEntriesResponse {
                    from: node.id.to_string(),
                    term: req.term,
                    success: true,
                    last_applied_index: node.last_applied_index(),
                })));
            }
            (RaftEvent::AppendEntriesResponseEvent(response), None) => {
                debug!("Received AppendEntriesResponse from peer: {response:?}");
                if response.term > node.current_term && node.state != Follower {
                    node = node.become_follower(response.term)?;
                }
                if node.state == Leader && node.current_term == response.term {
                    if response.success {
                        //Update next_index and match_index
                        node.next_index.insert(response.from.clone(), response.last_applied_index + 1);
                        node.match_index.insert(response.from, response.last_applied_index);
                        //Update commit_index based on the presence of the index (with matched term) from quorum
                        let commit_index = max(node.commit_index, 0) as usize;
                        for ci in commit_index..node.log.len() {
                            let entry = node.log.get(ci).unwrap();
                            if entry.term == node.current_term {
                                let count = node
                                    .peers
                                    .iter()
                                    .filter(|&p| *node.match_index.get(p).unwrap_or(&0) as usize >= ci)
                                    .count();
                                if count >= (node.peers.len() + 1) / 2 {
                                    node.commit_index = ci as i64;
                                }
                            }
                        }
                        info!("Received AppendEntryResponses from quorum. Commit index of the LEADER is now: {}", node.commit_index);
                    } else {
                        //Decrement next_index and retry
                        let next_index = node.next_index.get_mut(&response.from).unwrap();
                        *next_index -= 1;
                        //Retry would be done by the `tick` function
                    }
                }
                //TODO - Handle the responses by updating the logs
            }
            (RaftEvent::PeerVotesResponseEvent(responses), None) => {
                info!("Received RequestVoteResponse from peers: {responses:?}");
                let quorum_size = (node.peers.len() + 1) / 2;
                let response_count = responses.iter().filter(|&r| r.vote_granted).count();
                if response_count >= quorum_size {
                    //Yay! We have won the election. Become a leader.
                    node = node.become_leader()?;
                } else {
                    //We have failed the election. Become a follower.
                    let current_term = node.current_term;
                    node = node.become_follower(current_term)?;
                }
            }
            (RaftEvent::ClusterNodesDownEvent, None) => {
                info!("Not all cluster members are up. Received ClusterNodesDownEvent from peer_network");
                node.cluster_ready = false;
            }
            (RaftEvent::ClusterNodesUpEvent, None) => {
                info!("All cluster members are up. Received ClusterNodesUpEvent from peer_network");
                node.cluster_ready = true;
            }
            _ => {
                error!("Unexpected event received: {:?}", event);
                return Err(RaftError::InternalServerErrorWithContext(format!(
                    "Unexpected event received: {:?}",
                    event
                )));
            }
        }
        Ok(node)
    }

    //Process client requests
    pub fn handle_client_request(
        &mut self,
        event: (
            ClientEvent,
            oneshot::Sender<RaftResult<ClientEvent>>,
        ),
    ) -> RaftResult<()> {
        match event {
            (ClientEvent::CommandRequestEvent(cmd), sender) => {
                if self.state != Leader {
                    let _x = sender.send(Err(RaftError::BadRequest("Client request can be sent only to the leader.".to_string())));
                    Err(RaftError::BadRequest("Client request can be sent only to the leader.".to_string()))
                } else {
                    let entry = LogEntry {
                        index: self.last_applied_index() + 1,
                        term: self.current_term,
                        command: serde_json::to_string(&cmd).context("Unable to serialize SetCommand to json")?,
                    };
                    info!("New LogEntry being added to LEADER {} is : {:?}", self.id, entry);
                    self.log.append(entry); //Replication will be taken care of by the `tick` function
                    let table = Table::new(DisplayableLogEntry::formatted(self.log.inner())).to_string();
                    info!("Entries in the LEADER {} after addition is: \n {table}", self.id);
                    let _ = sender.send(Ok(ClientEvent::CommandResponseEvent(true)));
                    Ok(())
                }
            }
            _ => {
                error!("Unexpected event received: {:?}", event);
                Err(RaftError::InternalServerErrorWithContext(format!(
                    "Unexpected event received: {:?}",
                    event
                )))
            }
        }
    }

    pub fn become_candidate(self) -> RaftResult<RaftNode> {
        let mut node = self;
        node.state = NodeState::Candidate;
        node.current_term += 1;
        node.voted_for = Some(node.id.to_string());
        node.elapsed_ticks_since_last_heartbeat = 0;
        node.votes = 1;
        let (last_log_index, last_log_term) = node.log.last_log_index_and_term();

        //TODO - Consider starting election here or during the next `step` call
        let request_vote = RequestVoteRequest {
            to: "ALL".to_string(), //FIXME
            term: node.current_term,
            candidate_id: node.id.to_string(),
            last_log_index,
            last_log_term,
        };
        let _ = &node
            .node_to_peers_tx
            .send(RaftEvent::PeerVotesRequestEvent(request_vote))
            .map_err(|e| anyhow!(format!("Unable to send request vote to peers: {:?}", e)))?;
        Ok(node)
    }

    pub fn become_leader(self) -> RaftResult<RaftNode> {
        let mut node = self;
        info!("Node {} is promoted to be the LEADER", node.id);
        node.state = NodeState::Leader;
        let (last_log_index, last_log_term) = node.log.last_log_index_and_term();
        for peer in node.peers.iter() {
            let _ = &node
                .node_to_peers_tx
                .send(RaftEvent::AppendEntriesRequestEvent(AppendEntriesRequest {
                    to: peer.to_string(),
                    term: node.current_term,
                    leader_id: node.id.to_string(),
                    prev_log_index: last_log_index,
                    prev_log_term: last_log_term,
                    entries: vec![],
                    leader_commit_index: node.commit_index,
                    leader_commit_term: node.commit_term,
                }));
        }
        Ok(node)
    }

    pub fn become_follower(self, term: i32) -> RaftResult<RaftNode> {
        let mut node = self;
        info!("Node {} is becoming a FOLLOWER", node.id);
        node.state = NodeState::Follower;
        node.current_term = term;
        node.voted_for = None;
        node.elapsed_ticks_since_last_heartbeat = 0;
        node.votes = 0;
        Ok(node)
    }
}

#[cfg(test)]
mod tests {
    use futures_util::FutureExt;
    use test_log::test;
    use tokio::sync::{mpsc, oneshot};

    use crate::errors::RaftError;
    use crate::raft::HEARTBEAT_TICKS;
    use crate::raft::raft_log::RaftLog;
    use crate::raft::raft_node::{NodeState, RaftNode};
    use crate::rpc::rpc_server::raft::{AppendEntriesRequest, AppendEntriesResponse, LogEntry, RequestVoteRequest, RequestVoteResponse};
    use crate::rpc::RaftEvent;
    use crate::rpc::RaftEvent::{AppendEntriesResponseEvent, PeerVotesRequestEvent, PeerVotesResponseEvent, RequestVoteResponseEvent};
    use pretty_assertions::assert_eq;

    #[test]
    fn test_election_follower_to_candidate() -> Result<(), RaftError> {
        let (node_to_peers_tx, mut peers_from_node_tx) = mpsc::unbounded_channel();

        let mut node = RaftNode {
            id: "node1".to_string(),
            peers: vec!["node2".to_string(), "node3".to_string()],
            log: RaftLog::new(),
            current_term: 1,
            voted_for: None,
            commit_index: 0,
            commit_term: 0,
            next_index: Default::default(),
            match_index: Default::default(),
            state: NodeState::Follower,
            node_to_peers_tx,
            elapsed_ticks_since_last_heartbeat: 0,
            election_timeout_ticks: 1,
            votes: 1,
            cluster_ready: true,
        };

        //Since the difference between elapsed_ticks_since_last_heartbeat and election_timeout_ticks is 1, this should trigger an election
        node = node.tick()?;

        //Assert node state
        assert_eq!(node.state, NodeState::Candidate);
        assert_eq!(node.current_term, 2);
        assert_eq!(node.voted_for, Some("node1".to_string()));
        assert_eq!(node.votes, 1); //Candidate has voted for himself

        //Assert receipt of vote requests
        while let Some(Some(request)) = peers_from_node_tx.recv().now_or_never() {
            if let PeerVotesRequestEvent(request) = request {
                assert_eq!(
                    request,
                    RequestVoteRequest {
                        to: "ALL".to_string(), //FIXME
                        term: 2,
                        candidate_id: "node1".to_string(),
                        last_log_index: -1,
                        last_log_term: -1,
                    }
                );
            } else {
                panic!("Unexpected failure in testcase")
            }
        }
        Ok(())
    }

    #[test]
    fn test_election_candidate_to_leader() -> Result<(), RaftError> {
        let (node_to_peers_tx, mut peers_from_node_tx) = mpsc::unbounded_channel();
        let node_id = "node1".to_string();
        let mut node = RaftNode {
            id: node_id.clone(),
            peers: vec!["node2".to_string(), "node3".to_string()],
            log: RaftLog::new(),
            current_term: 1,
            voted_for: Some(node_id.clone()),
            commit_index: 0,
            commit_term: 0,
            next_index: Default::default(),
            match_index: Default::default(),
            state: NodeState::Candidate,
            node_to_peers_tx,
            elapsed_ticks_since_last_heartbeat: 0,
            election_timeout_ticks: 5,
            votes: 0,
            cluster_ready: true,
        };

        let peer_vote_responses = PeerVotesResponseEvent(vec![
            RequestVoteResponse {
                from: "node2".to_string(),
                term: 1,
                vote_granted: true,
            },
            RequestVoteResponse {
                from: "node3".to_string(),
                term: 1,
                vote_granted: true,
            },
        ]);

        node = node.step((peer_vote_responses, None))?;

        //Assert node state
        assert_eq!(node.state, NodeState::Leader);
        assert_eq!(node.current_term, 1);
        assert_eq!(node.voted_for, Some(node_id));
        /*for _ in 0..=HEARTBEAT_TICKS {
            node = node.tick()?;
        }*/
        let expected_heartbeats = vec![
            AppendEntriesRequest {
                to: "node2".to_string(),
                term: 1,
                leader_id: "node1".to_string(),
                prev_log_index: -1,
                prev_log_term: -1,
                entries: vec![],
                leader_commit_index: 0,
                leader_commit_term: 0,
            },
            AppendEntriesRequest {
                to: "node3".to_string(),
                term: 1,
                leader_id: "node1".to_string(),
                prev_log_index: -1,
                prev_log_term: -1,
                entries: vec![],
                leader_commit_index: 0,
                leader_commit_term: 0,
            },
        ];
        let mut actual_heartbeats = vec![];
        //Assert receipt of heartbeats
        while let Some(Some(request)) = peers_from_node_tx.recv().now_or_never() {
            if let RaftEvent::AppendEntriesRequestEvent(request) = request {
                actual_heartbeats.push(request);
            } else {
                panic!("Unexpected failure in testcase")
            }
        }

        assert_eq!(expected_heartbeats, actual_heartbeats);
        Ok(())
    }

    #[test]
    fn test_election_candidate_to_follower_on_receiving_append_entries() -> Result<(), RaftError> {
        let (node_to_peers_tx, _peers_from_node_tx) = mpsc::unbounded_channel();
        let node_id = "node1".to_string();

        let mut node = RaftNode {
            id: node_id.clone(),
            peers: vec!["node2".to_string()],
            log: RaftLog::new(),
            current_term: 1,
            voted_for: Some(node_id.clone()),
            commit_index: 0,
            commit_term: 0,
            next_index: Default::default(),
            match_index: Default::default(),
            state: NodeState::Candidate,
            node_to_peers_tx,
            elapsed_ticks_since_last_heartbeat: 0,
            election_timeout_ticks: 0,
            votes: 0,
            cluster_ready: true,
        };

        let append_entries_request = RaftEvent::AppendEntriesRequestEvent(AppendEntriesRequest {
            to: "node1".to_string(),
            term: 2,
            leader_id: "node2".to_string(),
            prev_log_index: 0,
            prev_log_term: 0,
            entries: vec![],
            leader_commit_index: 0,
            leader_commit_term: 0,
        });

        let (tx, mut rx) = oneshot::channel();
        node = node.step((append_entries_request, Some(tx)))?;

        //Assert node state
        assert_eq!(node.state, NodeState::Follower);
        assert_eq!(node.current_term, 2);
        assert_eq!(node.voted_for, Some("node2".to_string()));

        //Assert receipt of response
        if let Ok(response) = rx.try_recv() {
            if let Ok(AppendEntriesResponseEvent(response)) = response {
                assert_eq!(
                    response,
                    AppendEntriesResponse {
                        from: "node1".to_string(),
                        term: 2,
                        success: true,
                        last_applied_index: node.last_applied_index(),
                    }
                );
            } else {
                panic!("Unexpected failure in testcase")
            }
        } else {
            panic!("Unexpected failure in testcase")
        }
        Ok(())
    }


    #[test]
    fn test_election_candidate_to_follower_if_votes_not_granted() -> Result<(), RaftError> {
        let (node_to_peers_tx, _peers_from_node_tx) = mpsc::unbounded_channel();
        let node_id = "node1".to_string();
        let mut node = RaftNode {
            id: node_id.clone(),
            peers: vec!["node2".to_string(), "node3".to_string()],
            log: RaftLog::new(),
            current_term: 1,
            voted_for: Some(node_id.clone()),
            commit_index: 0,
            commit_term: 0,
            next_index: Default::default(),
            match_index: Default::default(),
            state: NodeState::Candidate,
            node_to_peers_tx,
            elapsed_ticks_since_last_heartbeat: 0,
            election_timeout_ticks: 1,
            votes: 0,
            cluster_ready: true,
        };

        let peer_votes_response = PeerVotesResponseEvent(vec![
            RequestVoteResponse {
                from: "node2".to_string(),
                term: 2,
                vote_granted: false,
            },
            RequestVoteResponse {
                from: "node3".to_string(),
                term: 2,
                vote_granted: false,
            },
        ]);

        node = node.step((peer_votes_response, None))?;

        //Assert node state
        assert_eq!(node.state, NodeState::Follower);
        assert_eq!(node.votes, 0);
        assert_eq!(node.current_term, 1);
        assert_eq!(node.voted_for, None);

        Ok(())
    }

    #[test]
    fn test_election_leader_to_follower_if_append_entries_response_has_greater_term() -> Result<(), RaftError>
    {
        let (node_to_peers_tx, _peers_from_node_tx) = mpsc::unbounded_channel();
        let node_id = "node1".to_string();
        let mut node = RaftNode {
            id: node_id.clone(),
            peers: vec!["node2".to_string(), "node3".to_string()],
            log: RaftLog::new(),
            current_term: 1,
            voted_for: Some(node_id.clone()),
            commit_index: 0,
            commit_term: 0,
            next_index: Default::default(),
            match_index: Default::default(),
            state: NodeState::Leader,
            node_to_peers_tx,
            elapsed_ticks_since_last_heartbeat: 0,
            election_timeout_ticks: 0,
            votes: 0,
            cluster_ready: true,
        };

        let append_entries_response = RaftEvent::AppendEntriesResponseEvent(AppendEntriesResponse {
            from: "node2".to_string(),
            term: 2,
            success: true,
            last_applied_index: node.last_applied_index(),
        });

        node = node.step((append_entries_response, None))?;
        //Assert node state
        assert_eq!(node.state, NodeState::Follower);
        assert_eq!(node.current_term, 2);
        assert_eq!(node.voted_for, None);

        Ok(())
    }

    #[tokio::test]
    async fn test_election_follower_to_not_grant_vote_if_req_term_is_lower() -> Result<(), RaftError> {
        let (node_to_peers_tx, _peers_from_node_tx) = mpsc::unbounded_channel();
        let node_id = "node1".to_string();
        let mut node = RaftNode {
            id: node_id.clone(),
            peers: vec!["node2".to_string(), "node3".to_string()],
            log: RaftLog::new(),
            current_term: 2,
            voted_for: Some(node_id.clone()),
            commit_index: 0,
            commit_term: 0,
            next_index: Default::default(),
            match_index: Default::default(),
            state: NodeState::Follower,
            node_to_peers_tx,
            elapsed_ticks_since_last_heartbeat: 0,
            election_timeout_ticks: 0,
            votes: 0,
            cluster_ready: true,
        };

        let request_vote = PeerVotesRequestEvent(RequestVoteRequest {
            to: "ALL".to_string(), //FIXME
            term: 1,
            candidate_id: "node2".to_string(),
            last_log_index: 0,
            last_log_term: 0,
        });

        let (tx, rx) = oneshot::channel();
        node = node.step((request_vote, Some(tx)))?;

        if let Ok(response) = rx.await {
            if let Ok(RequestVoteResponseEvent(response)) = response {
                assert_eq!(
                    response,
                    RequestVoteResponse {
                        from: "node1".to_string(),
                        term: 1,
                        vote_granted: false,
                    }
                );
            } else {
                panic!("Unexpected failure in testcase")
            }
        }
        //Assert node state
        assert_eq!(node.state, NodeState::Follower);
        assert_eq!(node.current_term, 2);
        assert_eq!(node.voted_for, Some(node_id));

        Ok(())
    }


    #[test]
    fn test_log_replication_leader_to_send_append_entries_with_logs() -> Result<(), RaftError> {
        let (node_to_peers_tx, mut peers_from_node_tx) = mpsc::unbounded_channel();
        let node_id = "node1".to_string();
        let mut log = RaftLog::new();
        let log_entry = LogEntry {
            index: 1,
            term: 1,
            command: "test".to_string(),
        };
        log.append(log_entry.clone());

        let mut node = RaftNode {
            id: node_id.clone(),
            peers: vec!["node2".to_string()],
            log,
            current_term: 1,
            voted_for: Some(node_id.clone()),
            commit_index: 0,
            commit_term: 0,
            next_index: Default::default(),
            match_index: Default::default(),
            state: NodeState::Leader,
            node_to_peers_tx,
            elapsed_ticks_since_last_heartbeat: 0,
            election_timeout_ticks: 0,
            votes: 0,
            cluster_ready: true,
        };

        for _ in 0..=HEARTBEAT_TICKS {
            node = node.tick()?;
        }

        if let Some(Some(request)) = peers_from_node_tx.recv().now_or_never() {
            if let RaftEvent::AppendEntriesRequestEvent(request) = request {
                assert_eq!(
                    request,
                    AppendEntriesRequest {
                        to: "node2".to_string(),
                        term: 1,
                        leader_id: node_id,
                        prev_log_index: -1,
                        prev_log_term: -1,
                        entries: vec![log_entry],
                        leader_commit_index: 0,
                        leader_commit_term: 0,
                    }
                );
            } else {
                panic!("Unexpected failure in testcase")
            }
        } else {
            panic!("Unexpected failure in testcase")
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_log_replication_leader_to_update_indexes_from_append_entries_responses() -> Result<(), RaftError> {
        let (node_to_peers_tx, _peers_from_node_tx) = mpsc::unbounded_channel();
        let node_id = "node1".to_string();
        let mut log = RaftLog::new();
        let log_entries = vec![
            LogEntry {
                index: 0,
                term: 1,
                command: "test1".to_string(),
            },
            LogEntry {
                index: 1,
                term: 1,
                command: "test2".to_string(),
            }];

        let _ = log_entries.into_iter().for_each(|entry| log.append(entry));

        let mut node = RaftNode {
            id: node_id.clone(),
            peers: vec!["node2".to_string(), "node3".to_string()],
            log,
            current_term: 1,
            voted_for: Some(node_id.clone()),
            commit_index: 0,
            commit_term: 0,
            next_index: Default::default(),
            match_index: Default::default(),
            state: NodeState::Leader,
            node_to_peers_tx,
            elapsed_ticks_since_last_heartbeat: 0,
            election_timeout_ticks: 0,
            votes: 0,
            cluster_ready: true,
        };

        node = node.step((RaftEvent::AppendEntriesResponseEvent(AppendEntriesResponse {
            from: "node2".to_string(),
            term: 1,
            success: true,
            last_applied_index: 1,
        }), None))?;

        node = node.step((RaftEvent::AppendEntriesResponseEvent(AppendEntriesResponse {
            from: "node3".to_string(),
            term: 1,
            success: true,
            last_applied_index: 1,
        }), None))?;

        //Assert node state
        assert_eq!(node.state, NodeState::Leader);
        assert_eq!(node.current_term, 1);
        assert_eq!(*node.next_index.get("node2").unwrap(), 2);
        assert_eq!(*node.match_index.get("node2").unwrap(), 1);
        assert_eq!(*node.next_index.get("node3").unwrap(), 2);
        assert_eq!(*node.match_index.get("node3").unwrap(), 1);
        assert_eq!(node.commit_index, 1);

        Ok(())
    }

    //Case 1
    #[tokio::test]
    async fn test_log_replication_follower_first_entry() -> Result<(), RaftError> {
        let (node_to_peers_tx, _peers_from_node_tx) = mpsc::unbounded_channel();
        let node_id = "node2".to_string();

        let log_entries = vec![
            LogEntry {
                index: 0,
                term: 1,
                command: "test1".to_string(),
            },
            LogEntry {
                index: 1,
                term: 1,
                command: "test2".to_string(),
            }];

        let mut node = RaftNode {
            id: node_id.clone(),
            peers: vec!["node1".to_string()],
            log: RaftLog::new(),
            current_term: 1,
            voted_for: Some("node1".to_string()),
            commit_index: -1,
            commit_term: 0,
            next_index: Default::default(),
            match_index: Default::default(),
            state: NodeState::Follower,
            node_to_peers_tx,
            elapsed_ticks_since_last_heartbeat: 0,
            election_timeout_ticks: 5,
            votes: 0,
            cluster_ready: true,
        };

        let (tx, mut rx) = oneshot::channel();
        node = node.step((RaftEvent::AppendEntriesRequestEvent(AppendEntriesRequest {
            to: node_id,
            term: 1,
            leader_id: "node1".to_string(),
            prev_log_index: -1,
            prev_log_term: -1,
            entries: log_entries,
            leader_commit_index: 0,
            leader_commit_term: 0,
        }), Some(tx)))?;

        //Assert node state
        assert_eq!(node.state, NodeState::Follower);
        assert_eq!(node.current_term, 1);
        assert_eq!(node.log.len(), 2);
        assert_eq!(node.last_applied_index(), 1);

        //Assert emitted response
        if let Ok(response) = rx.try_recv() {
            if let Ok(AppendEntriesResponseEvent(response)) = response {
                assert_eq!(
                    response,
                    AppendEntriesResponse {
                        from: "node2".to_string(),
                        term: 1,
                        success: true,
                        last_applied_index: node.last_applied_index(),
                    }
                );
            } else {
                panic!("Unexpected failure in testcase")
            }
        } else {
            panic!("Unexpected failure in testcase")
        }
        Ok(())
    }

    //Case 2
    #[tokio::test]
    async fn test_log_replication_follower_failure_overreached_append_entry() -> Result<(), RaftError> {
        let (node_to_peers_tx, _peers_from_node_tx) = mpsc::unbounded_channel();
        let node_id = "node2".to_string();
        let mut log = RaftLog::new();
        let follower_log_entries = vec![
            LogEntry {
                index: 0,
                term: 1,
                command: "test1".to_string(),
            },
            LogEntry {
                index: 1,
                term: 2,
                command: "index1_term2".to_string(),
            }];

        let _ = follower_log_entries.iter().for_each(|entry| log.append(entry.clone()));

        let mut node = RaftNode {
            id: node_id.clone(),
            peers: vec!["node1".to_string()],
            log,
            current_term: 1,
            voted_for: Some("node1".to_string()),
            commit_index: -1,
            commit_term: 0,
            next_index: Default::default(),
            match_index: Default::default(),
            state: NodeState::Follower,
            node_to_peers_tx,
            elapsed_ticks_since_last_heartbeat: 0,
            election_timeout_ticks: 5,
            votes: 0,
            cluster_ready: true,
        };

        let leader_log_entries = vec![
            LogEntry {
                index: 1,
                term: 1,
                command: "index1_term1".to_string(),
            }
        ];

        let (tx, mut rx) = oneshot::channel();
        node = node.step((RaftEvent::AppendEntriesRequestEvent(AppendEntriesRequest {
            to: node_id,
            term: 1,
            leader_id: "node1".to_string(),
            prev_log_index: 1,
            prev_log_term: 1,
            entries: leader_log_entries,
            leader_commit_index: 0,
            leader_commit_term: 0,
        }), Some(tx)))?;

        //Assert node state
        assert_eq!(node.state, NodeState::Follower);
        assert_eq!(node.current_term, 1);
        assert_eq!(node.last_applied_index(), 1);
        assert_eq!(node.log.len(), 2);
        assert_eq!(node.log.get(1), Some(&LogEntry {
            index: 1,
            term: 2,
            command: "index1_term2".to_string(),
        }));
        //How about commit index?

        //Assert emitted response
        if let Ok(response) = rx.try_recv() {
            if let Ok(AppendEntriesResponseEvent(response)) = response {
                assert_eq!(
                    response,
                    AppendEntriesResponse {
                        from: "node2".to_string(),
                        term: 1,
                        success: false,
                        last_applied_index: node.last_applied_index() - 1,//Decrements and retries
                    }
                );
            } else {
                panic!("Unexpected failure in testcase")
            }
        } else {
            panic!("Unexpected failure in testcase")
        }
        Ok(())
    }

    //Case 3
    #[tokio::test]
    async fn test_log_replication_follower_non_first_entry() -> Result<(), RaftError> {
        let (node_to_peers_tx, _peers_from_node_tx) = mpsc::unbounded_channel();
        let node_id = "node2".to_string();
        let mut log = RaftLog::new();
        let follower_log_entries = vec![
            LogEntry {
                index: 0,
                term: 1,
                command: "test1".to_string(),
            },
            LogEntry {
                index: 1,
                term: 1,
                command: "test2".to_string(),
            }];

        let _ = follower_log_entries.iter().for_each(|entry| log.append(entry.clone()));

        let mut node = RaftNode {
            id: node_id.clone(),
            peers: vec!["node1".to_string()],
            log,
            current_term: 1,
            voted_for: Some("node1".to_string()),
            commit_index: -1,
            commit_term: 0,
            next_index: Default::default(),
            match_index: Default::default(),
            state: NodeState::Follower,
            node_to_peers_tx,
            elapsed_ticks_since_last_heartbeat: 0,
            election_timeout_ticks: 5,
            votes: 0,
            cluster_ready: true,
        };

        let leader_log_entries = vec![
            LogEntry {
                index: 2,
                term: 1,
                command: "test3".to_string(),
            }
        ];

        let (tx, mut rx) = oneshot::channel();
        node = node.step((RaftEvent::AppendEntriesRequestEvent(AppendEntriesRequest {
            to: node_id,
            term: 1,
            leader_id: "node1".to_string(),
            prev_log_index: 1,
            prev_log_term: 1,
            entries: leader_log_entries,
            leader_commit_index: 0,
            leader_commit_term: 0,
        }), Some(tx)))?;

        //Assert node state
        assert_eq!(node.state, NodeState::Follower);
        assert_eq!(node.current_term, 1);
        assert_eq!(node.last_applied_index(), 2);
        assert_eq!(node.log.len(), 3);
        //How about commit index?

        //Assert emitted response
        if let Ok(response) = rx.try_recv() {
            if let Ok(AppendEntriesResponseEvent(response)) = response {
                assert_eq!(
                    response,
                    AppendEntriesResponse {
                        from: "node2".to_string(),
                        term: 1,
                        success: true,
                        last_applied_index: node.last_applied_index(),
                    }
                );
            } else {
                panic!("Unexpected failure in testcase")
            }
        } else {
            panic!("Unexpected failure in testcase")
        }
        Ok(())
    }
}

//TODO - Let's come to this later
//Reference: https://hoverbear.org/blog/rust-state-machine-pattern/
/*
struct Leader {}
struct Candidate {}
struct Follower {}

impl RaftNode<Leader> {
    fn new() -> Self {
        RaftNode {
            id: "",
            current_term: 0,
            voted_for: "",
            commit_index: 0,
                        state: Leader,
        }
    }
}*/
