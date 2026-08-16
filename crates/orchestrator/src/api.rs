//! HTTP control surface — `POST /deploy`, `GET /status`, `POST /teardown` —
//! over the `docker_client` primitives. Same layering as `crates/query-api`
//! (routes/DTOs separate from the client doing the real work) for
//! consistency with the rest of this workspace, not because this crate
//! needs it independently.
//!
//! Authenticated the same way as `crates/kernel`/`crates/query-api`
//! (docs/interfaces/kernel-auth.md) — `ORCHESTRATOR_API_KEY`, read and
//! enforced fail-closed in `main.rs`. `ORCHESTRATOR_BIND` still defaults to
//! `127.0.0.1` rather than `0.0.0.0` (see `main.rs`) — auth is a layer on
//! top of that default, not a reason to widen it: this surface can stop
//! containers, more sensitive than either kernel or query-api, not less.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use bollard::Docker;
use serde::Serialize;

use crate::auth::{ExpectedBearer, require_api_key};
use crate::docker_client::{self, ManagedService, OrchestratorError, ServiceStatus};

#[derive(Clone)]
pub struct AppState {
    pub docker: Arc<Docker>,
    pub network: String,
    pub services: Arc<Vec<ManagedService>>,
}

/// `api_key` gates all 3 routes below via `require_api_key`
/// (docs/interfaces/kernel-auth.md) — `route_layer` rather than `layer` so
/// it applies to the matched routes only, not to 404s on unknown paths
/// (same reasoning as `crates/query-api/src/app.rs`).
pub fn build_app(state: AppState, api_key: String) -> Router {
    Router::new()
        .route("/deploy", post(deploy))
        .route("/status", get(status))
        .route("/teardown", post(teardown))
        .route_layer(middleware::from_fn_with_state(
            ExpectedBearer::new(api_key),
            require_api_key,
        ))
        .with_state(state)
}

#[derive(Serialize)]
struct ServiceStatusDto {
    name: String,
    status: ServiceStatus,
}

async fn all_statuses(state: &AppState) -> Result<Vec<ServiceStatusDto>, ApiError> {
    let mut out = Vec::with_capacity(state.services.len());
    for service in state.services.iter() {
        let current = docker_client::status(&state.docker, &service.name).await?;
        out.push(ServiceStatusDto {
            name: service.name.clone(),
            status: current,
        });
    }
    Ok(out)
}

async fn status(State(state): State<AppState>) -> Result<Json<Vec<ServiceStatusDto>>, ApiError> {
    Ok(Json(all_statuses(&state).await?))
}

/// Idempotent, like `deploy_all` itself — calling this on an
/// already-deployed stack converges rather than errors, so a client can
/// retry blindly on a timeout without first checking what's already up.
async fn deploy(State(state): State<AppState>) -> Result<Json<Vec<ServiceStatusDto>>, ApiError> {
    docker_client::deploy_all(&state.docker, &state.network, &state.services).await?;
    Ok(Json(all_statuses(&state).await?))
}

async fn teardown(State(state): State<AppState>) -> Result<Json<Vec<ServiceStatusDto>>, ApiError> {
    docker_client::teardown_all(&state.docker, &state.services).await?;
    Ok(Json(all_statuses(&state).await?))
}

struct ApiError(OrchestratorError);

impl From<OrchestratorError> for ApiError {
    fn from(e: OrchestratorError) -> Self {
        ApiError(e)
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // Distinguishes "you asked for something that can't work"
        // (4xx — a bad depends_on, a missing image the client needs to
        // build first) from "the daemon or a container didn't cooperate"
        // (5xx/504) — collapsing everything to 500 would make a caller's
        // retry logic guess instead of knowing what to check.
        let status = match &self.0 {
            OrchestratorError::UnknownDependency { .. } | OrchestratorError::DependencyCycle => {
                StatusCode::BAD_REQUEST
            }
            OrchestratorError::MissingLocalImage(_) => StatusCode::UNPROCESSABLE_ENTITY,
            OrchestratorError::HealthTimeout { .. } => StatusCode::GATEWAY_TIMEOUT,
            OrchestratorError::BuildContext(_)
            | OrchestratorError::BuildFailed(_)
            | OrchestratorError::Docker(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorBody {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}
