#!/bin/sh
set -e
cd "$(dirname "$0")/../core"
cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir ../src/core target/wasm32-unknown-unknown/release/neartect_core.wasm
