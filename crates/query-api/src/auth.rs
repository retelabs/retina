//! Shared-secret auth on the HTTP surface — contract verified in
//! docs/interfaces/kernel-auth.md before writing this
//! (`axum::middleware::from_fn_with_state`, checked against docs.rs for the
//! exact axum 0.8.9 pinned in this workspace, not recalled from memory).

use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;

/// See the identical comment in crates/otlp-receiver/src/auth.rs — avoids
/// leaking timing information about how many leading bytes matched.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The set of expected `Authorization` header values, pre-formatted as
/// `Bearer <token>` once at startup rather than on every request — one per
/// known client (see `crates/query-api/src/main.rs`: `QUERY_API_KEY` plus
/// optional `QUERY_API_KEYS_EXTRA`), same reasoning as
/// `crates/otlp-receiver/src/auth.rs`'s `ApiKeyInterceptor`.
#[derive(Clone)]
pub struct ExpectedBearer(Vec<String>);

impl ExpectedBearer {
    pub fn new(tokens: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self(
            tokens
                .into_iter()
                .map(|t| format!("Bearer {}", t.into()))
                .collect(),
        )
    }
}

pub async fn require_api_key(
    State(expected): State<ExpectedBearer>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let provided = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    // Same "no early return across tokens" reasoning as
    // crates/otlp-receiver/src/auth.rs — checks every expected token
    // unconditionally rather than short-circuiting on the first match.
    let matched = match provided {
        Some(v) => expected.0.iter().fold(false, |acc, exp| {
            acc | constant_time_eq(v.as_bytes(), exp.as_bytes())
        }),
        None => false,
    };

    if matched {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}
