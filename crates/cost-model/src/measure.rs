//! Measures the real bytes per span against a real ClickHouse, not an
//! analytic estimate from the column types
//! (`docs/interfaces/clickhouse-schema.md`), which would ignore the real
//! compression (`LowCardinality`, column compression) and overestimate by
//! far. `system.parts` gives the compressed size actually on disk; checked
//! by reading `DESCRIBE TABLE system.parts` against a real server before
//! writing this query (`rows`, `data_compressed_bytes`, `database`,
//! `table`, `active` confirmed there, not guessed).

use clickhouse::Client;
use clickhouse::Row;
use serde::Deserialize;

#[derive(Row, Deserialize)]
struct PartsAggregate {
    rows: u64,
    compressed_bytes: u64,
}

/// `None` when the table has no row (nothing to measure) rather than a
/// division by zero disguised as a misleading `0.0`.
pub async fn measure_bytes_per_span(
    client: &Client,
    database: &str,
    table: &str,
) -> clickhouse::error::Result<Option<f64>> {
    let agg: PartsAggregate = client
        .query(
            "SELECT sum(rows) AS rows, sum(data_compressed_bytes) AS compressed_bytes \
             FROM system.parts WHERE database = ? AND table = ? AND active = 1",
        )
        .bind(database)
        .bind(table)
        .fetch_one()
        .await?;

    if agg.rows == 0 {
        return Ok(None);
    }
    Ok(Some(agg.compressed_bytes as f64 / agg.rows as f64))
}
