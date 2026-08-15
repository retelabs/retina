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

/// Checks every incoming request's `authorization` metadata against a
/// single expected `Bearer <token>` value. Fails closed: constructing this
/// requires a token (see `crates/kernel/src/main.rs`), there is no
/// "disabled" state.
#[derive(Clone)]
pub struct ApiKeyInterceptor {
    expected: String,
}

impl ApiKeyInterceptor {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            expected: format!("Bearer {}", token.into()),
        }
    }
}

impl Interceptor for ApiKeyInterceptor {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        let provided = request
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok());

        match provided {
            Some(v) if constant_time_eq(v.as_bytes(), self.expected.as_bytes()) => Ok(request),
            _ => Err(Status::unauthenticated(
                "missing or invalid authorization header",
            )),
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
        let mut interceptor = ApiKeyInterceptor::new("secret");
        let request = request_with_auth(Some("Bearer secret"));
        assert!(interceptor.call(request).is_ok());
    }

    #[test]
    fn rejects_missing_header() {
        let mut interceptor = ApiKeyInterceptor::new("secret");
        let request = request_with_auth(None);
        let err = interceptor.call(request).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn rejects_wrong_token() {
        let mut interceptor = ApiKeyInterceptor::new("secret");
        let request = request_with_auth(Some("Bearer wrong"));
        assert!(interceptor.call(request).is_err());
    }

    #[test]
    fn rejects_token_without_bearer_prefix() {
        let mut interceptor = ApiKeyInterceptor::new("secret");
        let request = request_with_auth(Some("secret"));
        assert!(interceptor.call(request).is_err());
    }
}
