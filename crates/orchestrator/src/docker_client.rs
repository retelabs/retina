//! Wraps the parts of the Docker Engine API this control plane needs:
//! ensure an image is present, ensure a container is running from it, poll
//! its health, tear it down. Every signature and field name here was read
//! from the real `bollard` 0.21.0 / `bollard-stubs` 1.53.1-rc source
//! (`~/.cargo/registry/src/.../bollard-0.21.0/src/container.rs` and
//! `bollard-stubs-.../src/models.rs`), not recalled from memory — bollard
//! renamed things across versions (`ContainerCreateBody` instead of the
//! older `Config<String>`), so this was worth checking for real.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use bollard::Docker;
use bollard::body_full;
use bollard::errors::Error as BollardError;
use bollard::models::{
    BuildInfo, ContainerCreateBody, ContainerState, HealthConfig, HealthStatusEnum, HostConfig,
    NetworkCreateRequest, NetworkCreateResponse, PortBinding,
};
use bollard::query_parameters::{
    BuildImageOptionsBuilder, CreateContainerOptions, CreateImageOptions, InspectContainerOptions,
    ListContainersOptions, ListNetworksOptions, RemoveContainerOptions, StartContainerOptions,
    StopContainerOptions,
};
use futures_util::StreamExt;

use crate::image_build::build_context_tar;

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
    /// `ImageSource::Local` and the image isn't there — for services that
    /// deliberately don't use `Build` (this control plane pulling their
    /// image is out of scope for them too), build via the existing
    /// Dockerfile path first.
    MissingLocalImage(String),
    /// Reading the build context (walking the directory, tarring it up)
    /// failed before Docker was ever involved — a local filesystem
    /// problem, not a daemon one.
    BuildContext(std::io::Error),
    /// The daemon accepted and ran the build, but it failed — `BuildInfo`'s
    /// own error message, not a transport-level `BollardError`.
    BuildFailed(String),
    /// A `depends_on` names a service that isn't in the batch being
    /// deployed — caught before touching Docker at all, not left to
    /// surface as a confusing runtime failure partway through a deploy.
    UnknownDependency {
        service: String,
        depends_on: String,
    },
    /// `depends_on` edges form a cycle — no valid deployment order exists.
    DependencyCycle,
}

