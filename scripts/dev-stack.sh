#!/usr/bin/env bash
# Squelette de déploiement complet en local (étape 6) : ClickHouse + kernel +
# query-api, avec les mêmes images Docker que la cible de déploiement.
# Usage : scripts/dev-stack.sh up|down|logs
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMPOSE_FILE="$ROOT_DIR/docker/docker-compose.stack.yml"
ACTION="${1:-up}"

case "$ACTION" in
  up)
    docker compose -f "$COMPOSE_FILE" up --build -d
    echo "kernel (OTLP/gRPC) : localhost:4317"
    echo "query-api (HTTP)   : http://localhost:8080"
    echo "ClickHouse (HTTP)  : http://localhost:8123 (user: dev / pass: dev, db: observability)"
    ;;
  down)
    docker compose -f "$COMPOSE_FILE" down
    ;;
  logs)
    docker compose -f "$COMPOSE_FILE" logs -f
    ;;
  *)
    echo "usage: scripts/dev-stack.sh up|down|logs" >&2
    exit 1
    ;;
esac
