#!/usr/bin/env bash
# Compile les plugins WASM (crates/plugin-wasm-example et futurs équivalents)
# vers wasm32-unknown-unknown, requis avant les tests --ignored de
# crates/plugin-wasm-host. Voir docs/interfaces/wasm-plugin-loading.md.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
cargo build -p plugin-wasm-example --target wasm32-unknown-unknown --release

echo "Compilé : target/wasm32-unknown-unknown/release/plugin_wasm_example.wasm"
