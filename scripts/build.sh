#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "❌ cargo is not available in PATH." >&2
  exit 1
fi

if ! command -v cargo near >/dev/null 2>&1; then
  cat <<'EOF' >&2
❌ cargo-near is not installed.
   Install it with `cargo install cargo-near` or follow https://github.com/near/cargo-near.
EOF
  exit 1
fi

if ! command -v docker >/dev/null 2>&1; then
  cat <<'EOF' >&2
❌ Docker is required for reproducible builds.
   Install Docker Desktop or another Docker runtime and ensure it is running.
EOF
  exit 1
fi

OUT_DIR="res"
CONTRACTS=("vault" "factory")

mkdir -p "${OUT_DIR}"

echo "📦 Building contracts with cargo-near reproducible pipeline..."
for contract in "${CONTRACTS[@]}"; do
  manifest="contracts/${contract}/Cargo.toml"
  echo "🔁 Building ${contract}..."
  cargo near build reproducible-wasm \
    --manifest-path "${manifest}" \
    --out-dir "${OUT_DIR}"
done

echo "✅ Reproducible WASM artifacts written to ${OUT_DIR}"
