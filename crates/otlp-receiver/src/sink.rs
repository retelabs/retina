//! Where validated events go once converted. Dossier step 2 scope is "accepte,
//! valide, persiste brut" — the real persistence backend lands in step 3
//! (crates/clickhouse-sink). This trait is the seam.
//!
//! Async + batch + fallible, not "one event at a time, infallible" as it was
//! before step 3: docs/interfaces/clickhouse-schema.md establishes that the
//! `clickhouse` driver commits a whole `Insert` or none of it (`end()` must
//! be called or the insert is aborted) — there's no per-row guarantee inside
//! one batch, so the trait has to expose that shape rather than pretend
//! writes are independent and infallible.

use std::sync::Mutex;

use crate::convert::ConvertedEvent;

pub trait SpanSink: Send + Sync {
    type Error: std::fmt::Display;

    /// Persists every event or none of them — callers must treat a failure
    /// as "the whole batch wasn't persisted", not attempt to guess which
    /// events made it through.
    fn accept_batch(
        &self,
        events: Vec<ConvertedEvent>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// Placeholder sink for tests and for running the receiver before step 3's
/// real backend is wired in. Not meant for production use — unbounded
/// growth, no persistence.
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
    type Error = std::convert::Infallible;

    async fn accept_batch(&self, events: Vec<ConvertedEvent>) -> Result<(), Self::Error> {
        self.events
            .lock()
            .expect("sink mutex poisoned")
            .extend(events);
        Ok(())
    }
}
