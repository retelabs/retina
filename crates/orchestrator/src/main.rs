//! Control plane "maison" (dossier section 5 — décision cloud ouverte, mise
//! de côté au profit d'un objectif d'apprentissage : coder un service
//! managé en Rust, comprendre les concepts, avant de choisir un hébergeur).
//! Remplace `scripts/dev-stack.sh` : déploie ClickHouse + `kernel` +
//! `query-api`, dans cet ordre (`depends_on`), sur un réseau Docker partagé
//! pour qu'ils se résolvent par nom comme sous `docker compose`.
//!
//! Pas encore un service HTTP — un binaire qui prouve, contre le vrai démon
//! Docker local, que le cycle de vie complet des 3 services marche.
//!
//! Prérequis : les images `docker-kernel:latest`/`docker-query-api:latest`
//! doivent déjà exister localement (`scripts/dev-stack.sh up` une fois, ou
//! `docker compose -f docker/docker-compose.stack.yml build`) — ce control
//! plane ne construit pas encore d'image lui-même
//! (docs/interfaces/docker-engine-api.md).

use std::time::Duration;

use bollard::Docker;
use orchestrator::docker_client::{
    HealthCheckSpec, ImageSource, ManagedService, PortSpec, deploy_all, status, teardown_all,
};

const NETWORK: &str = "trellis-orchestrator-net";
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_local_defaults()?;
    let services = vec![clickhouse_service(), kernel_service(), query_api_service()];

    for service in &services {
        println!(
            "état avant déploiement — {}: {:?}",
            service.name,
            status(&docker, &service.name).await?
        );
    }

    println!(
        "\n→ deploy_all (clickhouse d'abord, puis kernel/query-api une fois clickhouse healthy)"
    );
    deploy_all(&docker, NETWORK, &services).await?;

    for service in &services {
        println!(
            "état atteint — {}: {:?}",
            service.name,
            status(&docker, &service.name).await?
        );
    }

    if std::env::args().any(|a| a == "--keep-running") {
        println!(
            "\n--keep-running : pile laissée en route (query-api sur localhost:8080, kernel sur localhost:4317)"
        );
        return Ok(());
    }

    println!("\n→ teardown_all");
    teardown_all(&docker, &services).await?;

    for service in &services {
        println!(
            "état après teardown — {}: {:?}",
            service.name,
            status(&docker, &service.name).await?
        );
    }

    Ok(())
}