impl fmt::Display for OrchestratorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrchestratorError::Docker(e) => write!(f, "docker API error: {e}"),
            OrchestratorError::HealthTimeout { service, waited } => {
                write!(f, "`{service}` did not report healthy within {waited:?}")
            }
            OrchestratorError::MissingLocalImage(image) => write!(
                f,
                "image `{image}` not found locally and is not pulled from a registry — build it first"
            ),
            OrchestratorError::BuildContext(e) => write!(f, "failed to read build context: {e}"),
            OrchestratorError::BuildFailed(message) => write!(f, "image build failed: {message}"),
            OrchestratorError::UnknownDependency {
                service,
                depends_on,
            } => write!(
                f,
                "`{service}` depends_on `{depends_on}`, which isn't in this deployment batch"
            ),
            OrchestratorError::DependencyCycle => {
                write!(f, "depends_on edges form a cycle — no valid deploy order")
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageSource {
    /// Pulled from a registry if not already present locally — what
    /// `clickhouse/clickhouse-server:latest` is.
    Registry,
    /// Must already exist locally — for a service whose image this control
    /// plane is deliberately not responsible for at all (neither pulled
    /// nor built here).
    Local,
    /// Built from a Dockerfile via the real `/build` Engine API endpoint —
    /// what `kernel`/`query-api` use now, replacing the `docker build`
    /// step that used to happen outside this crate entirely.
    Build {
        /// Build context root, tarred up in memory
        /// (`crates/orchestrator/src/image_build.rs`) respecting
        /// `.dockerignore` at that root.
        context: PathBuf,
        /// Dockerfile path, relative to `context` — matches the `-f` flag
        /// of `docker build` / the `dockerfile:` key of a compose service.
        dockerfile: String,
    },
}

/// A published `container_port -> host_port` mapping, TCP only (the only
/// protocol either `kernel` or `query-api` speaks).
pub struct PortSpec {
    pub container_port: u16,
    pub host_port: u16,
}

pub struct ManagedService {
    pub name: String,
    pub image: String,
    pub image_source: ImageSource,
    pub env: Vec<String>,
    pub healthcheck: Option<HealthCheckSpec>,
    pub ports: Vec<PortSpec>,
    /// Names of other `ManagedService`s (in the same deploy batch) that
    /// must be healthy before this one starts — `deploy_all` reads this to
    /// order the rollout, matching `depends_on: condition: service_healthy`
    /// in `docker/docker-compose.stack.yml`.
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
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

/// For `Registry`: pulls `image` if the daemon doesn't already have it.
/// Streams progress events (`create_image` doesn't actually pull anything
/// until the stream is driven — a lazy `Stream`, not a future, the one real
/// gotcha here). For `Local`: just confirms it's there, or reports
/// precisely which image is missing rather than letting `create_container`
/// fail later with a less specific error.
async fn ensure_image(
    docker: &Docker,
    image: &str,
    source: &ImageSource,
) -> Result<(), OrchestratorError> {
    match source {
        ImageSource::Registry => {
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
        ImageSource::Local => docker
            .inspect_image(image)
            .await
            .map(|_| ())
            .map_err(|e| match e {
                BollardError::DockerResponseServerError {
                    status_code: 404, ..
                } => OrchestratorError::MissingLocalImage(image.to_string()),
                other => OrchestratorError::Docker(other),
            }),
        ImageSource::Build {
            context,
            dockerfile,
        } => build_image(docker, context, dockerfile, image).await,
    }
}

/// Builds `image` from `dockerfile` (relative to `context`) via the real
/// `/build` Engine API endpoint. Always runs — unlike `Registry`'s
/// "pull only if missing", a build is expected to run on every deploy the
/// same way `docker build`/`docker compose build` do; Docker's own layer
/// cache is what keeps a rebuild of an unchanged context fast, not a
/// decision made here.
async fn build_image(
    docker: &Docker,
    context: &std::path::Path,
    dockerfile: &str,
    tag: &str,
) -> Result<(), OrchestratorError> {
    let tar = build_context_tar(context).map_err(OrchestratorError::BuildContext)?;

    let options = BuildImageOptionsBuilder::default()
        .dockerfile(dockerfile)
        .t(tag)
        .rm(true)
        .build();

    let mut build = docker.build_image(options, None, Some(body_full(tar.into())));
    while let Some(event) = build.next().await {
        let info: BuildInfo = event?;
        if let Some(message) = info.error_detail.and_then(|d| d.message) {
            return Err(OrchestratorError::BuildFailed(message));
        }
    }
    Ok(())
}

/// Same shape as `bollard::models::PortMap`, spelled out locally so
/// `port_config`'s signature doesn't trip `clippy::type_complexity`.
type PortBindings = HashMap<String, Option<Vec<PortBinding>>>;

/// Builds the `{"<port>/tcp": {}}` exposed-ports declaration and the
/// `HostConfig.port_bindings` map that actually publishes to the host — the
/// real Engine API wants both (`ExposedPorts` declares the port exists,
/// `PortBindings` is what maps it out), matching what `docker run -p`
/// sets under the hood.
fn port_config(ports: &[PortSpec]) -> (Option<Vec<String>>, Option<PortBindings>) {
    if ports.is_empty() {
        return (None, None);
    }
    let mut exposed = Vec::with_capacity(ports.len());
    let mut bindings = HashMap::with_capacity(ports.len());
    for p in ports {
        let key = format!("{}/tcp", p.container_port);
        exposed.push(key.clone());
        bindings.insert(
            key,
            Some(vec![PortBinding {
                host_ip: None,
                host_port: Some(p.host_port.to_string()),
            }]),
        );
    }
    (Some(exposed), Some(bindings))
}

/// Idempotent: safe to call whether the container has never existed, exists
/// but is stopped, or is already running. Attaches the container to
/// `network` (a user-defined network — required for containers to resolve
/// each other by name; Docker's default `bridge` network does *not* give
/// containers DNS resolution by name, only a user-defined one does, which
/// is why `deploy_all` always creates one via `ensure_network` first).
///
/// Known limitation: `ensure_image` (including a real `Build`) only runs
/// when the container doesn't exist yet — a container already running from
/// an older image build is not recreated from a fresher one. Rebuilding
/// picked-up code changes today means tearing the container down first;
/// detecting "the image actually changed" and recreating on top of that is
/// a real reconciliation feature, not attempted here.
pub async fn ensure_running(
    docker: &Docker,
    network: &str,
    service: &ManagedService,
) -> Result<(), OrchestratorError> {
    if !container_exists(docker, &service.name).await? {
        ensure_image(docker, &service.image, &service.image_source).await?;

        let (exposed_ports, port_bindings) = port_config(&service.ports);
        let config = ContainerCreateBody {
            image: Some(service.image.clone()),
            env: Some(service.env.clone()),
            healthcheck: service.healthcheck.as_ref().map(HealthConfig::from),
            exposed_ports,
            host_config: Some(HostConfig {
                network_mode: Some(network.to_string()),
                port_bindings,
                ..Default::default()
            }),
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

/// Idempotent: creates the user-defined network `deploy_all` attaches every
/// managed container to, unless it already exists.
///
/// The check-then-create below is a real TOCTOU race, not a hypothetical
/// one — found by this crate's own test suite, where two `#[tokio::test]`s
/// both call `ensure_network` on the same name and run concurrently by
/// default. Both see "doesn't exist" from `list_networks` before either has
/// created it, both call `create_network`, and the loser gets a 409. Rather
/// than serialize the check (which wouldn't fully close the race against a
/// second *process* anyway), the fix is the same shape as
/// `ensure_running`'s already-started handling: treat "already exists" as
/// success, because for an idempotent "ensure" call that's exactly what it
/// is.
pub async fn ensure_network(docker: &Docker, name: &str) -> Result<(), OrchestratorError> {
    let options = ListNetworksOptions {
        filters: Some(HashMap::from([(
            "name".to_string(),
            vec![name.to_string()],
        )])),
    };
    let existing = docker.list_networks(Some(options)).await?;
    if existing.iter().any(|n| n.name.as_deref() == Some(name)) {
        return Ok(());
    }
    docker
        .create_network(NetworkCreateRequest {
            name: name.to_string(),
            ..Default::default()
        })
        .await
        .or_else(|e| match e {
            BollardError::DockerResponseServerError {
                status_code: 409, ..
            } => Ok(NetworkCreateResponse::default()),
            other => Err(other),
        })?;
    Ok(())
}

/// Orders `services` so every dependency comes before its dependents
/// (Kahn's algorithm) — deterministic for a fixed input order, since ties
/// are broken by the services' original position rather than by iterating
/// a `HashMap`.
fn topological_order(
    services: &[ManagedService],
) -> Result<Vec<&ManagedService>, OrchestratorError> {
    let mut in_degree: HashMap<&str, usize> =
        services.iter().map(|s| (s.name.as_str(), 0)).collect();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

    for s in services {
        for dep in &s.depends_on {
            if !in_degree.contains_key(dep.as_str()) {
                return Err(OrchestratorError::UnknownDependency {
                    service: s.name.clone(),
                    depends_on: dep.clone(),
                });
            }
            *in_degree.get_mut(s.name.as_str()).unwrap() += 1;
            dependents.entry(dep.as_str()).or_default().push(&s.name);
        }
    }

    let mut queue: std::collections::VecDeque<&str> = services
        .iter()
        .filter(|s| in_degree[s.name.as_str()] == 0)
        .map(|s| s.name.as_str())
        .collect();

    let mut ordered_names = Vec::with_capacity(services.len());
    while let Some(name) = queue.pop_front() {
        ordered_names.push(name);
        if let Some(deps) = dependents.get(name) {
            for &dependent in deps {
                let entry = in_degree.get_mut(dependent).unwrap();
                *entry -= 1;
                if *entry == 0 {
                    queue.push_back(dependent);
                }
            }
        }
    }

    if ordered_names.len() != services.len() {
        return Err(OrchestratorError::DependencyCycle);
    }

    let by_name: HashMap<&str, &ManagedService> =
        services.iter().map(|s| (s.name.as_str(), s)).collect();
    Ok(ordered_names.into_iter().map(|n| by_name[n]).collect())
}

/// Deploys a batch of services in dependency order: creates the shared
/// network, then for each service (dependencies before dependents) ensures
/// it's running and waits for it to be healthy before moving on to
/// whatever depends on it. By the time a dependent's `ensure_running` runs,
/// everything it `depends_on` is already healthy — exactly what
/// `docker-compose`'s `depends_on: condition: service_healthy` gives you,
/// implemented from the API up instead of consumed as a compose feature.
pub async fn deploy_all(
    docker: &Docker,
    network: &str,
    services: &[ManagedService],
) -> Result<(), OrchestratorError> {
    ensure_network(docker, network).await?;

    for service in topological_order(services)? {
        ensure_running(docker, network, service).await?;
        wait_healthy(docker, &service.name, Duration::from_secs(60)).await?;
    }

    Ok(())
}

/// Tears down a batch in reverse dependency order (dependents before their
/// dependencies) — cleaner shutdown, though Docker itself doesn't enforce
/// this ordering the way it enforces creation-time network attachment.
pub async fn teardown_all(
    docker: &Docker,
    services: &[ManagedService],
) -> Result<(), OrchestratorError> {
    for service in topological_order(services)?.into_iter().rev() {
        teardown(docker, &service.name).await?;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn service(name: &str, depends_on: &[&str]) -> ManagedService {
        ManagedService {
            name: name.to_string(),
            image: "unused:latest".to_string(),
            image_source: ImageSource::Registry,
            env: vec![],
            healthcheck: None,
            ports: vec![],
            depends_on: depends_on.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn topological_order_places_dependencies_before_dependents() {
        // Deliberately listed out of order (kernel/query-api before their
        // dependency) — the sort, not input order, must produce this.
        let services = vec![
            service("kernel", &["clickhouse"]),
            service("query-api", &["clickhouse"]),
            service("clickhouse", &[]),
        ];
        let order: Vec<&str> = topological_order(&services)
            .unwrap()
            .iter()
            .map(|s| s.name.as_str())
            .collect();

        let clickhouse_pos = order.iter().position(|&n| n == "clickhouse").unwrap();
        let kernel_pos = order.iter().position(|&n| n == "kernel").unwrap();
        let query_api_pos = order.iter().position(|&n| n == "query-api").unwrap();
        assert!(clickhouse_pos < kernel_pos);
        assert!(clickhouse_pos < query_api_pos);
    }

    #[test]
    fn topological_order_rejects_a_dependency_outside_the_batch() {
        let services = vec![service("kernel", &["ghost"])];
        assert!(matches!(
            topological_order(&services),
            Err(OrchestratorError::UnknownDependency { .. })
        ));
    }

    #[test]
    fn topological_order_rejects_a_cycle() {
        let services = vec![service("a", &["b"]), service("b", &["a"])];
        assert!(matches!(
            topological_order(&services),
            Err(OrchestratorError::DependencyCycle)
        ));
    }

    #[test]
    fn topological_order_is_deterministic_for_independent_services() {
        // No edges between "b" and "c" — several orders are valid
        // topologically, but the function should still return the same one
        // every time (input order breaks ties), not vary run to run.
        let services = vec![
            service("a", &[]),
            service("b", &["a"]),
            service("c", &["a"]),
        ];
        let first: Vec<&str> = topological_order(&services)
            .unwrap()
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        let second: Vec<&str> = topological_order(&services)
            .unwrap()
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(first, second);
    }
}
