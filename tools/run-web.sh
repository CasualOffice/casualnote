#!/bin/sh
# Builds the engine to WebAssembly and serves the web app.
set -e
cd "$(dirname "$0")/.."
wasm-pack build crates/casual-note-wasm --target web --release --out-dir ../../webapp/pkg --no-typescript
echo "Open http://127.0.0.1:${1:-8741}/"
cd webapp && exec python3 -m http.server "${1:-8741}" --bind 127.0.0.1
