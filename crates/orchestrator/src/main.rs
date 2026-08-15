//! Control plane "maison" (dossier section 5 — décision cloud ouverte, mise
//! de côté au profit d'un objectif d'apprentissage : coder un service
//! managé en Rust, comprendre les concepts, avant de choisir un hébergeur).
//! Exposes `POST /deploy`, `GET /status`, `POST /teardown` over ClickHouse +
//! `kernel` + `query-api` (`crates/orchestrator/src/topology.rs`), replacing
//! `scripts/dev-stack.sh` with a real service rather than a script.
//!
//! Prérequis : les images `docker-kernel:latest`/`docker-query-api:latest`
//! doivent déjà exister localement (`scripts/dev-stack.sh up` une fois, ou
//! `docker compose -f docker/docker-compose.stack.yml build`) — ce control
//! plane ne construit pas encore d'image lui-même
//! (docs/interfaces/docker-engine-api.md).
//!
//! Pas d'authentification (docs.rs/interfaces/docker-engine-api.md,
//! `src/api.rs`) — délibérément différé, compensé par un bind par défaut
//! sur `127.0.0.1` plutôt que `0.0.0.0` (contrairement à `crates/kernel`/
//! `crates/query-api`, qui écoutent sur toutes les interfaces).

use std::sync::Arc;

use bollard::Docker;
use orchestrator::api::{AppState, build_app};
use orchestrator::topology::{NETWORK, trellis_stack};

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_local_defaults()?;
    let bind_addr = env_or("ORCHESTRATOR_BIND", "127.0.0.1:9000");

    let state = AppState {
        docker: Arc::new(docker),
        network: NETWORK.to_string(),
        services: Arc::new(trellis_stack()),
    };
    let app = build_app(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    eprintln!("orchestrator listening on {bind_addr}");
    eprintln!("  POST /deploy    — deploy clickhouse, then kernel/query-api once it's healthy");
    eprintln!("  GET  /status    — current status of all 3 services");
    eprintln!("  POST /teardown  — stop and remove all 3");
    axum::serve(listener, app).await?;

    Ok(())
}
