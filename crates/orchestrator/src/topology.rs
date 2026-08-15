//! The trellis-specific deployment topology (ClickHouse + `kernel` +
//! `query-api`) — separate from `docker_client` (generic control-plane
//! primitives, knows nothing about trellis) and from `api`/`main` (HTTP
//! wiring), so each stays about one thing.

use std::time::Duration;

use crate::docker_client::{HealthCheckSpec, ImageSource, ManagedService, PortSpec};

pub const NETWORK: &str = "trellis-orchestrator-net";
const CLICKHOUSE_NAME: &str = "trellis-orchestrator-clickhouse";

fn clickhouse_service() -> ManagedService {
    // Même image/variables d'env/healthcheck que docker/docker-compose.stack.yml
    // — le control plane doit reproduire ce que compose fait, pas inventer
    // sa propre définition du service.
    ManagedService {
        name: CLICKHOUSE_NAME.to_string(),
        image: "clickhouse/clickhouse-server:latest".to_string(),
        image_source: ImageSource::Registry,
        env: vec![
            "CLICKHOUSE_DB=observability".to_string(),
            "CLICKHOUSE_USER=dev".to_string(),
            "CLICKHOUSE_PASSWORD=dev".to_string(),
            "CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT=1".to_string(),
        ],
        healthcheck: Some(HealthCheckSpec {
            test: vec![
                "CMD".to_string(),
                "wget".to_string(),
                "--spider".to_string(),
                "-q".to_string(),
                "http://localhost:8123/ping".to_string(),
            ],
            interval: Duration::from_secs(2),
            timeout: Duration::from_secs(2),
            retries: 30,
        }),
        ports: vec![],
        depends_on: vec![],
    }
}

/// Both `kernel` and `query-api` need to reach ClickHouse by its container
/// name — resolvable because `deploy_all` puts every service on the same
/// user-defined network (`NETWORK`), which gives Docker's embedded DNS
/// resolution by container name; the default `bridge` network would not.
fn clickhouse_url() -> String {
    format!("http://{CLICKHOUSE_NAME}:8123")
}

fn kernel_service() -> ManagedService {
    ManagedService {
        name: "trellis-orchestrator-kernel".to_string(),
        image: "docker-kernel:latest".to_string(),
        image_source: ImageSource::Local,
        env: vec![
            format!("CLICKHOUSE_URL={}", clickhouse_url()),
            "CLICKHOUSE_USER=dev".to_string(),
            "CLICKHOUSE_PASSWORD=dev".to_string(),
            "CLICKHOUSE_DATABASE=observability".to_string(),
            // Dev-only fixed token, same posture as docker-compose.stack.yml
            // (docs/interfaces/kernel-auth.md).
            "KERNEL_API_KEY=dev-kernel-key".to_string(),
        ],
        healthcheck: None,
        ports: vec![PortSpec {
            container_port: 4317,
            host_port: 4317,
        }],
        depends_on: vec![CLICKHOUSE_NAME.to_string()],
    }
}

fn query_api_service() -> ManagedService {
    ManagedService {
        name: "trellis-orchestrator-query-api".to_string(),
        image: "docker-query-api:latest".to_string(),
        image_source: ImageSource::Local,
        env: vec![
            format!("CLICKHOUSE_URL={}", clickhouse_url()),
            "CLICKHOUSE_USER=dev".to_string(),
            "CLICKHOUSE_PASSWORD=dev".to_string(),
            "CLICKHOUSE_DATABASE=observability".to_string(),
            "QUERY_API_KEY=dev-query-key".to_string(),
        ],
        healthcheck: None,
        ports: vec![PortSpec {
            container_port: 8080,
            host_port: 8080,
        }],
        depends_on: vec![CLICKHOUSE_NAME.to_string()],
    }
}

/// ClickHouse, then `kernel`/`query-api` (both `depends_on` it) — order in
/// this `Vec` doesn't matter to `deploy_all` (it sorts by `depends_on`
/// itself), written dependency-first here only for readability.
pub fn trellis_stack() -> Vec<ManagedService> {
    vec![clickhouse_service(), kernel_service(), query_api_service()]
}
