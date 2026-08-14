#!/usr/bin/env bash
# Rapporte l'état des specs externes épinglées dans vendor/.
# À lancer avant de coder une intégration touchant l'une de ces specs
# (voir /contract et CLAUDE.md).
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR_DIR="$ROOT_DIR/vendor"

EXPECTED=("semconv-genai" "opentelemetry-proto")
STATUS=0

if [[ ! -d "$VENDOR_DIR" ]]; then
  echo "vendor/ absent — aucune spec externe épinglée pour l'instant."
  exit 1
fi

for name in "${EXPECTED[@]}"; do
  pin_file="$VENDOR_DIR/$name/PINNED_REF.md"
  if [[ -f "$pin_file" ]]; then
    echo "== $name =="
    grep -E "^(Source|Ref demandée|Commit résolu|Épinglé le)" "$pin_file"
    echo
  else
    echo "== $name : PAS ÉPINGLÉ =="
    echo "   -> scripts/pin-$([[ "$name" == "opentelemetry-proto" ]] && echo otlp-proto || echo semconv).sh <ref>"
    echo
    STATUS=1
  fi
done

exit $STATUS
