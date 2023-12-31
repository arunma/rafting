use serde_derive::{Deserialize, Serialize};

pub mod web_server;

#[derive(Debug, Clone)]
pub enum ClientEvent {
    CommandRequestEvent(SetCommand),
    CommandResponseEvent(bool),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetCommand {
    key: String,
    value: String,
}
