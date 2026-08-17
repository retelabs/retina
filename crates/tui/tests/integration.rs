//! Exercises `api::ApiClient` against a real running `query-api` — the
//! riskiest new code here is the HTTP/auth/deserialization path (reusing
//! `query_api::dto` types via `Deserialize`), not the ratatui rendering
//! (which already type-checks against a real ratatui version). Not run by
//! default `cargo test`:
//!
//!   scripts/dev-stack.sh up   (or any reachable query-api)
//!   QUERY_API_URL=http://localhost:8080 QUERY_API_KEY=<key> \
//!     cargo test -p tui -- --ignored

use tui::api::ApiClient;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn client() -> ApiClient {
    let base_url = env_or("QUERY_API_URL", "http://localhost:8080");
    let api_key = env_or("QUERY_API_KEY", "dev-query-key");
    ApiClient::new(base_url, api_key)
}

#[tokio::test]
#[ignore = "requires a real reachable query-api"]
async fn lists_real_traces() {
    let traces = client().list_traces(5).await.expect("list_traces failed");
    // Not asserting a specific count — this hits whatever's really in
    // ClickHouse right now, the point is the request/auth/deserialization
    // round-trip succeeds against the real server, not a fixture.
    println!("got {} trace(s)", traces.len());
}

#[tokio::test]
#[ignore = "requires a real reachable query-api"]
async fn fetches_a_real_trace_and_computes_its_span_tree() {
    let traces = client().list_traces(1).await.expect("list_traces failed");
    let Some(trace) = traces.first() else {
        eprintln!("no traces in ClickHouse yet — nothing to fetch, not a failure");
        return;
    };

    let spans = client()
        .get_trace(&trace.trace_id)
        .await
        .expect("get_trace failed");
    assert!(!spans.is_empty());

    let tree = tui::app::span_tree(&spans);
    assert_eq!(tree.len(), spans.len(), "span_tree must not drop spans");
}

#[tokio::test]
#[ignore = "requires a real reachable query-api"]
async fn fetches_real_metrics_summary() {
    let metrics = client()
        .metrics_summary()
        .await
        .expect("metrics_summary failed");
    println!(
        "{} kind(s), {} spans_with_warnings",
        metrics.by_kind.len(),
        metrics.spans_with_warnings
    );
}

#[tokio::test]
#[ignore = "requires a real reachable query-api"]
async fn rejects_a_bad_token() {
    let client = ApiClient::new(
        env_or("QUERY_API_URL", "http://localhost:8080"),
        "wrong-key".to_string(),
    );
    let err = client.list_traces(1).await.expect_err("expected 401");
    assert!(matches!(err, tui::api::ApiError::Status(status, _) if status.as_u16() == 401));
}
