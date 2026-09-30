#!/usr/bin/env bash
# Épingle vendor/semconv-genai sur un commit/tag exact de
# open-telemetry/semantic-conventions-genai. Ne jamais suivre main en continu
# (design-dossier.md, section 2.1) : ces conventions sont en
# statut Development et changent sous nos pieds sinon.
#
# Usage : scripts/pin-semconv.sh <tag-ou-commit>
set -euo pipefail

REF="${1:?usage: scripts/pin-semconv.sh <tag-ou-commit>}"
REPO_URL="https://github.com/open-telemetry/semantic-conventions-genai.git"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT_DIR/vendor/semconv-genai"

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
scripts/pin-semconv.sh <nouvelle-ref>

Toute modification de ce pin doit s'accompagner d'une relecture de la couche
de mapping OTLP -> modèle interne (dossier section 2.1) et d'une mise à jour
de docs/interfaces/semconv-genai.md.
EOF

echo "vendor/semconv-genai épinglé sur $RESOLVED_SHA (ref demandée: $REF)"
