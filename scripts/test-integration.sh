#!/usr/bin/env bash
# Runs every integration test (#[ignore]) from scratch, with no stack set up
# by hand: a throwaway ClickHouse at the pinned version, the WASM plugin
# built, a real kernel plus a fraudos replay (real data for cost-model), a
# real query-api (for tui). Free ports are picked at launch: nothing touches
# the dev ClickHouse, dev-stack or anything else.
#
# Usage: scripts/test-integration.sh [--with-docker]
#   --with-docker: adds the orchestrator tests (a real Docker daemon; they
#                  create containers and build real images, several minutes).
# CONTAINER_CLI=podman to use podman instead of docker.
# IT_CLICKHOUSE_URL=http://clickhouse:8123: use this ClickHouse (a CI
#   service, user/pass dev/dev) instead of starting one.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CLI="${CONTAINER_CLI:-docker}"
# The same image as docker/docker-compose.*.yml (git grep clickhouse-server:).
IMAGE="clickhouse/clickhouse-server:26.8.15.10"
NAME="trellis-it-clickhouse-$$"
WITH_DOCKER=0
[[ "${1:-}" == "--with-docker" ]] && WITH_DOCKER=1

free_port() { python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'; }
CH_PORT=$(free_port) KERNEL_PORT=$(free_port) API_PORT=$(free_port)
export CLICKHOUSE_URL=http://127.0.0.1:$CH_PORT
export CLICKHOUSE_USER=dev CLICKHOUSE_PASSWORD=dev CLICKHOUSE_DATABASE=observability
export KERNEL_API_KEY=it-kernel-key QUERY_API_KEY=it-query-key
LOG_DIR="$(mktemp -d)"
PIDS=()

cleanup() {
  for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
  "$CLI" rm -f "$NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

wait_for() { # wait_for <description> <commande...>
  local what="$1"; shift
  for _ in $(seq 120); do
    "$@" >/dev/null 2>&1 && return 0
    # A process started here that dies (port taken, config): fail at once
    # rather than talk to something else on the same port.
    for pid in "${PIDS[@]}"; do
      kill -0 "$pid" 2>/dev/null || { echo "$what: process $pid died (logs: $LOG_DIR)" >&2; return 1; }
    done
    sleep 0.5
  done
  echo "timed out waiting for $what (logs: $LOG_DIR)" >&2
  return 1
}

if [[ -n "${IT_CLICKHOUSE_URL:-}" ]]; then
  export CLICKHOUSE_URL="$IT_CLICKHOUSE_URL"
  echo "== ClickHouse provided: $CLICKHOUSE_URL"
else
  echo "== ClickHouse $IMAGE"
  "$CLI" run -d --rm --name "$NAME" -p 127.0.0.1:$CH_PORT:8123 \
    -e CLICKHOUSE_DB=observability -e CLICKHOUSE_USER=dev -e CLICKHOUSE_PASSWORD=dev \
    -e CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT=1 "$IMAGE" >/dev/null
fi
wait_for ClickHouse curl -sf "$CLICKHOUSE_URL/ping"

echo "== WASM plugin"
scripts/build-wasm-plugins.sh >/dev/null

cargo build -q -p kernel -p query-api -p fraudos-replay

echo "== ClickHouse: sink, plugins, query-api, WASM host"
# Each suite applies the migrations itself.
cargo test -q -p clickhouse-sink -p plugin-sink -p query-api -p plugin-wasm-host -- --ignored

echo "== real kernel + fraudos replay, then cost-model"
KERNEL_BIND=127.0.0.1:$KERNEL_PORT target/debug/kernel >"$LOG_DIR/kernel.log" 2>&1 &
PIDS+=($!)
wait_for kernel bash -c 'exec 3<>/dev/tcp/127.0.0.1/'$KERNEL_PORT
KERNEL_ADDR=http://127.0.0.1:$KERNEL_PORT target/debug/fraudos-replay >"$LOG_DIR/replay.log" 2>&1
cargo test -q -p cost-model -- --ignored

echo "== real query-api, then tui"
QUERY_API_BIND=127.0.0.1:$API_PORT target/debug/query-api >"$LOG_DIR/query-api.log" 2>&1 &
PIDS+=($!)
wait_for query-api curl -s -o /dev/null http://127.0.0.1:$API_PORT/
QUERY_API_URL=http://127.0.0.1:$API_PORT cargo test -q -p tui -- --ignored

if [[ "$WITH_DOCKER" == 1 ]]; then
  echo "== orchestrator (Docker daemon)"
  cargo test -q -p orchestrator -- --ignored
else
  echo "== orchestrator: skipped (--with-docker to include it)"
fi
echo "OK: every requested integration test passed."
