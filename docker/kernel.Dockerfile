# Dossier step 6 (section 2.2), the deployment skeleton: builds the image of
# the `kernel` binary (otlp-receiver + clickhouse-sink wired together,
# crates/kernel). The image is the same artefact whatever the cloud; only the
# provisioning of the VM running it depends on that choice (ADR 0002).
#
# Build from the repository root (the context must include vendor/, read by
# crates/otlp-receiver/build.rs through a relative path):
#   docker build -f docker/kernel.Dockerfile -t retina-kernel .

FROM rust:1.97.1-slim-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p kernel

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/kernel /usr/local/bin/kernel

# The same defaults as crates/kernel/src/main.rs (env_or): the real
# deployment configuration comes from the provisioning's environment
# variables, not from a change to this image.
ENV KERNEL_BIND=0.0.0.0:4317
EXPOSE 4317
ENTRYPOINT ["/usr/local/bin/kernel"]
