#!/bin/bash
set -e

# Ensure the res/ folder exists
mkdir -p res

# Build all workspace contracts
cargo build --target wasm32-unknown-unknown --release

# Rebuild vault with integration-test feature
cargo build -p vault --target wasm32-unknown-unknown --release --features integration-test

# Optimize WASM binaries
for file in target/wasm32-unknown-unknown/release/*.wasm; do
    filename=$(basename "$file")
    wasm-opt -Oz -o "res/$filename" "$file"
done

echo "✅ Build complete. Optimized WASM files are in the res/ folder."
