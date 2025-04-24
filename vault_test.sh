#!/bin/bash
set -e

# Detect native Rust target (e.g. x86_64-unknown-linux-gnu)
NATIVE_TARGET=$(rustc -vV | grep "host:" | awk '{print $2}')

# Ensure the vault_res/ folder exists for optimized wasm output
mkdir -p vault_res

echo "🔧 Rebuilding vault.wasm with integration-test feature (for sandbox test use)..."
cargo build -p vault --target wasm32-unknown-unknown --release --features integration-test

echo "🧪 Optimizing vault.wasm..."
wasm-opt -Oz -o vault_res/vault.wasm target/wasm32-unknown-unknown/release/vault.wasm

echo "✅ vault.wasm with integration-test feature rebuilt and optimized."

echo "🧪 Running vault integration tests on native target: $NATIVE_TARGET"
RUSTFLAGS="-C panic=unwind" cargo test \
  -p vault \
  --release \
  --features integration-test \
  --target "$NATIVE_TARGET"

echo "✅ All tests passed!"
