#!/usr/bin/env bash
# End-to-end demo of the kernel: starts the containerised stack (ClickHouse +
# kernel + query-api), replays the two realistic fraudos scenarios, then
# queries the results through query-api. Everything running here is the same
# code as what is deployed (the same Docker images as docker-compose.stack.yml).
#
# Usage: scripts/demo.sh [--keep-running]
#   --keep-running   do not stop the stack at the end (by default it is
#                    stopped, so nothing is left running in the background).
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

KEEP_RUNNING=false
[[ "${1:-}" == "--keep-running" ]] && KEEP_RUNNING=true

# Must match the dev tokens fixed in docker-compose.stack.yml
# (docs/interfaces/kernel-auth.md).
export KERNEL_API_KEY=dev-kernel-key
QUERY_API_KEY=dev-query-key
AUTH_HEADER="Authorization: Bearer $QUERY_API_KEY"

section() { echo; echo "=== $1 ==="; }
pretty_json() { python3 -m json.tool 2>/dev/null || cat; }

section "1/5: starting the stack (ClickHouse + kernel + query-api, in containers)"
./scripts/dev-stack.sh up

cleanup() {
  if [[ "$KEEP_RUNNING" == false ]]; then
    section "5/5: stopping the stack"
    ./scripts/dev-stack.sh down
  else
    echo
    echo "Stack left running (--keep-running): http://localhost:8080, localhost:4317, http://localhost:8123"
  fi
}
trap cleanup EXIT

section "Waiting for query-api to answer"
for i in $(seq 1 30); do
  curl -s -o /dev/null -H "$AUTH_HEADER" "http://localhost:8080/metrics/summary" && break
  sleep 1
done

section "2/5: replaying two realistic fraudos scenarios (crates/fraudos-replay)"
echo "--- fraud_investigator_confirmed.json (confirmed fraud, escalated to Opus) ---"
cargo run --quiet -p fraudos-replay -- crates/fraudos-replay/fixtures/fraud_investigator_confirmed.json
echo
echo "--- compliance_officer_dismissed.json (case dismissed, one tool failed) ---"
cargo run --quiet -p fraudos-replay -- crates/fraudos-replay/fixtures/compliance_officer_dismissed.json

section "3/5: GET /traces (recent traces, derived on the fly from ClickHouse)"
curl -s -H "$AUTH_HEADER" "http://localhost:8080/traces?limit=5" | pretty_json

section "4/5: GET /traces/{trace_id} and GET /metrics/summary"
TRACE_ID=$(curl -s -H "$AUTH_HEADER" "http://localhost:8080/traces?limit=1" | python3 -c "import json,sys; print(json.load(sys.stdin)[0]['trace_id'])")
echo "--- tree of the most recent trace ($TRACE_ID) ---"
curl -s -H "$AUTH_HEADER" "http://localhost:8080/traces/$TRACE_ID" | pretty_json
echo
echo "--- metrics aggregated by kind ---"
curl -s -H "$AUTH_HEADER" "http://localhost:8080/metrics/summary" | pretty_json
