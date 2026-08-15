#!/usr/bin/env bash
# Démo de bout en bout du kernel : lance la pile conteneurisée (ClickHouse +
# kernel + query-api), rejoue les 2 scénarios fraudos réalistes, puis
# interroge les résultats via query-api. Tout ce qui tourne ici est le même
# code que celui déployé (mêmes images Docker que docker-compose.stack.yml).
#
# Usage : scripts/demo.sh [--keep-running]
#   --keep-running   ne pas arrêter la pile à la fin (par défaut, elle est
#                     coupée pour ne rien laisser tourner en arrière-plan).
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

KEEP_RUNNING=false
[[ "${1:-}" == "--keep-running" ]] && KEEP_RUNNING=true

# Doit correspondre aux jetons dev fixés dans docker-compose.stack.yml
# (docs/interfaces/kernel-auth.md).
export KERNEL_API_KEY=dev-kernel-key
QUERY_API_KEY=dev-query-key
AUTH_HEADER="Authorization: Bearer $QUERY_API_KEY"

section() { echo; echo "=== $1 ==="; }
pretty_json() { python3 -m json.tool 2>/dev/null || cat; }

section "1/5 — démarrage de la pile (ClickHouse + kernel + query-api, en conteneurs)"
./scripts/dev-stack.sh up

cleanup() {
  if [[ "$KEEP_RUNNING" == false ]]; then
    section "5/5 — arrêt de la pile"
    ./scripts/dev-stack.sh down
  else
    echo
    echo "Pile laissée en route (--keep-running) : http://localhost:8080, localhost:4317, http://localhost:8123"
  fi
}
trap cleanup EXIT

section "Attente que query-api réponde"
for i in $(seq 1 30); do
  curl -s -o /dev/null -H "$AUTH_HEADER" "http://localhost:8080/metrics/summary" && break
  sleep 1
done

section "2/5 — rejeu de 2 scénarios fraudos réalistes (crates/fraudos-replay)"
echo "--- fraud_investigator_confirmed.json (fraude confirmée, escaladée vers Opus) ---"
cargo run --quiet -p fraudos-replay -- crates/fraudos-replay/fixtures/fraud_investigator_confirmed.json
echo
echo "--- compliance_officer_dismissed.json (dossier classé, un outil en échec) ---"
cargo run --quiet -p fraudos-replay -- crates/fraudos-replay/fixtures/compliance_officer_dismissed.json

section "3/5 — GET /traces (traces récentes, dérivées à la volée depuis ClickHouse)"
curl -s -H "$AUTH_HEADER" "http://localhost:8080/traces?limit=5" | pretty_json

section "4/5 — GET /traces/{trace_id} et GET /metrics/summary"
TRACE_ID=$(curl -s -H "$AUTH_HEADER" "http://localhost:8080/traces?limit=1" | python3 -c "import json,sys; print(json.load(sys.stdin)[0]['trace_id'])")
echo "--- arbre de la trace la plus récente ($TRACE_ID) ---"
curl -s -H "$AUTH_HEADER" "http://localhost:8080/traces/$TRACE_ID" | pretty_json
echo
echo "--- métriques agrégées par kind ---"
curl -s -H "$AUTH_HEADER" "http://localhost:8080/metrics/summary" | pretty_json
