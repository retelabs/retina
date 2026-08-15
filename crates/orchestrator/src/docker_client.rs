//! Wraps the parts of the Docker Engine API this control plane needs:
//! ensure an image is present, ensure a container is running from it, poll
//! its health, tear it down. Every signature and field name here was read
//! from the real `bollard` 0.21.0 / `bollard-stubs` 1.53.1-rc source
//! (`~/.cargo/registry/src/.../bollard-0.21.0/src/container.rs` and
//! `bollard-stubs-.../src/models.rs`), not recalled from memory — bollard
//! renamed things across versions (`ContainerCreateBody` instead of the
//! older `Config<String>`), so this was worth checking for real.

use std::fmt;
use std::time::Duration;

use bollard::Docker;
use bollard::errors::Error as BollardError;
use bollard::models::{ContainerCreateBody, ContainerState, HealthConfig, HealthStatusEnum};
use bollard::query_parameters::{
    CreateContainerOptions, CreateImageOptions, InspectContainerOptions, ListContainersOptions,
    RemoveContainerOptions, StartContainerOptions, StopContainerOptions,
};
use futures_util::StreamExt;

#[derive(Debug)]
pub enum OrchestratorError {
    Docker(BollardError),
    /// `wait_healthy` gave up before the container reported healthy —
    /// distinct from a Docker API failure, the daemon answered fine each
    /// time, the container just never got there.
    HealthTimeout {
        service: String,
        waited: Duration,
    },
}

impl fmt::Display for OrchestratorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrchestratorError::Docker(e) => write!(f, "docker API error: {e}"),
            OrchestratorError::HealthTimeout { service, waited } => {
                write!(f, "`{service}` did not report healthy within {waited:?}")
            }
        }
    }
}

impl std::error::Error for OrchestratorError {}

impl From<BollardError> for OrchestratorError {
    fn from(e: BollardError) -> Self {
        OrchestratorError::Docker(e)
    }
}

/// A healthcheck, in the same shape Docker itself wants (command + timing),
/// so `ManagedService` reads like the `healthcheck:` block in
/// `docker/docker-compose.stack.yml` rather than an ad-hoc invention.
pub struct HealthCheckSpec {
    /// e.g. `vec!["CMD".into(), "wget".into(), "--spider".into(), "-q".into(), "http://localhost:8123/ping".into()]`
    /// — same array form (not `CMD-SHELL`) as `docker/docker-compose.stack.yml`'s
    /// `healthcheck.test`.
    pub test: Vec<String>,
    pub interval: Duration,
    pub timeout: Duration,
    pub retries: i64,
}

impl From<&HealthCheckSpec> for HealthConfig {
    fn from(spec: &HealthCheckSpec) -> Self {
        HealthConfig {
            test: Some(spec.test.clone()),
            interval: Some(spec.interval.as_nanos() as i64),
            timeout: Some(spec.timeout.as_nanos() as i64),
            retries: Some(spec.retries),
            start_period: None,
            start_interval: None,
        }
    }
}

