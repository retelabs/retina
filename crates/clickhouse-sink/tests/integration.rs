//! Runs against the real local ClickHouse (docker/docker-compose.clickhouse.yml).
//! Not run by default `cargo test` — requires:
//!
//!   scripts/dev-clickhouse.sh up
//!   cargo test -p clickhouse-sink -- --ignored
//!
//! This is the one place that actually proves the schema in
//! migrations/0001_create_spans.sql and the driver contract in
//! docs/interfaces/clickhouse-schema.md hold up against a real server, not
//! just against our own assumptions about the crate's API.

use clickhouse::Client;
use clickhouse_sink::{ClickHouseSink, SpanRow, run_migrations};
use kernel_model::{
    ModelCallEvent, OperationName, ProviderName, SpanContext, SpanId, SpanStatus, TokenCount,
    TraceId,
};
use otlp_receiver::{ConvertedEvent, SpanSink};

/// A trace id no earlier run used: rows are no longer purged between runs
/// (see `recent_ns`), so a fixed id would find the previous runs' rows too.
/// First byte `tag` keeps ids readable per test; the rest is the clock.
fn unique_trace_id(tag: u8) -> TraceId {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let mut bytes = nanos.to_be_bytes();
    bytes[0] = tag;
    TraceId::try_from(&bytes[..]).unwrap()
}

/// A start time in the recent past, in Unix nanoseconds. The `spans` table
/// drops rows 90 days after `start_time` (migration 0002): a fixture dated
/// 1970 is already expired when inserted and disappears at the next
/// background merge, so a read-back then races that merge (seen failing
/// with 0 rows on 2026-09-30).
fn recent_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
        - 60_000_000_000
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Reads connection details from the environment (falling back to the local
/// `scripts/dev-clickhouse.sh` defaults) rather than hardcoding
/// `localhost:8123` — CI runs this test inside a container where
/// ClickHouse is reachable via its service alias, not `localhost` (found by
/// reproducing the CI job locally after a hardcoded `localhost` passed
/// every local run but failed the first real pipeline).
fn test_client() -> Client {
    Client::default()
        .with_url(env_or("CLICKHOUSE_URL", "http://localhost:8123"))
        .with_user(env_or("CLICKHOUSE_USER", "dev"))
        .with_password(env_or("CLICKHOUSE_PASSWORD", "dev"))
        .with_database(env_or("CLICKHOUSE_DATABASE", "observability"))
}

#[tokio::test]
#[ignore = "requires `scripts/dev-clickhouse.sh up`"]
async fn insert_and_read_back_a_model_call_span() {
    let t0 = recent_ns();
    let client = test_client();

    run_migrations(&client).await.expect(
        "failed to apply migrations — is ClickHouse running? (scripts/dev-clickhouse.sh up)",
    );

    let trace_id = unique_trace_id(7);
    let span_id = SpanId::try_from(&[9u8; 8][..]).unwrap();

    let event = ConvertedEvent::ModelCall(ModelCallEvent {
        span: SpanContext {
            trace_id,
            span_id,
            parent_span_id: None,
            start_time_unix_nano: t0 + 1_000,
            end_time_unix_nano: t0 + 1_500,
            status: SpanStatus::default(),
            error_type: None,
        },
        provider_name: ProviderName::AwsBedrock,
        operation_name: OperationName::Chat,
        request_model: Some("claude".to_string()),
        response_model: None,
        input_tokens: Some(TokenCount::try_from(42).unwrap()),
        output_tokens: None,
        cache_read_input_tokens: None,
        cache_creation_input_tokens: None,
        finish_reasons: vec![],
        conversation_id: None,
        extra_attributes: vec![],
    });

    let sink = ClickHouseSink::new(client.clone(), "spans");
    sink.accept_batch(vec![event])
        .await
        .expect("insert should succeed");

    // Binding [u8; N] straight into a FixedString `?` placeholder does not
    // work with this driver version (it serializes as a Tuple, producing a
    // ClickHouse NO_COMMON_TYPE error) — filtering client-side sidesteps
    // that rather than guessing the right bind incantation. Worth
    // revisiting once the API layer (étape 4) needs parameterized lookups.
    let rows: Vec<SpanRow> = client
        .query("SELECT ?fields FROM spans")
        .fetch_all()
        .await
        .expect("query should succeed");

    let matches: Vec<_> = rows
        .iter()
        .filter(|r| r.trace_id == trace_id.as_bytes() && r.span_id == span_id.as_bytes())
        .collect();

    assert_eq!(
        matches.len(),
        1,
        "expected exactly one row for this trace_id/span_id"
    );
    assert_eq!(matches[0].provider_name.as_deref(), Some("aws.bedrock"));
    assert_eq!(matches[0].input_tokens, Some(42));
}
