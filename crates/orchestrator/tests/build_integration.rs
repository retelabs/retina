//! Runs against the real local Docker daemon and actually builds a real
//! venice image from its real Dockerfile — the slowest test in this crate
//! (a full `cargo build --release -p kernel` inside the builder stage), but
//! the only thing that proves `ImageSource::Build` does what
//! `docker build -f docker/kernel.Dockerfile .` used to do outside this
//! crate entirely. Not run by default `cargo test`:
//!
//!   cargo test -p orchestrator -- --ignored build_image

use bollard::Docker;
use orchestrator::docker_client::{
    ImageSource, ManagedService, ensure_network, ensure_running, teardown,
};

const NETWORK: &str = "venice-orchestrator-build-test-net";

/// `crates/orchestrator` -> repo root, the same build context
/// `docker/docker-compose.stack.yml` uses (`context: ..` relative to
/// `docker/`).
fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root should exist relative to CARGO_MANIFEST_DIR")
}

#[tokio::test]
#[ignore = "requires a local Docker daemon; builds a real image, takes a while"]
async fn build_image_produces_a_real_runnable_kernel_container() {
    let docker = Docker::connect_with_local_defaults().expect("failed to connect to Docker");
    ensure_network(&docker, NETWORK).await.unwrap();

    let service = ManagedService {
        name: "venice-orchestrator-build-test-kernel".to_string(),
        image: "venice-orchestrator-test-kernel:latest".to_string(),
        image_source: ImageSource::Build {
            context: repo_root(),
            dockerfile: "docker/kernel.Dockerfile".to_string(),
        },
        env: vec!["KERNEL_API_KEY=test-key".to_string()],
        healthcheck: None,
        ports: vec![],
        depends_on: vec![],
    };

    teardown(&docker, &service.name).await.unwrap();

    // The real claim: ensure_running builds the image via the /build API
    // (no external `docker build` involved) and then starts a container
    // from it. If the build fails, this fails here, not later with a
    // confusing "no such image" from create_container.
    ensure_running(&docker, NETWORK, &service)
        .await
        .expect("build + start should succeed");

    // It's not just "some container exists" — inspect the image bollard
    // itself just built and confirm the binary it produced is really
    // `kernel` (the container's entrypoint), proving the Dockerfile that
    // ran was the real one, not a stale/wrong image.
    let inspected = docker.inspect_image(&service.image).await.unwrap();
    let entrypoint = inspected
        .config
        .expect("built image should have a config")
        .entrypoint
        .expect("kernel.Dockerfile sets an ENTRYPOINT");
    assert_eq!(entrypoint, vec!["/usr/local/bin/kernel".to_string()]);

    teardown(&docker, &service.name).await.unwrap();
}
