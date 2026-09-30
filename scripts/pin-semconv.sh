#!/usr/bin/env bash
# Pins vendor/semconv-genai to an exact commit or tag of
# open-telemetry/semantic-conventions-genai. Never track main continuously
# (design-dossier.md, section 2.1): these conventions are in Development
# status and would change under our feet otherwise.
#
# Usage: scripts/pin-semconv.sh <tag-or-commit>
set -euo pipefail

REF="${1:?usage: scripts/pin-semconv.sh <tag-or-commit>}"
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
Source: $REPO_URL
Requested ref: $REF
Resolved commit: $RESOLVED_SHA
Pinned on: $(date -u +%Y-%m-%dT%H:%M:%SZ)

Do not edit this directory by hand. To change the pin:
scripts/pin-semconv.sh <new-ref>

Any change to this pin must come with a review of the OTLP -> internal
model mapping layer (design dossier section 2.1) and an update of
docs/interfaces/semconv-genai.md.
EOF

echo "vendor/semconv-genai pinned to $RESOLVED_SHA (requested ref: $REF)"
