#!/usr/bin/env bash
# Reports the state of the external specs pinned in vendor/.
# Run it before coding an integration that touches one of these specs
# (see /contract and CLAUDE.md).
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR_DIR="$ROOT_DIR/vendor"

EXPECTED=("semconv-genai" "opentelemetry-proto")
STATUS=0

if [[ ! -d "$VENDOR_DIR" ]]; then
  echo "vendor/ missing: no external spec pinned yet."
  exit 1
fi

for name in "${EXPECTED[@]}"; do
  pin_file="$VENDOR_DIR/$name/PINNED_REF.md"
  if [[ -f "$pin_file" ]]; then
    echo "== $name =="
    grep -E "^(Source|Requested ref|Resolved commit|Pinned on):" "$pin_file"
    echo
  else
    echo "== $name: NOT PINNED =="
    echo "   -> scripts/pin-$([[ "$name" == "opentelemetry-proto" ]] && echo otlp-proto || echo semconv).sh <ref>"
    echo
    STATUS=1
  fi
done

exit $STATUS
