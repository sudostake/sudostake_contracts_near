#!/bin/bash
set -e

# Ensure the res/ folder exists to store .wasm outputs
mkdir -p res

echo "📦 Building vault contract for wasm32-unknown-unknown..."
cargo build -p vault --target wasm32-unknown-unknown --release
wasm-opt -Oz -o res/vault.wasm target/wasm32-unknown-unknown/release/vault.wasm

echo "📦 Building factory contract for wasm32-unknown-unknown..."
cargo build -p factory --target wasm32-unknown-unknown --release
wasm-opt -Oz -o res/factory.wasm target/wasm32-unknown-unknown/release/factory.wasm

echo "✅ WASM build complete. Files available in ./res"
