pub mod node;
pub mod server;

//TODO - Need to come from a config
pub const TICK_INTERVAL_MS: u64 = 1000;
pub const ELECTION_MIN_TIMEOUT: u64 = 7;
pub const ELECTION_MAX_TIMEOUT: u64 = 10;

