use tabled::Tabled;
use crate::rpc::rpc_server::raft::LogEntry;

#[derive(Debug, Tabled)]
pub struct DisplayableLogEntry {
    index: i64,
    term: i32,
    command: String,
}

impl DisplayableLogEntry {
    pub fn formatted(log_entries: &[LogEntry]) -> Vec<DisplayableLogEntry> {
        let mut entries = Vec::new();
        for entry in log_entries.iter() {
            let command = entry.command.clone();
            entries.push(DisplayableLogEntry {
                index: entry.index,
                term: entry.term,
                command,
            });
        }
        entries
    }
}