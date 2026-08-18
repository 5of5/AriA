#!/usr/bin/env bash
# Build the WASM surface for both the browser demo and the Node parity check.
#
#   ./scripts/build_wasm.sh
#   node www/parity.mjs
#   python3 -m http.server -d www    # then open http://localhost:8000
set -euo pipefail

cd "$(dirname "$0")/.."

WASM=target/wasm32-unknown-unknown/release/aria_engine_wasm.wasm

cargo build -p aria-engine-wasm --target wasm32-unknown-unknown --release

# Browser demo (ES module with an async `init` default export).
wasm-bindgen "$WASM" --out-dir www/pkg --target web

# Node harness (CommonJS-interop module used by www/parity.mjs).
wasm-bindgen "$WASM" --out-dir www/pkg-node --target nodejs

echo "wasm built: www/pkg (web), www/pkg-node (nodejs)"