pub struct ManagedService {
    pub name: String,
    pub image: String,
    pub env: Vec<String>,
    pub healthcheck: Option<HealthCheckSpec>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ServiceStatus {
    /// No container by this name exists yet.
    Absent,
    /// Exists but Docker reports it as not running (`exited`, `created`, ...).
    Stopped,
    /// Running, no healthcheck configured — matches `HealthStatusEnum::NONE`.
    RunningNoHealthcheck,
    Starting,
    Healthy,
    Unhealthy,
}

async fn container_exists(docker: &Docker, name: &str) -> Result<bool, OrchestratorError> {
    // list_containers(all: true) is how the Engine API distinguishes "never
    // created" from "exists but stopped" — inspect_container alone can't
    // (it 404s for both in slightly different-looking ways depending on
    // version, list+filter is the documented pattern).
    let options = ListContainersOptions {
        all: true,
        filters: Some(std::collections::HashMap::from([(
            "name".to_string(),
            vec![format!("^/{name}$")],
        )])),
        ..Default::default()
    };
    let containers = docker.list_containers(Some(options)).await?;
    Ok(!containers.is_empty())
}

/// Pulls `image` if the daemon doesn't already have it. Streams progress
/// events (`create_image` doesn't actually pull anything until the stream
/// is driven — a lazy `Stream`, not a future, the one real gotcha here).
async fn ensure_image(docker: &Docker, image: &str) -> Result<(), OrchestratorError> {
    let options = CreateImageOptions {
        from_image: Some(image.to_string()),
        ..Default::default()
    };
    let mut pull = docker.create_image(Some(options), None, None);
    while let Some(event) = pull.next().await {
        event?;
    }
    Ok(())
}

/// Idempotent: safe to call whether the container has never existed, exists
/// but is stopped, or is already running.
pub async fn ensure_running(
    docker: &Docker,
    service: &ManagedService,
) -> Result<(), OrchestratorError> {
    if !container_exists(docker, &service.name).await? {
        ensure_image(docker, &service.image).await?;

        let config = ContainerCreateBody {
            image: Some(service.image.clone()),
            env: Some(service.env.clone()),
            healthcheck: service.healthcheck.as_ref().map(HealthConfig::from),
            ..Default::default()
        };
        let options = CreateContainerOptions {
            name: Some(service.name.clone()),
            ..Default::default()
        };
        docker.create_container(Some(options), config).await?;
    }

    docker
        .start_container(&service.name, None::<StartContainerOptions>)
        .await
        .or_else(|e| match e {
            // Already running is success, not an error, for an idempotent
            // "ensure" call.
            BollardError::DockerResponseServerError {
                status_code: 304, ..
            } => Ok(()),
            other => Err(other),
        })?;

    Ok(())
}

async fn container_state(
    docker: &Docker,
    name: &str,
) -> Result<Option<ContainerState>, OrchestratorError> {
    Ok(docker
        .inspect_container(name, None::<InspectContainerOptions>)
        .await?
        .state)
}

pub async fn status(docker: &Docker, name: &str) -> Result<ServiceStatus, OrchestratorError> {
    if !container_exists(docker, name).await? {
        return Ok(ServiceStatus::Absent);
    }
    let Some(state) = container_state(docker, name).await? else {
        return Ok(ServiceStatus::Absent);
    };
    if state.running != Some(true) {
        return Ok(ServiceStatus::Stopped);
    }
    match state.health.and_then(|h| h.status) {
        None | Some(HealthStatusEnum::NONE) | Some(HealthStatusEnum::EMPTY) => {
            Ok(ServiceStatus::RunningNoHealthcheck)
        }
        Some(HealthStatusEnum::STARTING) => Ok(ServiceStatus::Starting),
        Some(HealthStatusEnum::HEALTHY) => Ok(ServiceStatus::Healthy),
        Some(HealthStatusEnum::UNHEALTHY) => Ok(ServiceStatus::Unhealthy),
    }
}

/// Polls `status` until it's `Healthy` (or `RunningNoHealthcheck`, for a
/// service that doesn't define a healthcheck — "running" is all we can ask
/// of it), or gives up after `timeout`. This is the control plane's
/// reconciliation loop in miniature: observe, compare to desired state,
/// wait, repeat.
pub async fn wait_healthy(
    docker: &Docker,
    name: &str,
    timeout: Duration,
) -> Result<ServiceStatus, OrchestratorError> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let current = status(docker, name).await?;
        if matches!(
            current,
            ServiceStatus::Healthy | ServiceStatus::RunningNoHealthcheck
        ) {
            return Ok(current);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(OrchestratorError::HealthTimeout {
                service: name.to_string(),
                waited: timeout,
            });
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Idempotent: absent-to-begin-with is success, not an error.
pub async fn teardown(docker: &Docker, name: &str) -> Result<(), OrchestratorError> {
    if !container_exists(docker, name).await? {
        return Ok(());
    }

    docker
        .stop_container(name, None::<StopContainerOptions>)
        .await
        .or_else(|e| match e {
            BollardError::DockerResponseServerError {
                status_code: 304 | 404,
                ..
            } => Ok(()),
            other => Err(other),
        })?;

    docker
        .remove_container(name, None::<RemoveContainerOptions>)
        .await?;

    Ok(())
}
