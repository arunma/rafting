pub mod raft_node;
pub mod raft_server;
mod raft_log;
pub mod display;

pub const HEARTBEAT_TICKS: u64 = 2;
pub const ELECTION_MIN_TIMEOUT_TICKS: u64 = 7;
pub const ELECTION_MAX_TIMEOUT_TICKS: u64 = 10;

