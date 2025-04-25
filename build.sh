#!/bin/bash
set -e

# Ensure wasm-opt is installed
if ! command -v wasm-opt &> /dev/null
then
    echo "❌ Error: wasm-opt not found. Please install Binaryen (brew install binaryen or apt install binaryen)." >&2
    exit 1
fi

# Ensure the res/ folder exists to store .wasm outputs
mkdir -p res

echo "📦 Building vault contract for wasm32-unknown-unknown..."
RUSTFLAGS='-C link-arg=--export-table -C link-arg=--export=__heap_base -C link-arg=--export=__data_end' \
cargo build -p vault --target wasm32-unknown-unknown --release
wasm-opt -Oz -o res/vault.wasm target/wasm32-unknown-unknown/release/vault.wasm

echo "📦 Building factory contract for wasm32-unknown-unknown..."
RUSTFLAGS='-C link-arg=--export-table -C link-arg=--export=__heap_base -C link-arg=--export=__data_end' \
cargo build -p factory --target wasm32-unknown-unknown --release
wasm-opt -Oz -o res/factory.wasm target/wasm32-unknown-unknown/release/factory.wasm

echo "✅ WASM build complete. Files available in ./res"
