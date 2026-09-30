//! Runs against the real local ClickHouse (docker/docker-compose.clickhouse.yml).
//! Not run by default `cargo test` — requires:
//!
//!   scripts/dev-clickhouse.sh up
//!   cargo test -p query-api -- --ignored
//!
//! Uses `tower::ServiceExt::oneshot` to drive the axum `Router` in-process
//! (no real TCP listener, no HTTP client dependency needed) — the standard
//! way to integration-test an axum app.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use clickhouse::Client;
use clickhouse_sink::ClickHouseSink;
use http_body_util::BodyExt;
use kernel_model::{
    AgentInvocationKind, AgentRunEvent, ModelCallEvent, OperationName, ProviderName, SpanContext,
    SpanId, SpanStatus, TokenCount, TraceId,
};
use otlp_receiver::{ConvertedEvent, SpanSink};
use query_api::build_app;
use serde_json::Value;
use tower::ServiceExt;

const TEST_API_KEY: &str = "test-key";

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

fn authed_request(uri: impl AsRef<str>) -> Request<Body> {
    Request::builder()
        .uri(uri.as_ref())
        .header(header::AUTHORIZATION, format!("Bearer {TEST_API_KEY}"))
        .body(Body::empty())
        .unwrap()
}

/// See the identical comment in crates/clickhouse-sink/tests/integration.rs
/// — env-configurable so this also works against a CI service
/// (reachable by alias, not `localhost`), not just local Docker Compose.
fn test_client() -> Client {
    Client::default()
        .with_url(env_or("CLICKHOUSE_URL", "http://localhost:8123"))
        .with_user(env_or("CLICKHOUSE_USER", "dev"))
        .with_password(env_or("CLICKHOUSE_PASSWORD", "dev"))
        .with_database(env_or("CLICKHOUSE_DATABASE", "observability"))
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn seed(client: &Client, trace_id: TraceId) {
    let t0 = recent_ns();
    let model_call = ConvertedEvent::ModelCall(ModelCallEvent {
        span: SpanContext {
            trace_id,
            span_id: SpanId::try_from(&[1u8; 8][..]).unwrap(),
            parent_span_id: None,
            start_time_unix_nano: t0 + 1_000,
            end_time_unix_nano: t0 + 2_000,
            status: SpanStatus::default(),
            error_type: None,
        },
        provider_name: ProviderName::AwsBedrock,
        operation_name: OperationName::Chat,
        request_model: Some("claude".to_string()),
        response_model: None,
        input_tokens: Some(TokenCount::try_from(10).unwrap()),
        output_tokens: Some(TokenCount::try_from(20).unwrap()),
        cache_read_input_tokens: None,
        cache_creation_input_tokens: None,
        finish_reasons: vec![],
        conversation_id: None,
        extra_attributes: vec![],
    });

    let agent_run = ConvertedEvent::AgentRun(AgentRunEvent {
        span: SpanContext {
            trace_id,
            span_id: SpanId::try_from(&[2u8; 8][..]).unwrap(),
            parent_span_id: Some(SpanId::try_from(&[1u8; 8][..]).unwrap()),
            start_time_unix_nano: t0 + 500,
            end_time_unix_nano: t0 + 2_500,
            status: SpanStatus::default(),
            error_type: None,
        },
        invocation_kind: AgentInvocationKind::Internal,
        operation_name: OperationName::InvokeAgent,
        agent_name: Some("triage".to_string()),
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
        extra_attributes: vec![],
    });

    let sink = ClickHouseSink::new(client.clone(), "spans");
    sink.accept_batch(vec![model_call, agent_run])
        .await
        .expect("seed insert should succeed");
}

/// One priced `model_call` span, isolated from `seed()` above — mixing it
/// in there would break `get_trace_returns_both_spans_ordered_by_start_time`'s
/// `spans.len() == 2` assertion.
async fn seed_priced_model_call(client: &Client, trace_id: TraceId) {
    let t0 = recent_ns();
    let model_call = ConvertedEvent::ModelCall(ModelCallEvent {
        span: SpanContext {
            trace_id,
            span_id: SpanId::try_from(&[3u8; 8][..]).unwrap(),
            parent_span_id: None,
            start_time_unix_nano: t0 + 1_000,
            end_time_unix_nano: t0 + 2_000,
            status: SpanStatus::default(),
            error_type: None,
        },
        provider_name: ProviderName::OpenAi,
        operation_name: OperationName::Chat,
        request_model: Some("gpt-4o-mini".to_string()),
        response_model: Some("gpt-4o-mini".to_string()),
        input_tokens: Some(TokenCount::try_from(1_000_000).unwrap()),
        output_tokens: Some(TokenCount::try_from(1_000_000).unwrap()),
        cache_read_input_tokens: None,
        cache_creation_input_tokens: None,
        finish_reasons: vec![],
        conversation_id: None,
        extra_attributes: vec![],
    });

    let sink = ClickHouseSink::new(client.clone(), "spans");
    sink.accept_batch(vec![model_call])
        .await
        .expect("seed insert should succeed");
}

async fn setup_priced(id_byte: u8) -> (Router, String) {
    let client = test_client();
    clickhouse_sink::run_migrations(&client).await.expect(
        "failed to apply migrations — is ClickHouse running? (scripts/dev-clickhouse.sh up)",
    );

    let trace_id = unique_trace_id(id_byte);
    seed_priced_model_call(&client, trace_id).await;

    (
        build_app(client, vec![TEST_API_KEY.to_string()]),
        hex::encode(trace_id.as_bytes()),
    )
}

/// `id_byte` gives each test its own `trace_id` (`[id_byte; 16]`) — tests run
/// concurrently by default and share the one real ClickHouse table, so
/// reusing a fixed trace_id across tests causes cross-test row collisions
/// (seen first-hand: `get_trace` returned 10 spans instead of 2 before this
/// was parameterized, because 5 tests were all seeding the same trace_id in
/// parallel).
async fn setup(id_byte: u8) -> (Router, String) {
    let client = test_client();
    clickhouse_sink::run_migrations(&client).await.expect(
        "failed to apply migrations — is ClickHouse running? (scripts/dev-clickhouse.sh up)",
    );

    let trace_id = unique_trace_id(id_byte);
    seed(&client, trace_id).await;

    (
        build_app(client, vec![TEST_API_KEY.to_string()]),
        hex::encode(trace_id.as_bytes()),
    )
}

#[tokio::test]
#[ignore = "requires `scripts/dev-clickhouse.sh up`"]
async fn list_traces_includes_the_seeded_trace() {
    let (app, trace_id_hex) = setup(1).await;

    let response = app
        .oneshot(authed_request("/traces?limit=10"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let traces = body.as_array().unwrap();
    assert!(
        traces.iter().any(|t| t["trace_id"] == trace_id_hex),
        "expected {trace_id_hex} in {body}"
    );
}

#[tokio::test]
#[ignore = "requires `scripts/dev-clickhouse.sh up`"]
async fn get_trace_returns_both_spans_ordered_by_start_time() {
    let (app, trace_id_hex) = setup(2).await;

    let response = app
        .oneshot(authed_request(format!("/traces/{trace_id_hex}")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let spans = body.as_array().unwrap();
    assert_eq!(spans.len(), 2);
    // agent_run started at 500, model_call at 1000 — ORDER BY start_time
    assert_eq!(spans[0]["kind"], "agent_run");
    assert_eq!(spans[1]["kind"], "model_call");
    assert_eq!(spans[1]["input_tokens"], 10);
}

#[tokio::test]
#[ignore = "requires `scripts/dev-clickhouse.sh up`"]
async fn get_trace_rejects_malformed_trace_id() {
    let (app, _) = setup(3).await;

    let response = app
        .oneshot(authed_request("/traces/not-hex"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[ignore = "requires `scripts/dev-clickhouse.sh up`"]
async fn get_trace_returns_404_for_unknown_but_well_formed_trace_id() {
    let (app, _) = setup(4).await;
    let unknown = hex::encode([99u8; 16]);

    let response = app
        .oneshot(authed_request(format!("/traces/{unknown}")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires `scripts/dev-clickhouse.sh up`"]
async fn metrics_summary_counts_by_kind() {
    let (app, _) = setup(5).await;

    let response = app
        .oneshot(authed_request("/metrics/summary"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let by_kind = body["by_kind"].as_array().unwrap();
    // Not `==`: this endpoint aggregates the whole table with no trace_id
    // filter, and other tests in this file seed their own model_call rows
    // concurrently — `>=` only asserts *our* contribution landed, not that
    // we own the table.
    let model_call = by_kind
        .iter()
        .find(|k| k["kind"] == "model_call")
        .expect("expected a model_call row");
    assert!(model_call["total_input_tokens"].as_u64().unwrap() >= 10);
}

#[tokio::test]
#[ignore = "requires `scripts/dev-clickhouse.sh up`"]
async fn get_trace_includes_a_computed_cost_for_a_priced_model_call() {
    let (app, trace_id_hex) = setup_priced(7).await;

    let response = app
        .oneshot(authed_request(format!("/traces/{trace_id_hex}")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let spans = body.as_array().unwrap();
    assert_eq!(spans.len(), 1);
    // gpt-4o-mini: $0.15 input + $0.60 output per million tokens — proof the
    // real pipeline (event -> SpanRow -> ClickHouse -> SpanDto -> JSON)
    // carries cost_usd end to end, not just crates/pricing in isolation.
    let cost = spans[0]["cost_usd"].as_f64().unwrap();
    assert!((cost - 0.75).abs() < 1e-9);
}

#[tokio::test]
#[ignore = "requires `scripts/dev-clickhouse.sh up`"]
async fn metrics_summary_includes_a_non_null_total_cost_once_a_priced_span_exists() {
    let (app, _) = setup_priced(8).await;

    let response = app
        .oneshot(authed_request("/metrics/summary"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let by_kind = body["by_kind"].as_array().unwrap();
    let model_call = by_kind
        .iter()
        .find(|k| k["kind"] == "model_call")
        .expect("expected a model_call row");
    // >=, not ==: this endpoint aggregates the whole table, and other tests
    // in this file seed model_call rows concurrently (same reasoning as
    // metrics_summary_counts_by_kind above).
    assert!(model_call["total_cost_usd"].as_f64().unwrap() >= 0.75 - 1e-9);
}

#[tokio::test]
#[ignore = "requires `scripts/dev-clickhouse.sh up`"]
async fn requests_without_a_valid_bearer_token_are_rejected() {
    let (app, _) = setup(6).await;

    let missing_header = Request::builder()
        .uri("/metrics/summary")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(missing_header).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let wrong_token = Request::builder()
        .uri("/metrics/summary")
        .header(header::AUTHORIZATION, "Bearer not-the-real-key")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(wrong_token).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "requires `scripts/dev-clickhouse.sh up`"]
async fn a_second_client_with_its_own_token_can_also_authenticate() {
    // Proves build_app really accepts more than one valid token — the real
    // motivation for this (a second real client getting its
    // own revocable credential instead of sharing the first client's).
    let client = test_client();
    clickhouse_sink::run_migrations(&client).await.expect(
        "failed to apply migrations — is ClickHouse running? (scripts/dev-clickhouse.sh up)",
    );
    let app = build_app(
        client,
        vec![TEST_API_KEY.to_string(), "second-client-key".to_string()],
    );

    let first_client = Request::builder()
        .uri("/metrics/summary")
        .header(header::AUTHORIZATION, format!("Bearer {TEST_API_KEY}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(first_client).await.unwrap().status(),
        StatusCode::OK
    );

    let second_client = Request::builder()
        .uri("/metrics/summary")
        .header(header::AUTHORIZATION, "Bearer second-client-key")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(second_client).await.unwrap().status(),
        StatusCode::OK
    );

    let neither = Request::builder()
        .uri("/metrics/summary")
        .header(header::AUTHORIZATION, "Bearer some-other-key")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.oneshot(neither).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
}
