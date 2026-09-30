#!/usr/bin/env bash
# Compiles the WASM plugins (crates/plugin-wasm-example and future ones)
# to wasm32-unknown-unknown, required before the --ignored tests of
# crates/plugin-wasm-host. See docs/interfaces/wasm-plugin-loading.md.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
cargo build -p plugin-wasm-example --target wasm32-unknown-unknown --release

echo "Built: target/wasm32-unknown-unknown/release/plugin_wasm_example.wasm"
