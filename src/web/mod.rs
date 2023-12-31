use serde_derive::{Deserialize, Serialize};

pub mod web_server;

#[derive(Debug, Clone, Deserialize)]
struct Command(String);

#[derive(Debug, Clone)]
pub enum ClientEvent {
    CommandRequestEvent(Command),
    CommandResponseEvent(bool),
}
