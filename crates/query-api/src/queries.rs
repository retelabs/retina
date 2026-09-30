//! SQL against the `spans` table (migrations/0001_create_spans.sql). Kept
//! separate from `routes.rs` so the queries are testable without spinning up
//! an HTTP server (see tests/integration.rs).

use clickhouse::{Client, Row};
use clickhouse_sink::SpanRow;
use serde::Deserialize;

use crate::dto::{KindMetricsDto, MetricsSummaryDto, TraceSummaryDto, ticks_to_nanos};

#[derive(Debug, Row, Deserialize)]
struct TraceSummaryRow {
    trace_id: [u8; 16],
    span_count: u64,
    trace_start: i64,
    trace_end: i64,
}

/// "List recent traces" (dossier step 4). There is no `traces`
/// table — a trace is derived on the fly by grouping `spans` by `trace_id`.
pub async fn list_recent_traces(
    client: &Client,
    limit: u64,
) -> clickhouse::error::Result<Vec<TraceSummaryDto>> {
    let rows: Vec<TraceSummaryRow> = client
        .query(
            "SELECT trace_id, count() AS span_count, min(start_time) AS trace_start, max(end_time) AS trace_end \
             FROM spans GROUP BY trace_id ORDER BY trace_start DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all()
        .await?;

    Ok(rows
        .into_iter()
        .map(|r| TraceSummaryDto {
            trace_id: hex::encode(r.trace_id),
            span_count: r.span_count,
            start_time_unix_nano: ticks_to_nanos("trace_start", r.trace_start),
            end_time_unix_nano: ticks_to_nanos("trace_end", r.trace_end),
        })
        .collect())
}

/// "Fetch a trace's tree" (dossier step 4) — returns a flat list
/// ordered by `start_time`, not a nested JSON tree. Each span already
/// carries `parent_span_id`, which is enough to reconstruct the tree
/// client-side; building and validating an actual nested structure
/// server-side (multiple roots, orphaned parents, cycles from malformed
/// input) is more than "no rich dashboard" calls for at the MVP.
///
/// Binds the hex string and lets ClickHouse's `unhex()` do the decoding,
/// rather than binding `[u8; 16]` directly — binding a fixed-size array hits
/// a real driver quirk (docs/interfaces/clickhouse-schema.md: it serializes
/// as a `Tuple` literal, which ClickHouse then refuses to compare against
/// `FixedString`). Verified against a real server before relying on it.
pub async fn get_trace_spans(
    client: &Client,
    trace_id_hex: &str,
) -> clickhouse::error::Result<Vec<SpanRow>> {
    client
        .query("SELECT ?fields FROM spans WHERE trace_id = unhex(?) ORDER BY start_time")
        .bind(trace_id_hex)
        .fetch_all()
        .await
}

#[derive(Debug, Row, Deserialize)]
struct KindMetricsRow {
    kind: String,
    span_count: u64,
    total_input_tokens: Option<u64>,
    total_output_tokens: Option<u64>,
    total_cost_usd: Option<f64>,
}

/// "Aggregate a few basic metrics" (dossier step 4) — span counts and
/// token totals per event kind. Deliberately not parameterized by time
/// range yet: no retention policy exists to bound the scan (dossier section
/// 4), so a time filter would be cosmetic rather than load-bearing at the
/// MVP's expected volume.
pub async fn metrics_summary(client: &Client) -> clickhouse::error::Result<MetricsSummaryDto> {
    let rows: Vec<KindMetricsRow> = client
        .query(
            "SELECT kind, count() AS span_count, sum(input_tokens) AS total_input_tokens, \
             sum(output_tokens) AS total_output_tokens, sum(cost_usd) AS total_cost_usd \
             FROM spans GROUP BY kind ORDER BY kind",
        )
        .fetch_all()
        .await?;

    // `mapContains` verified against a real local ClickHouse before use
    // (docs/interfaces/oncology-governance.md) — spans a plugin flagged via
    // crates/plugin-sink carry a `plugin.warning` entry in extra_attributes.
    let spans_with_warnings: u64 = client
        .query("SELECT countIf(mapContains(extra_attributes, 'plugin.warning')) FROM spans")
        .fetch_one()
        .await?;

    Ok(MetricsSummaryDto {
        by_kind: rows
            .into_iter()
            .map(|r| KindMetricsDto {
                kind: r.kind,
                span_count: r.span_count,
                total_input_tokens: r.total_input_tokens.unwrap_or(0),
                total_output_tokens: r.total_output_tokens.unwrap_or(0),
                total_cost_usd: r.total_cost_usd,
            })
            .collect(),
        spans_with_warnings,
    })
}
