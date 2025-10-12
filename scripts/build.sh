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
"${ROOT_DIR}/scripts/prepare_res_dirs.sh" >/dev/null || true

echo "📦 Building contracts with cargo-near reproducible pipeline..."
for contract in "${CONTRACTS[@]}"; do
  manifest="contracts/${contract}/Cargo.toml"
  echo "🔁 Building ${contract}..."
  cargo near build reproducible-wasm \
    --manifest-path "${manifest}" \
    --out-dir "${OUT_DIR}"

  artifact="${OUT_DIR}/${contract}.wasm"
  abi_json_src="target/near/${contract}/${contract}_abi.json"
  abi_zst_src="target/near/${contract}/${contract}_abi.zst"

  for abi_src in "${abi_json_src}" "${abi_zst_src}"; do
    if [[ -f "${abi_src}" ]]; then
      cp "${abi_src}" "${OUT_DIR}/"
    fi
  done

  if [[ -f "${artifact}" ]]; then
    if command -v sha256sum >/dev/null 2>&1; then
      hash_line="$(sha256sum "${artifact}")"
    else
      hash_line="$(shasum -a 256 "${artifact}")"
    fi
    printf '%s\n' "${hash_line}" > "${artifact}.sha256"
    hash_value="${hash_line%% *}"
    echo "   SHA-256: ${hash_value}"
  else
    echo "⚠️  Expected artifact ${artifact} is missing." >&2
  fi
done

echo "✅ Reproducible WASM artifacts written to ${OUT_DIR}"
