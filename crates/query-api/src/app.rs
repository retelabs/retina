use axum::Router;
use axum::middleware;
use axum::routing::get;
use clickhouse::Client;

use crate::auth::{ExpectedBearer, require_api_key};
use crate::routes::{get_trace, list_traces, metrics_summary};

/// The 2-3 endpoints dossier étape 4 asks for — see
/// docs/interfaces/query-api.md for the contract each one exposes.
///
/// Route syntax note: axum 0.8 requires `{param}`, not the pre-0.8 `:param`
/// — using the old syntax panics at router build time rather than failing
/// silently (verified against the 0.8 changelog before writing this).
///
/// `api_key` gates every route below via `require_api_key`
/// (docs/interfaces/kernel-auth.md) — `route_layer` rather than `layer` so
/// it applies to the matched routes only, not to 404s on unknown paths.
pub fn build_app(client: Client, api_key: String) -> Router {
    Router::new()
        .route("/traces", get(list_traces))
        .route("/traces/{trace_id}", get(get_trace))
        .route("/metrics/summary", get(metrics_summary))
        .route_layer(middleware::from_fn_with_state(
            ExpectedBearer::new(api_key),
            require_api_key,
        ))
        .with_state(client)
}
