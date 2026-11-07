#!/bin/sh
# Rebuilds the engine for the browser. --dev is much faster and is what to use while working.
set -e
cd "$(dirname "$0")/.."
wasm-pack build crates/casual-note-wasm --target web "${1:---release}" --out-dir ../../webapp/pkg --no-typescript
