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
        self.inner.push(entry);
    }

    pub fn get(&self, index: u64) -> Option<&LogEntry> {
        self.inner.get(index as usize)
    }

    pub fn get_from(&self, index: u64) -> Vec<LogEntry> {
        self.inner.iter().skip(index as usize).cloned().collect::<Vec<LogEntry>>()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    index: u64,
    term: u32,
    command: String,
}
