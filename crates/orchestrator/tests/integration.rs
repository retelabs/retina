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
    HealthCheckSpec, ManagedService, ServiceStatus, ensure_running, status, teardown, wait_healthy,
};

fn test_service() -> ManagedService {
    ManagedService {
        name: "trellis-orchestrator-test-clickhouse".to_string(),
        image: "clickhouse/clickhouse-server:latest".to_string(),
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
    }
}

#[tokio::test]
#[ignore = "requires a local Docker daemon"]
async fn ensure_running_is_idempotent_and_reaches_healthy() {
    let docker = Docker::connect_with_local_defaults().expect("failed to connect to Docker");
    let service = test_service();

    // Clean slate — a previous failed run shouldn't make this test flaky.
    teardown(&docker, &service.name).await.unwrap();
    assert_eq!(
        status(&docker, &service.name).await.unwrap(),
        ServiceStatus::Absent
    );

    ensure_running(&docker, &service).await.unwrap();
    let reached = wait_healthy(&docker, &service.name, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(reached, ServiceStatus::Healthy);

    // The actual idempotence claim: calling it again on an already-running,
    // already-healthy container must not error (the "start on an already
    // started container" 304 path, and the "container already exists"
    // path).
    ensure_running(&docker, &service).await.unwrap();
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
