//! Where validated events go once converted. Dossier step 2 scope is "accepte,
//! valide, persiste brut" — the real persistence backend (ClickHouse or
//! managed equivalent) is step 3 and not decided yet (dossier section 5). This
//! trait is the seam: swapping the in-memory sink for a real one later
//! shouldn't require touching `service.rs` or `convert.rs`.

use std::sync::Mutex;

use crate::convert::ConvertedEvent;

pub trait SpanSink: Send + Sync {
    fn accept(&self, event: ConvertedEvent);
}

/// Placeholder sink for tests and for running the receiver before step 3
/// lands. Not meant for production use — unbounded growth, no persistence.
#[derive(Default)]
pub struct InMemorySink {
    events: Mutex<Vec<ConvertedEvent>>,
}

impl InMemorySink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn drain(&self) -> Vec<ConvertedEvent> {
        std::mem::take(&mut self.events.lock().expect("sink mutex poisoned"))
    }
}

impl SpanSink for InMemorySink {
    fn accept(&self, event: ConvertedEvent) {
        self.events.lock().expect("sink mutex poisoned").push(event);
    }
}
