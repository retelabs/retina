#!/usr/bin/env bash
# A local ClickHouse instance for development (kernel MVP step 3).
# Usage: scripts/dev-clickhouse.sh up|down|logs
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMPOSE_FILE="$ROOT_DIR/docker/docker-compose.clickhouse.yml"
ACTION="${1:-up}"

case "$ACTION" in
  up)
    docker compose -f "$COMPOSE_FILE" up -d
    echo "Dev ClickHouse ready: http://localhost:8123 (user: dev / pass: dev, db: observability)"
    ;;
  down)
    docker compose -f "$COMPOSE_FILE" down
    ;;
  logs)
    docker compose -f "$COMPOSE_FILE" logs -f
    ;;
  *)
    echo "usage: scripts/dev-clickhouse.sh up|down|logs" >&2
    exit 1
    ;;
esac
