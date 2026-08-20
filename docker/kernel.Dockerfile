# Étape 6 (dossier section 2.2) — squelette de déploiement : construit
# l'image du binaire `kernel` (otlp-receiver + clickhouse-sink câblés,
# crates/kernel). Un seul cloud/une seule région pas encore choisis
# (dossier section 5) — cette image est le même artefact quel que soit le
# cloud, seul le provisionnement de la VM qui la fait tourner en dépendra.
#
# Build depuis la racine du repo (le contexte doit inclure vendor/, lu par
# crates/otlp-receiver/build.rs via un chemin relatif) :
#   docker build -f docker/kernel.Dockerfile -t venice-kernel .

FROM rust:1.97.1-slim-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p kernel

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/kernel /usr/local/bin/kernel

# Mêmes défauts que crates/kernel/src/main.rs (env_or) — la vraie
# configuration en déploiement viendra des variables d'environnement du
# provisionnement, pas d'un changement de cette image.
ENV KERNEL_BIND=0.0.0.0:4317
EXPOSE 4317
ENTRYPOINT ["/usr/local/bin/kernel"]
