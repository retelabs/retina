//! [`ClickHouseSink`] — the real [`SpanSink`] backend (dossier step 3),
//! replacing `otlp_receiver::InMemorySink`. Whole-batch-or-nothing per
//! `Export` call, matching the driver's `Insert` semantics
//! (docs/interfaces/clickhouse-schema.md).

use std::fmt;

use clickhouse::Client;
use otlp_receiver::{ConvertedEvent, SpanSink};

use crate::row::{RowConversionError, SpanRow};

#[derive(Debug)]
pub enum SinkError {
    Conversion(RowConversionError),
    Client(clickhouse::error::Error),
}

impl fmt::Display for SinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SinkError::Conversion(e) => write!(f, "{e}"),
            SinkError::Client(e) => write!(f, "clickhouse client error: {e}"),
        }
    }
}

impl std::error::Error for SinkError {}

pub struct ClickHouseSink {
    client: Client,
    table: String,
}

impl ClickHouseSink {
    pub fn new(client: Client, table: impl Into<String>) -> Self {
        Self {
            client,
            table: table.into(),
        }
    }
}

impl SpanSink for ClickHouseSink {
    type Error = SinkError;

    async fn accept_batch(&self, events: Vec<ConvertedEvent>) -> Result<(), Self::Error> {
        if events.is_empty() {
            return Ok(());
        }

        let mut insert = self
            .client
            .insert::<SpanRow>(&self.table)
            .await
            .map_err(SinkError::Client)?;
        for event in events {
            let row = SpanRow::try_from(event).map_err(SinkError::Conversion)?;
            insert.write(&row).await.map_err(SinkError::Client)?;
        }
        // Must be called or the whole insert is silently aborted (driver
        // contract, docs/interfaces/clickhouse-schema.md).
        insert.end().await.map_err(SinkError::Client)?;
        Ok(())
    }
}
