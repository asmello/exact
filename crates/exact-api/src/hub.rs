// In-process registry of online runners. The WS endpoint registers each
// runner here on Hello; the dispatcher reads from it to find a runner
// matching a submission's board.

use std::sync::Arc;

use dashmap::DashMap;
use exact_proto::{Board, ServerToRunner};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct RunnerEntry {
    pub device_id: String,
    pub board: Board,
    pub cclk_hz: u64,
    pub synthetic: bool,
    /// Send-side of the WS write loop. Cloned per dispatch.
    pub tx: mpsc::UnboundedSender<ServerToRunner>,
}

#[derive(Default)]
pub struct RunnerHub {
    online: DashMap<String, RunnerEntry>,
}

impl RunnerHub {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn insert(&self, entry: RunnerEntry) {
        self.online.insert(entry.device_id.clone(), entry);
    }

    pub fn remove(&self, device_id: &str) {
        self.online.remove(device_id);
    }

    pub fn get(&self, device_id: &str) -> Option<RunnerEntry> {
        self.online.get(device_id).map(|r| r.clone())
    }

    pub fn find_for_board(&self, board: Board) -> Option<RunnerEntry> {
        self.online
            .iter()
            .find(|e| e.value().board == board)
            .map(|e| e.value().clone())
    }

    pub fn list(&self) -> Vec<RunnerEntry> {
        self.online.iter().map(|e| e.value().clone()).collect()
    }
}
