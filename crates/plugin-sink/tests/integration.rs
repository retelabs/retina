//! `PluginSink` wrapping a *real* `ClickHouseSink` — the exact combination
//! `crates/kernel` runs in production, only ever covered before by
//! `InMemorySink` unit tests (crate-internal) or by manually running the
//! full kernel binary. Not run by default `cargo test` — requires:
//!
//!   scripts/dev-clickhouse.sh up
//!   cargo test -p plugin-sink -- --ignored

use clickhouse::Client;
use clickhouse_sink::ClickHouseSink;
use kernel_model::{
    AgentInvocationKind, AgentRunEvent, AttributeValue, OperationName, SpanContext, SpanId,
    SpanStatus, TraceId,
};
use otlp_receiver::{ConvertedEvent, SpanSink};
use plugin_api::Plugin;
use plugin_fraudos::FraudosPlugin;
use plugin_medical::MedicalPlugin;
use plugin_sink::PluginSink;

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

fn test_client() -> Client {
    Client::default()
        .with_url(env_or("CLICKHOUSE_URL", "http://localhost:8123"))
        .with_user(env_or("CLICKHOUSE_USER", "dev"))
        .with_password(env_or("CLICKHOUSE_PASSWORD", "dev"))
        .with_database(env_or("CLICKHOUSE_DATABASE", "observability"))
}

async fn migrate(client: &Client) {
    clickhouse_sink::run_migrations(client).await.expect(
        "failed to apply migrations — is ClickHouse running? (scripts/dev-clickhouse.sh up)",
    );
}

fn sample_span(trace_id: TraceId, id_byte: u8) -> SpanContext {
    let t0 = recent_ns();
    SpanContext {
        trace_id,
        span_id: SpanId::try_from(&[id_byte; 8][..]).unwrap(),
        parent_span_id: None,
        start_time_unix_nano: t0 + 1_000,
        end_time_unix_nano: t0 + 1_500,
        status: SpanStatus::default(),
        error_type: None,
    }
}

fn hex_encode(bytes: [u8; 16]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

async fn fetch_row(client: &Client, trace_id: TraceId) -> clickhouse_sink::SpanRow {
    let rows: Vec<clickhouse_sink::SpanRow> = client
        .query("SELECT ?fields FROM spans WHERE trace_id = unhex(?)")
        .bind(hex_encode(trace_id.as_bytes()))
        .fetch_all()
        .await
        .expect("query should succeed");
    assert_eq!(rows.len(), 1, "expected exactly one row for this trace_id");
    rows.into_iter().next().unwrap()
}

#[tokio::test]
#[ignore = "requires `scripts/dev-clickhouse.sh up`"]
async fn fraudos_plugin_warning_lands_in_a_real_clickhouse_row() {
    let client = test_client();
    migrate(&client).await;

    let clickhouse_sink = ClickHouseSink::new(client.clone(), "spans");
    let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(FraudosPlugin), Box::new(MedicalPlugin)];
    let sink = PluginSink::new(clickhouse_sink, plugins);

    let trace_id = unique_trace_id(11);
    let event = ConvertedEvent::AgentRun(AgentRunEvent {
        span: sample_span(trace_id, 11),
        invocation_kind: AgentInvocationKind::Internal,
        operation_name: OperationName::InvokeAgent,
        agent_name: Some("fraud_investigator".to_string()),
        agent_id: None,
        agent_description: None,
        agent_version: None,
        request_model: None,
        provider_name: None,
        input_tokens: None,
        output_tokens: None,
        cache_read_input_tokens: None,
        cache_creation_input_tokens: None,
        conversation_id: None,
        // CONFIRMED_FRAUD with no transaction_id — should trigger
        // FraudosPlugin's warning, and MedicalPlugin should stay silent
        // (no oncology.* attributes here) even though it also runs.
        extra_attributes: vec![(
            "fraudos.final_decision".to_string(),
            AttributeValue::String("CONFIRMED_FRAUD".to_string()),
        )],
    });

    sink.accept_batch(vec![event])
        .await
        .expect("insert should succeed");

    let row = fetch_row(&client, trace_id).await;
    let attrs = &row.extra_attributes;

    let warning = attrs
        .get("plugin.warning")
        .expect("expected a plugin.warning entry");
    assert!(
        warning.contains("fraudos-plugin"),
        "warning should be attributed to fraudos-plugin: {warning}"
    );
    assert!(warning.contains("CONFIRMED_FRAUD"));
    assert_eq!(
        attrs.get("fraudos.requires_urgent_review"),
        Some(&"true".to_string())
    );
    // MedicalPlugin ran too (both plugins always run) but had nothing to say.
    assert!(!attrs.contains_key("oncology.awaiting_approval"));
}

#[tokio::test]
#[ignore = "requires `scripts/dev-clickhouse.sh up`"]
async fn medical_plugin_warning_lands_in_a_real_clickhouse_row() {
    let client = test_client();
    migrate(&client).await;

    let clickhouse_sink = ClickHouseSink::new(client.clone(), "spans");
    let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(FraudosPlugin), Box::new(MedicalPlugin)];
    let sink = PluginSink::new(clickhouse_sink, plugins);

    let trace_id = unique_trace_id(22);
    let event = ConvertedEvent::AgentRun(AgentRunEvent {
        span: sample_span(trace_id, 22),
        invocation_kind: AgentInvocationKind::Internal,
        operation_name: OperationName::InvokeAgent,
        agent_name: Some("oncology_pipeline".to_string()),
        agent_id: None,
        agent_description: None,
        agent_version: None,
        request_model: None,
        provider_name: None,
        input_tokens: None,
        output_tokens: None,
        cache_read_input_tokens: None,
        cache_creation_input_tokens: None,
        conversation_id: None,
        extra_attributes: vec![
            (
                "oncology.current_step".to_string(),
                AttributeValue::String("done".to_string()),
            ),
            (
                "oncology.hipaa_cleared".to_string(),
                AttributeValue::Bool(true),
            ),
            (
                "oncology.gdpr_cleared".to_string(),
                AttributeValue::Bool(true),
            ),
            (
                "oncology.submitted_by".to_string(),
                AttributeValue::String("dr.okafor".to_string()),
            ),
            // no approved_by — should trigger MedicalPlugin's warning
        ],
    });

    sink.accept_batch(vec![event])
        .await
        .expect("insert should succeed");

    let row = fetch_row(&client, trace_id).await;
    let attrs = &row.extra_attributes;

    let warning = attrs
        .get("plugin.warning")
        .expect("expected a plugin.warning entry");
    assert!(
        warning.contains("medical-plugin"),
        "warning should be attributed to medical-plugin: {warning}"
    );
    assert!(warning.contains("HITL"));
    assert_eq!(
        attrs.get("oncology.awaiting_approval"),
        Some(&"true".to_string())
    );
    // FraudosPlugin ran too but had nothing to say (no fraudos.* attributes).
}
