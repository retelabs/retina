# Étape 6 — image du binaire `query-api` (crates/query-api). Voir
# docker/kernel.Dockerfile pour le contexte général.
#
#   docker build -f docker/query-api.Dockerfile -t trellis-query-api .

FROM rust:1.97.1-slim-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p query-api

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/query-api /usr/local/bin/query-api

ENV QUERY_API_BIND=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/query-api"]
