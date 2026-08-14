use axum::Router;
use axum::routing::get;
use clickhouse::Client;

use crate::routes::{get_trace, list_traces, metrics_summary};

/// The 2-3 endpoints dossier étape 4 asks for — see
/// docs/interfaces/query-api.md for the contract each one exposes.
///
/// Route syntax note: axum 0.8 requires `{param}`, not the pre-0.8 `:param`
/// — using the old syntax panics at router build time rather than failing
/// silently (verified against the 0.8 changelog before writing this).
pub fn build_app(client: Client) -> Router {
    Router::new()
        .route("/traces", get(list_traces))
        .route("/traces/{trace_id}", get(get_trace))
        .route("/metrics/summary", get(metrics_summary))
        .with_state(client)
}
