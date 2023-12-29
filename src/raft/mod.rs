pub mod node;
pub mod server;

//TODO - Need to source this from a config
//FIXME: Really slowing down for dev purposes
pub const TICK_INTERVAL_MS: u64 = 2000;
pub const HEARTBEAT_TICKS: u64 = 2;
pub const ELECTION_MIN_TIMEOUT: u64 = 7;
pub const ELECTION_MAX_TIMEOUT: u64 = 10;
