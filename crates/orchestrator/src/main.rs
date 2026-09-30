//! A home-made control plane (design dossier section 5: the cloud decision was
//! left open and set aside for a learning goal, coding a managed service in
//! Rust to understand the concepts before choosing a host; see ADR 0002).
//! Exposes `POST /deploy`, `GET /status`, `POST /teardown` over ClickHouse +
//! `kernel` + `query-api` (`crates/orchestrator/src/topology.rs`), replacing
//! `scripts/dev-stack.sh` with a real service rather than a script.
//!
//! `kernel`/`query-api` are built by this control plane itself
//! (`POST /build`, `src/image_build.rs` for the tar context): no external
//! `docker build` is needed. Only ClickHouse is still pulled from a registry
//! (`docs/interfaces/docker-engine-api.md`).
//!
//! Authenticated like `crates/kernel`/`crates/query-api`
//! (docs/interfaces/kernel-auth.md, `src/api.rs`): `ORCHESTRATOR_API_KEY`,
//! fail-closed at startup. `ORCHESTRATOR_BIND` defaults to `127.0.0.1`
//! rather than `0.0.0.0` (unlike `crates/kernel`/`crates/query-api`):
//! authentication adds to that caution, it does not replace it.

use std::sync::Arc;

use bollard::Docker;
use orchestrator::api::{AppState, build_app};
use orchestrator::topology::{NETWORK, retina_stack};

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// `ORCHESTRATOR_API_KEY` is required and must not be blank: `ORCHESTRATOR_API_KEY=` (set but empty, a
/// common `.env` slip) would otherwise start the process with the token
/// `""`, i.e. accept `authorization: Bearer ` — fail closed on it exactly
/// like on an unset key. Pure function, testable without the environment.
fn primary_api_key(raw: Option<String>) -> Result<String, String> {
    match raw {
        Some(key) if !key.trim().is_empty() => Ok(key),
        Some(_) => Err(
            "ORCHESTRATOR_API_KEY is set but blank — see docs/interfaces/kernel-auth.md"
                .to_string(),
        ),
        None => {
            Err("ORCHESTRATOR_API_KEY must be set — see docs/interfaces/kernel-auth.md".to_string())
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind_addr = env_or("ORCHESTRATOR_BIND", "127.0.0.1:9000");
    // Fails closed (docs/interfaces/kernel-auth.md): resolved before any
    // Docker access, same order as crates/kernel — a config error must
    // fail before touching a network or system resource, not after.
    let api_key = primary_api_key(std::env::var("ORCHESTRATOR_API_KEY").ok())
        .unwrap_or_else(|e| panic!("{e}"));

    let docker = Docker::connect_with_local_defaults()?;

    let state = AppState {
        docker: Arc::new(docker),
        network: NETWORK.to_string(),
        services: Arc::new(retina_stack()),
    };
    let app = build_app(state, api_key);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    eprintln!("orchestrator listening on {bind_addr}");
    eprintln!("  POST /deploy    — deploy clickhouse, then kernel/query-api once it's healthy");
    eprintln!("  GET  /status    — current status of all 3 services");
    eprintln!("  POST /teardown  — stop and remove all 3");
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_api_key_refuses_unset_and_blank() {
        assert!(primary_api_key(None).is_err());
        assert!(primary_api_key(Some(String::new())).is_err());
        assert!(primary_api_key(Some("  ".to_string())).is_err());
        assert_eq!(primary_api_key(Some("k".to_string())).unwrap(), "k");
    }
}
