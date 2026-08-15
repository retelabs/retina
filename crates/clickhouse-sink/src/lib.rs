//! ClickHouse persistence for the observability kernel (dossier section 2.2,
//! étape 3). Schema and driver contract are documented in
//! docs/interfaces/clickhouse-schema.md — read that before touching
//! `row.rs` or `migrations/`.

pub mod migrate;
pub mod row;
pub mod sink;

pub use migrate::run_migrations;
pub use row::{RowConversionError, SpanRow};
pub use sink::{ClickHouseSink, SinkError};
