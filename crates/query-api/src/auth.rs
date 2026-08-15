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

/// The expected `Authorization` header value, pre-formatted as
/// `Bearer <token>` once at startup rather than on every request.
#[derive(Clone)]
pub struct ExpectedBearer(String);

impl ExpectedBearer {
    pub fn new(token: impl Into<String>) -> Self {
        Self(format!("Bearer {}", token.into()))
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

    match provided {
        Some(v) if constant_time_eq(v.as_bytes(), expected.0.as_bytes()) => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}
