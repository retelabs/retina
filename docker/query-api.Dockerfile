# Step 6: the image of the `query-api` binary (crates/query-api). See
# docker/kernel.Dockerfile for the general context.
#
#   docker build -f docker/query-api.Dockerfile -t retina-query-api .

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
