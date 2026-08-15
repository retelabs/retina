//! V0 du control plane "maison" (dossier section 5 — décision cloud
//! ouverte, mise de côté au profit d'un objectif d'apprentissage : coder un
//! service managé en Rust, comprendre les concepts, avant de choisir un
//! hébergeur). Remplace `scripts/dev-stack.sh` pour un seul service pour
//! l'instant (ClickHouse) — prouver le principe avant de l'étendre à
//! `kernel`/`query-api`, qui ont une dépendance d'ordre en plus (attendre
//! ClickHouse healthy avant de démarrer).
//!
//! Pas encore un service HTTP — un binaire qui prouve, contre le vrai démon
//! Docker local, que le cycle de vie complet marche : tirer l'image, créer
//! le conteneur, le démarrer, attendre qu'il soit sain, lire son état, le
//! détruire.

use std::time::Duration;

use bollard::Docker;
use orchestrator::docker_client::{
    HealthCheckSpec, ManagedService, ensure_running, status, teardown, wait_healthy,
};

fn clickhouse_service() -> ManagedService {
    // Même image/variables d'env/healthcheck que docker/docker-compose.stack.yml
    // — le control plane doit reproduire ce que compose fait, pas inventer
    // sa propre définition du service.
    ManagedService {
        name: "trellis-orchestrator-clickhouse".to_string(),
        image: "clickhouse/clickhouse-server:latest".to_string(),
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
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let docker = Docker::connect_with_local_defaults()?;
    let service = clickhouse_service();

    println!(
        "état avant déploiement : {:?}",
        status(&docker, &service.name).await?
    );

    println!("→ ensure_running({})", service.name);
    ensure_running(&docker, &service).await?;

    println!("→ wait_healthy (jusqu'à 60s)");
    let reached = wait_healthy(&docker, &service.name, Duration::from_secs(60)).await?;
    println!("état atteint : {reached:?}");

    println!("→ teardown({})", service.name);
    teardown(&docker, &service.name).await?;
    println!(
        "état après teardown : {:?}",
        status(&docker, &service.name).await?
    );

    Ok(())
}
