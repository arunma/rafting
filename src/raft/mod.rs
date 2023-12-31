pub mod raft_node;
pub mod raft_server;
mod raft_log;

//TODO - Need to source this from a config
//FIXME: Really slowing down for dev purposes
pub const TICK_INTERVAL_MS: u64 = 2000;
pub const HEARTBEAT_TICKS: u64 = 2;
pub const ELECTION_MIN_TIMEOUT_TICKS: u64 = 7;
pub const ELECTION_MAX_TIMEOUT_TICKS: u64 = 10;
