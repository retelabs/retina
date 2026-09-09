//! Shared-secret auth on the gRPC surface — contract verified in
//! docs/interfaces/kernel-auth.md before writing this (tonic's
//! `Interceptor` trait and `TraceServiceServer::with_interceptor`, both
//! checked against the actual generated code, not recalled from memory).

use tonic::metadata::MetadataValue;
use tonic::service::Interceptor;
use tonic::{Request, Status};

/// Constant-time comparison — a naive byte-by-byte `==` with early return
/// leaks timing information about how many leading bytes matched, a real
/// side channel for a shared secret.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Checks every incoming request's `authorization` metadata against a set of
/// expected `Bearer <token>` values — one per known client (see
/// `crates/kernel/src/main.rs`: `KERNEL_API_KEY` plus optional
/// `KERNEL_API_KEYS_EXTRA`), so a second real client (a second real vertical
/// showing up, same reasoning `ENABLED_PLUGINS` already applies elsewhere in
/// this kernel) doesn't have to share a credential with the first one to be
/// revocable independently. Fails closed: constructing this requires at
/// least the primary token, there is no "disabled" state.
#[derive(Clone)]
pub struct ApiKeyInterceptor {
    expected: Vec<String>,
}

impl ApiKeyInterceptor {
    pub fn new(tokens: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            expected: tokens
                .into_iter()
                .map(|t| format!("Bearer {}", t.into()))
                .collect(),
        }
    }
}

impl Interceptor for ApiKeyInterceptor {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        let provided = request
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok());

        // Checks against every expected token unconditionally (no early
        // return on the first match) — otherwise which token index matched
        // would itself leak a little timing information, on top of the
        // per-token constant-time comparison already guarding against a
        // leak of *which token* it is.
        let matched = match provided {
            Some(v) => self.expected.iter().fold(false, |acc, exp| {
                acc | constant_time_eq(v.as_bytes(), exp.as_bytes())
            }),
            None => false,
        };

        if matched {
            Ok(request)
        } else {
            Err(Status::unauthenticated(
                "missing or invalid authorization header",
            ))
        }
    }
}

/// Builds the `authorization` metadata value a client should attach to
/// every request — used by `fraudos-replay`/`oncology-replay` so the header
/// format lives in one place instead of being duplicated per caller.
pub fn bearer_metadata_value(token: &str) -> Result<MetadataValue<tonic::metadata::Ascii>, String> {
    format!("Bearer {token}")
        .parse()
        .map_err(|e| format!("token is not a valid metadata value: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request_with_auth(header: Option<&str>) -> Request<()> {
        let mut request = Request::new(());
        if let Some(h) = header {
            request
                .metadata_mut()
                .insert("authorization", h.parse().unwrap());
        }
        request
    }

    #[test]
    fn accepts_matching_bearer_token() {
        let mut interceptor = ApiKeyInterceptor::new(["secret"]);
        let request = request_with_auth(Some("Bearer secret"));
        assert!(interceptor.call(request).is_ok());
    }

    #[test]
    fn rejects_missing_header() {
        let mut interceptor = ApiKeyInterceptor::new(["secret"]);
        let request = request_with_auth(None);
        let err = interceptor.call(request).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn rejects_wrong_token() {
        let mut interceptor = ApiKeyInterceptor::new(["secret"]);
        let request = request_with_auth(Some("Bearer wrong"));
        assert!(interceptor.call(request).is_err());
    }

    #[test]
    fn rejects_token_without_bearer_prefix() {
        let mut interceptor = ApiKeyInterceptor::new(["secret"]);
        let request = request_with_auth(Some("secret"));
        assert!(interceptor.call(request).is_err());
    }

    #[test]
    fn accepts_any_token_from_a_multi_token_set() {
        let mut interceptor = ApiKeyInterceptor::new(["primary", "secondary"]);
        assert!(
            interceptor
                .call(request_with_auth(Some("Bearer primary")))
                .is_ok()
        );
        assert!(
            interceptor
                .call(request_with_auth(Some("Bearer secondary")))
                .is_ok()
        );
    }

    #[test]
    fn rejects_a_token_not_in_the_multi_token_set() {
        let mut interceptor = ApiKeyInterceptor::new(["primary", "secondary"]);
        let request = request_with_auth(Some("Bearer neither"));
        assert!(interceptor.call(request).is_err());
    }
}
