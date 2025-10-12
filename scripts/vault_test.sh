#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

RUST_FALLBACK_VERSION="1.86.0"

"${ROOT_DIR}/scripts/prepare_res_dirs.sh" >/dev/null || true

if [[ -z "${NEAR_SANDBOX_BIN_PATH:-}" ]]; then
  cat <<EOF
ℹ️  NEAR_SANDBOX_BIN_PATH is not set; near-workspaces will attempt to download the sandbox binary.
    If you need to run fully offline, download near-sandbox once and export:
      export NEAR_SANDBOX_BIN_PATH="${ROOT_DIR}/bin/near-sandbox"
EOF
else
  if [[ ! -x "${NEAR_SANDBOX_BIN_PATH}" ]]; then
    echo "❌ NEAR_SANDBOX_BIN_PATH points to '${NEAR_SANDBOX_BIN_PATH}', but it is not executable."
    exit 1
  fi
fi

if ! command -v cargo near >/dev/null 2>&1; then
  echo "❌ cargo-near is required. Install it with 'cargo install cargo-near'." >&2
  exit 1
fi

if ! command -v wasm-opt >/dev/null 2>&1; then
  echo "❌ wasm-opt not found. Install Binaryen (e.g. 'brew install binaryen' or 'sudo apt install binaryen')."
  exit 1
fi

mkdir -p vault_res

detect_toolchain_override() {
  if [[ -n "${CARGO_NEAR_TOOLCHAIN_OVERRIDE:-}" ]]; then
    echo "ℹ️  Using CARGO_NEAR_TOOLCHAIN_OVERRIDE='${CARGO_NEAR_TOOLCHAIN_OVERRIDE}'"
    TOOLCHAIN_OVERRIDE="${CARGO_NEAR_TOOLCHAIN_OVERRIDE}"
    return
  fi

  if ! command -v rustup >/dev/null 2>&1; then
    echo "⚠️  rustup not found; skipping automatic Rust ${RUST_FALLBACK_VERSION} fallback. Set CARGO_NEAR_TOOLCHAIN_OVERRIDE manually if needed."
    return
  fi

  local release host candidate alt_candidate
  release="$(rustc -vV | awk '/release:/ {print $2}')"
  host="$(rustc -vV | awk '/host:/ {print $2}')"

  if [[ "${release}" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+) ]]; then
    local major="${BASH_REMATCH[1]}"
    local minor="${BASH_REMATCH[2]}"
    if (( major > 1 )) || (( major == 1 && minor >= 87 )); then
      candidate="${RUST_FALLBACK_VERSION}-${host}"
      alt_candidate="${RUST_FALLBACK_VERSION}"
      if rustup toolchain list | grep -qE "^${candidate}(\s|\(|$)"; then
        echo "ℹ️  rustc ${release} detected; overriding cargo-near toolchain with ${candidate}."
        TOOLCHAIN_OVERRIDE="${candidate}"
      elif rustup toolchain list | grep -qE "^${alt_candidate}(\s|\(|$)"; then
        echo "ℹ️  rustc ${release} detected; overriding cargo-near toolchain with ${alt_candidate}."
        TOOLCHAIN_OVERRIDE="${alt_candidate}"
      else
        cat <<EOF
❌ rustc ${release} is incompatible with the current nearcore VM. Install Rust ${RUST_FALLBACK_VERSION} and rerun:
    rustup toolchain install ${RUST_FALLBACK_VERSION}
    CARGO_NEAR_TOOLCHAIN_OVERRIDE=${RUST_FALLBACK_VERSION} ${ROOT_DIR}/scripts/vault_test.sh
EOF
        exit 1
      fi
    fi
  fi
}

TOOLCHAIN_OVERRIDE=""
detect_toolchain_override

echo "🔧 Rebuilding vault.wasm with cargo-near (integration-test feature)..."
build_cmd=(
  cargo near build non-reproducible-wasm
  --locked
  --manifest-path contracts/vault/Cargo.toml
  --features integration-test
  --out-dir vault_res
)
if [[ -n "${TOOLCHAIN_OVERRIDE}" ]]; then
  build_cmd+=(--override-toolchain "${TOOLCHAIN_OVERRIDE}")
fi
"${build_cmd[@]}"
echo "✅ vault_res/vault.wasm rebuilt."

NATIVE_TARGET=$(rustc -vV | grep "host:" | awk '{print $2}')
echo "🧪 Running vault integration tests on native target: $NATIVE_TARGET"
RUST_TEST_THREADS="${RUST_TEST_THREADS:-1}" \
  RUSTFLAGS="-C panic=unwind" \
  cargo test \
  -p vault \
  --release \
  --features integration-test \
  --target "$NATIVE_TARGET"

echo "✅ All tests passed!"
