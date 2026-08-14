#!/usr/bin/env bash
# Épingle vendor/opentelemetry-proto sur un commit/tag exact de
# open-telemetry/opentelemetry-proto, pour compilation via tonic/prost.
#
# Usage : scripts/pin-otlp-proto.sh <tag-ou-commit>
set -euo pipefail

REF="${1:?usage: scripts/pin-otlp-proto.sh <tag-ou-commit>}"
REPO_URL="https://github.com/open-telemetry/opentelemetry-proto.git"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT_DIR/vendor/opentelemetry-proto"

rm -rf "$DEST"
mkdir -p "$DEST"
git init -q "$DEST"
git -C "$DEST" remote add origin "$REPO_URL"
git -C "$DEST" fetch --depth 1 origin "$REF"
git -C "$DEST" checkout -q FETCH_HEAD
RESOLVED_SHA="$(git -C "$DEST" rev-parse FETCH_HEAD)"
rm -rf "$DEST/.git"

cat > "$DEST/PINNED_REF.md" <<EOF
Source : $REPO_URL
Ref demandée : $REF
Commit résolu : $RESOLVED_SHA
Épinglé le : $(date -u +%Y-%m-%dT%H:%M:%SZ)

Ne pas éditer ce répertoire à la main. Pour changer le pin :
scripts/pin-otlp-proto.sh <nouvelle-ref>

Le receiver tonic/prost (étape 2 du kernel) doit compiler les .proto depuis
ce répertoire, pas depuis une copie ad hoc ou une version différente
récupérée via une crate tierce.
EOF

echo "vendor/opentelemetry-proto épinglé sur $RESOLVED_SHA (ref demandée: $REF)"
