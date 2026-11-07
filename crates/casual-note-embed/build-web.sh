#!/bin/sh
# Builds the browser's inference module into webapp/pkg/embed, separate from the main engine and
# loaded only after a person installs the model. Always release: a dev build is unusably slow.
# opt-level 3 and SIMD matter here, unlike for the engine: see the numbers in docs/30, PERF-08.
set -e
cd "$(dirname "$0")/../.."
CARGO_PROFILE_RELEASE_OPT_LEVEL=3 \
RUSTFLAGS='-C target-feature=+simd128 --cfg getrandom_backend="wasm_js"' \
wasm-pack build crates/casual-note-embed --target web --release --out-dir "${1:-../../webapp/pkg/embed}" --no-typescript -- --features web
