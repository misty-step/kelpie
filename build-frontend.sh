#!/bin/sh
set -eu

cargo build \
  --manifest-path frontend/Cargo.toml \
  --target-dir target/frontend \
  --target wasm32-unknown-unknown \
  --release

rm -rf static/wasm
wasm-bindgen \
  --target web \
  --out-dir static/wasm \
  --out-name kelpie_frontend \
  target/frontend/wasm32-unknown-unknown/release/kelpie_frontend.wasm
