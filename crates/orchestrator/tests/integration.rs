//! Runs against the real local Docker daemon. Not run by default
//! `cargo test` — requires Docker running locally:
//!
//!   cargo test -p orchestrator -- --ignored
//!
//! Proves the control plane's core promise: calling `ensure_running`
//! repeatedly converges to "running", never errors on "already there" —
//! the same idempotence a real reconciliation loop depends on (a control
//! plane that can't safely retry isn't one).

use std::time::Duration;

use bollard::Docker;
use orchestrator::docker_client::{
    HealthCheckSpec, ImageSource, ManagedService, ServiceStatus, deploy_all, ensure_network,
    ensure_running, status, teardown, teardown_all, wait_healthy,
};

const NETWORK: &str = "venice-orchestrator-test-net";

fn clickhouse_test_service(name: &str) -> ManagedService {
    ManagedService {
        name: name.to_string(),
        image: "clickhouse/clickhouse-server:latest".to_string(),
        image_source: ImageSource::Registry,
        env: vec![
            "CLICKHOUSE_DB=observability".to_string(),
            "CLICKHOUSE_USER=dev".to_string(),
            "CLICKHOUSE_PASSWORD=dev".to_string(),
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

#[tokio::test]
#[ignore = "requires a local Docker daemon"]
async fn ensure_running_is_idempotent_and_reaches_healthy() {
    let docker = Docker::connect_with_local_defaults().expect("failed to connect to Docker");
    let service = clickhouse_test_service("venice-orchestrator-test-clickhouse");
    ensure_network(&docker, NETWORK).await.unwrap();

    // Clean slate — a previous failed run shouldn't make this test flaky.
    teardown(&docker, &service.name).await.unwrap();
    assert_eq!(
        status(&docker, &service.name).await.unwrap(),
        ServiceStatus::Absent
    );

    ensure_running(&docker, NETWORK, &service).await.unwrap();
    let reached = wait_healthy(&docker, &service.name, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(reached, ServiceStatus::Healthy);

    // The actual idempotence claim: calling it again on an already-running,
    // already-healthy container must not error (the "start on an already
    // started container" 304 path, and the "container already exists"
    // path).
    ensure_running(&docker, NETWORK, &service).await.unwrap();
    assert_eq!(
        status(&docker, &service.name).await.unwrap(),
        ServiceStatus::Healthy
    );

    teardown(&docker, &service.name).await.unwrap();
    assert_eq!(
        status(&docker, &service.name).await.unwrap(),
        ServiceStatus::Absent
    );

    // Idempotent teardown too — tearing down something already absent
    // must not error either.
    teardown(&docker, &service.name).await.unwrap();
}

#[tokio::test]
#[ignore = "requires a local Docker daemon"]
async fn deploy_all_brings_up_a_dependency_chain_and_teardown_all_clears_it() {
    let docker = Docker::connect_with_local_defaults().expect("failed to connect to Docker");

    let dependency = clickhouse_test_service("venice-orchestrator-test-chain-a");
    let mut dependent = clickhouse_test_service("venice-orchestrator-test-chain-b");
    dependent.depends_on = vec![dependency.name.clone()];
    // No healthcheck on the dependent — exercises wait_healthy's "running,
    // no healthcheck configured" success path, distinct from the
    // dependency's real one.
    dependent.healthcheck = None;
    let services = vec![dependency, dependent];

    for s in &services {
        teardown(&docker, &s.name).await.unwrap();
    }

    deploy_all(&docker, NETWORK, &services).await.unwrap();
    for s in &services {
        let current = status(&docker, &s.name).await.unwrap();
        assert!(
            matches!(
                current,
                ServiceStatus::Healthy | ServiceStatus::RunningNoHealthcheck
            ),
            "expected {} to be up, got {current:?}",
            s.name
        );
    }

    teardown_all(&docker, &services).await.unwrap();
    for s in &services {
        assert_eq!(
            status(&docker, &s.name).await.unwrap(),
            ServiceStatus::Absent
        );
    }
}
