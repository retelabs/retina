//! JSON-facing shapes for the HTTP API — a deliberate adaptation layer, same
//! reasoning as every other boundary in this kernel (dossier section 2.1):
//! `SpanRow` (ClickHouse storage shape) is not the wire shape. Trace/span ids
//! are hex strings here, matching the OTLP/JSON convention already used
//! upstream (docs/interfaces/otlp-ingestion.md) rather than inventing a new
//! encoding.
//!
//! `Deserialize` added 2026-08-17 alongside `Serialize`: `crates/tui`
//! depends on this crate as a library and parses these exact types back out
//! of the HTTP responses it receives — one shared definition of the wire
//! shape instead of a second copy that could drift from this one.

use std::collections::HashMap;

use clickhouse_sink::SpanRow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpanDto {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub kind: String,
    pub start_time_unix_nano: u64,
    pub end_time_unix_nano: u64,
    pub status_code: String,
    pub status_message: String,
    pub error_type: Option<String>,
    pub operation_name: String,
    pub provider_name: Option<String>,
    pub request_model: Option<String>,
    pub response_model: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub finish_reasons: Vec<String>,
    pub conversation_id: Option<String>,
    pub cost_usd: Option<f64>,
    pub tool_name: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_type: Option<String>,
    pub tool_description: Option<String>,
    pub agent_invocation_kind: Option<String>,
    pub agent_name: Option<String>,
    pub agent_id: Option<String>,
    pub agent_description: Option<String>,
    pub agent_version: Option<String>,
    pub extra_attributes: HashMap<String, String>,
}

/// Ticks stored via `DateTime64(9)` are always non-negative here — the only
/// way a row exists in `spans` is through `row::nanos_to_i64`, which already
/// rejected negative/overflowing values before the insert
/// (docs/interfaces/clickhouse-schema.md). This is that invariant made
/// explicit rather than a silent `as u64`.
pub(crate) fn ticks_to_nanos(field: &'static str, ticks: i64) -> u64 {
    u64::try_from(ticks)
        .unwrap_or_else(|_| panic!("invariant violated: negative DateTime64 ticks read back for `{field}` ({ticks}) — a row was written without going through row::nanos_to_i64"))
}

impl From<SpanRow> for SpanDto {
    fn from(row: SpanRow) -> Self {
        SpanDto {
            trace_id: hex::encode(row.trace_id),
            span_id: hex::encode(row.span_id),
            parent_span_id: row.parent_span_id.map(hex::encode),
            kind: row.kind,
            start_time_unix_nano: ticks_to_nanos("start_time", row.start_time),
            end_time_unix_nano: ticks_to_nanos("end_time", row.end_time),
            status_code: row.status_code,
            status_message: row.status_message,
            error_type: row.error_type,
            operation_name: row.operation_name,
            provider_name: row.provider_name,
            request_model: row.request_model,
            response_model: row.response_model,
            input_tokens: row.input_tokens,
            output_tokens: row.output_tokens,
            cache_read_input_tokens: row.cache_read_input_tokens,
            cache_creation_input_tokens: row.cache_creation_input_tokens,
            finish_reasons: row.finish_reasons,
            conversation_id: row.conversation_id,
            cost_usd: row.cost_usd,
            tool_name: row.tool_name,
            tool_call_id: row.tool_call_id,
            tool_type: row.tool_type,
            tool_description: row.tool_description,
            agent_invocation_kind: row.agent_invocation_kind,
            agent_name: row.agent_name,
            agent_id: row.agent_id,
            agent_description: row.agent_description,
            agent_version: row.agent_version,
            extra_attributes: row.extra_attributes,
        }
    }
}

/// One row of `GET /traces` — a trace summarized from its spans, not a
/// first-class stored entity (there is no `traces` table, see
/// docs/interfaces/query-api.md).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSummaryDto {
    pub trace_id: String,
    pub span_count: u64,
    pub start_time_unix_nano: u64,
    pub end_time_unix_nano: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KindMetricsDto {
    pub kind: String,
    pub span_count: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    /// `None` — deliberately *not* collapsed to `0.0` like the token totals
    /// above. Verified against a real ClickHouse (2026-08-17):
    /// `sum(Nullable(Float64))` returns `NULL` for an empty group *and* a
    /// group where every `cost_usd` is `NULL` — those are different
    /// situations ("no spans of this kind" / "no priced model in this
    /// kind's spans") that collapsing to `0.0` would make indistinguishable
    /// from "genuinely $0 spent", misleading anyone reading the summary.
    pub total_cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummaryDto {
    pub by_kind: Vec<KindMetricsDto>,
    /// Spans carrying at least one `plugin.warning` entry in
    /// `extra_attributes` — the governance/monitoring signal
    /// `crates/plugin-sink` produces (docs/interfaces/oncology-governance.md).
    /// Zero for every span until a plugin is actually wired into a running
    /// `kernel` (`crates/kernel/src/main.rs`).
    pub spans_with_warnings: u64,
}
