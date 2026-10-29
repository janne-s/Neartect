#!/bin/sh
set -e
cd "$(dirname "$0")/../core"
cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir ../src/core target/wasm32-unknown-unknown/release/neartect_core.wasm
wasm-opt -O3 --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals --enable-reference-types --enable-multivalue ../src/core/neartect_core_bg.wasm -o ../src/core/neartect_core_bg.wasm
