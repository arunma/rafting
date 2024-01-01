use crate::rpc::rpc_server::raft::LogEntry;

#[derive(Debug)]
pub struct RaftLog {
    inner: Vec<LogEntry>,
}

impl RaftLog {
    pub fn new() -> Self {
        Self {
            inner: Vec::new()
        }
    }
    pub fn append(&mut self, entry: LogEntry) {
        self.inner.push(entry)
    }

    pub fn get(&self, index: usize) -> Option<&LogEntry> {
        self.inner.get(index)
    }

    pub fn get_from(&self, index: usize) -> Vec<LogEntry> {
        self.inner.iter().skip(index).cloned().collect::<Vec<LogEntry>>()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn last_log_index(&self) -> i64 {
        self.len() as i64 - 1
    }

    pub fn last_log_index_and_term(&self) -> (i64, i32) {
        self
            .get(self.last_log_index() as usize).map(|entry| (entry.index, entry.term))
            .unwrap_or((-1, -1))
    }

    pub fn truncate(&mut self, index: usize) {
        self.inner.truncate(index)
    }

    pub fn append_all(&mut self, entries: Vec<LogEntry>) {
        self.inner.extend(entries)
    }

    pub fn append_all_from(&mut self, entries: Vec<LogEntry>, index: usize) {
        self.inner.truncate(index);
        self.inner.extend(entries)
    }

    pub fn term_for_index(&self, index: usize) -> i32 {
        self.inner.get(index).map(|entry| entry.term).unwrap_or(-1)
    }
}