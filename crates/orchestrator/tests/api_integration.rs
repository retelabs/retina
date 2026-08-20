//! Drives the real HTTP surface (`src/api.rs`) via `tower::ServiceExt::oneshot`
//! — same pattern `crates/query-api` uses to test its `Router` in-process,
//! no real TCP listener needed. Runs against the real local Docker daemon
//! (this is the same 3-service venice stack `main.rs` deploys, not a toy
//! topology) — `kernel`/`query-api` are built by `/deploy` itself now
//! (`ImageSource::Build`), no external `docker build` prerequisite left:
//!
//!   cargo test -p orchestrator -- --ignored
//!
//! Slow the first time (a real release build of both binaries inside
//! Docker) — fast on repeat runs via Docker's own layer cache.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use bollard::Docker;
use http_body_util::BodyExt;
use orchestrator::api::{AppState, build_app};
use orchestrator::docker_client::teardown_all;
use orchestrator::topology::{NETWORK, venice_stack};
use serde_json::Value;
use tower::ServiceExt;

const TEST_API_KEY: &str = "test-key";

fn app() -> Router {
    let docker = Docker::connect_with_local_defaults().expect("failed to connect to Docker");
    build_app(
        AppState {
            docker: Arc::new(docker),
            network: NETWORK.to_string(),
            services: Arc::new(venice_stack()),
        },
        TEST_API_KEY.to_string(),
    )
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Authenticated request — the path every real test below exercises.
async fn request(app: Router, method: Method, path: &str) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method(method)
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {TEST_API_KEY}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires a local Docker daemon; builds real images the first time"]
async fn deploy_status_teardown_round_trip_over_http() {
    // Clean slate — a previous failed run shouldn't make this test flaky.
    let docker = Docker::connect_with_local_defaults().unwrap();
    teardown_all(&docker, &venice_stack()).await.unwrap();

    let before = body_json(request(app(), Method::GET, "/status").await).await;
    for entry in before.as_array().unwrap() {
        assert_eq!(entry["status"], "Absent");
    }

    let deployed = body_json(request(app(), Method::POST, "/deploy").await).await;
    let statuses = deployed.as_array().unwrap();
    assert_eq!(statuses.len(), 3);
    for entry in statuses {
        let status = entry["status"].as_str().unwrap();
        assert!(
            status == "Healthy" || status == "RunningNoHealthcheck",
            "expected {} to be up, got {status}",
            entry["name"]
        );
    }

    // Idempotent from the HTTP side too — the same claim already proven at
    // the docker_client level, now proven through the API a client
    // actually calls.
    let response = request(app(), Method::POST, "/deploy").await;
    assert_eq!(response.status(), StatusCode::OK);

    let after_teardown = body_json(request(app(), Method::POST, "/teardown").await).await;
    for entry in after_teardown.as_array().unwrap() {
        assert_eq!(entry["status"], "Absent");
    }
}

#[tokio::test]
#[ignore = "requires a local Docker daemon"]
async fn requests_without_a_valid_bearer_token_are_rejected() {
    let missing_header = Request::builder()
        .uri("/status")
        .body(Body::empty())
        .unwrap();
    let response = app().oneshot(missing_header).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let wrong_token = Request::builder()
        .uri("/status")
        .header(header::AUTHORIZATION, "Bearer not-the-real-key")
        .body(Body::empty())
        .unwrap();
    let response = app().oneshot(wrong_token).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
