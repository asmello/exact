// In-process pub/sub for typed submission deltas.
//
// Producers (build worker, dispatcher, runner WS) publish concrete event
// variants; the SSE handler forwards each to the browser as a named SSE
// event with a JSON payload. Snapshots-on-connect happen out-of-band in
// the SSE handler — the bus only carries deltas.

use std::sync::Arc;

use dashmap::DashMap;
use exact_proto::b64_bytes_opt;
use serde::Serialize;
use tokio::sync::broadcast;
use uuid::Uuid;

const CHANNEL_CAPACITY: usize = 64;

/// Per-submission state delta. Each variant maps 1:1 to a named SSE event.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SubmissionEvent {
    /// Lifecycle status change (queued → building → ready → running).
    /// Terminal statuses (done / failed) use `Finalized` / `Failed`.
    Status { status: String },

    /// One test case completed. Replaces or inserts into the local
    /// case_results array keyed by `case_ord`.
    CaseResult {
        case_ord: i32,
        status: String,
        exit_code: Option<i32>,
        cycles: Option<i64>,
        #[serde(with = "b64_bytes_opt")]
        output: Option<Vec<u8>>,
        passed: Option<bool>,
        synthetic: bool,
    },

    /// Run finished cleanly. Carries the aggregates the UI surfaces.
    Finalized {
        total_cycles: Option<i64>,
        passed: i32,
        total_cases: i32,
    },

    /// Terminal failure (build or runtime). `log` mirrors the
    /// submissions.build_log column.
    Failed { log: String },
}

#[derive(Default)]
pub struct EventBus {
    inner: DashMap<Uuid, broadcast::Sender<SubmissionEvent>>,
}

impl EventBus {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn subscribe(&self, id: Uuid) -> broadcast::Receiver<SubmissionEvent> {
        let entry = self
            .inner
            .entry(id)
            .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0);
        entry.subscribe()
    }

    /// Fire-and-forget. Drops silently if no subscribers are listening.
    pub fn publish(&self, id: Uuid, event: SubmissionEvent) {
        if let Some(tx) = self.inner.get(&id) {
            let _ = tx.send(event);
        }
    }
}
