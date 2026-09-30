#!/usr/bin/env bash
# Pins vendor/opentelemetry-proto to an exact commit or tag of
# open-telemetry/opentelemetry-proto, compiled through tonic/prost.
#
# Usage: scripts/pin-otlp-proto.sh <tag-or-commit>
set -euo pipefail

REF="${1:?usage: scripts/pin-otlp-proto.sh <tag-or-commit>}"
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
Source: $REPO_URL
Requested ref: $REF
Resolved commit: $RESOLVED_SHA
Pinned on: $(date -u +%Y-%m-%dT%H:%M:%SZ)

Do not edit this directory by hand. To change the pin:
scripts/pin-otlp-proto.sh <new-ref>

The tonic/prost receiver (kernel step 2) must compile the .proto files from
this directory, not from an ad hoc copy or a different version pulled in
through a third-party crate.
EOF

echo "vendor/opentelemetry-proto pinned to $RESOLVED_SHA (requested ref: $REF)"
