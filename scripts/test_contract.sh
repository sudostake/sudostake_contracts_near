#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: test_contract.sh --module <name> [--unit] [--integration] [--suite <pattern>] [--] [cargo-test-args...]

Runs unit tests, integration tests, or both for the requested contract crate.
When integration tests are requested the corresponding Wasm is rebuilt before running.

Options:
  -m, --module <name>    Contract crate to test (e.g. vault, factory)
      --unit             Run unit tests only (defaults to running both when omitted)
      --integration      Run integration tests only (defaults to running both when omitted)
  -s, --suite <pattern>  Only run tests whose names contain <pattern>
  -h, --help             Show this help message
  --                     Pass remaining args directly to `cargo test`
EOF
  exit "${1:-0}"
}

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

ORIGINAL_ARGS=("$@")

MODULE=""
RUN_UNIT=false
RUN_INTEGRATION=false
SUITE_PATTERN=""
EXTRA_ARGS=()

while (($# > 0)); do
  case "$1" in
    -m|--module)
      MODULE="${2:-}"
      if [[ -z "${MODULE}" ]]; then
        echo "❌ --module requires a value." >&2
        usage 1
      fi
      shift 2
      ;;
    --unit)
      RUN_UNIT=true
      shift
      ;;
    --integration)
      RUN_INTEGRATION=true
      shift
      ;;
    -s|--suite)
      SUITE_PATTERN="${2:-}"
      if [[ -z "${SUITE_PATTERN}" ]]; then
        echo "❌ --suite requires a value." >&2
        usage 1
      fi
      shift 2
      ;;
    -h|--help)
      usage 0
      ;;
    --)
      shift
      EXTRA_ARGS=("$@")
      break
      ;;
    *)
      echo "❌ Unknown option: $1" >&2
      usage 1
      ;;
  esac
done

if [[ -z "${MODULE}" ]]; then
  echo "❌ --module must be specified." >&2
  usage 1
fi

if [[ "${RUN_UNIT}" = false && "${RUN_INTEGRATION}" = false ]]; then
  RUN_UNIT=true
  RUN_INTEGRATION=true
fi

MODULE="$(echo "${MODULE}" | tr '[:upper:]' '[:lower:]')"

RUST_FALLBACK_VERSION="1.86.0"
TOOLCHAIN_OVERRIDE=""
CRATE=""
MANIFEST=""
WASM_OUT_DIR=""
WASM_FILE=""
INTEGRATION_BUILD_FEATURES=()
INTEGRATION_TEST_FEATURES=()
NEEDS_PREPARE_RES=false
INTEGRATION_TEST_TARGETS=()
INTEGRATION_TEST_LABELS=()

if ! command -v cargo >/dev/null 2>&1; then
  echo "❌ cargo is required to run tests." >&2
  exit 1
fi

case "${MODULE}" in
  vault)
    CRATE="vault"
    MANIFEST="contracts/vault/Cargo.toml"
    WASM_OUT_DIR="vault_res"
    WASM_FILE="vault.wasm"
    INTEGRATION_BUILD_FEATURES=("integration-test")
    INTEGRATION_TEST_FEATURES=("integration-test")
    NEEDS_PREPARE_RES=true
    ;;
  factory)
    CRATE="factory"
    MANIFEST="contracts/factory/Cargo.toml"
    WASM_OUT_DIR="res"
    WASM_FILE="factory.wasm"
    INTEGRATION_BUILD_FEATURES=()
    INTEGRATION_TEST_FEATURES=("integration-test")
    NEEDS_PREPARE_RES=true
    ;;
  *)
    echo "❌ Unsupported module '${MODULE}'." >&2
    exit 1
    ;;
esac

TESTS_DIR="contracts/${MODULE}/tests"
if [[ -d "${TESTS_DIR}" ]]; then
  while IFS= read -r test_path; do
    test_file="$(basename "${test_path}")"
    case "${test_file}" in
      *_tests.rs|test_*.rs) ;;
      *) continue ;;
    esac

    if [[ -n "${SUITE_PATTERN}" && "${test_file}" != *"${SUITE_PATTERN}"* ]]; then
      continue
    fi

    test_name="${test_file%.rs}"
    INTEGRATION_TEST_TARGETS+=("${test_name}")
    label="${test_name%_tests}"
    if [[ -z "${label}" || "${label}" == "${test_name}" ]]; then
      INTEGRATION_TEST_LABELS+=("${test_name}")
    else
      INTEGRATION_TEST_LABELS+=("${label}")
    fi
  done < <(find "${TESTS_DIR}" -maxdepth 1 -type f -name "*.rs" | sort)
fi

detect_toolchain_override() {
  if [[ -n "${CARGO_NEAR_TOOLCHAIN_OVERRIDE:-}" ]]; then
    echo "ℹ️  Using CARGO_NEAR_TOOLCHAIN_OVERRIDE='${CARGO_NEAR_TOOLCHAIN_OVERRIDE}'"
    TOOLCHAIN_OVERRIDE="${CARGO_NEAR_TOOLCHAIN_OVERRIDE}"
    return
  fi

  if ! command -v rustup >/dev/null 2>&1; then
    cat <<EOF
⚠️  rustup not found; skipping automatic Rust ${RUST_FALLBACK_VERSION} fallback.
   You can manually set the toolchain override by exporting CARGO_NEAR_TOOLCHAIN_OVERRIDE=<toolchain> before running this script.
EOF
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
        echo "ℹ️  Overriding cargo-near toolchain with ${candidate}."
        TOOLCHAIN_OVERRIDE="${candidate}"
      elif rustup toolchain list | grep -qE "^${alt_candidate}(\s|\(|$)"; then
        echo "ℹ️  Overriding cargo-near toolchain with ${alt_candidate}."
        TOOLCHAIN_OVERRIDE="${alt_candidate}"
      else
        printf -v script_path_q '%q' "$0"
        if ((${#ORIGINAL_ARGS[@]} > 0)); then
          printf -v original_args_q ' %q' "${ORIGINAL_ARGS[@]}"
        else
          original_args_q=""
        fi
        cat <<EOF
❌ rustc ${release} is incompatible with the current nearcore VM. Install Rust ${RUST_FALLBACK_VERSION} and rerun:
    rustup toolchain install ${RUST_FALLBACK_VERSION}
    CARGO_NEAR_TOOLCHAIN_OVERRIDE=${RUST_FALLBACK_VERSION} ${script_path_q}${original_args_q}
EOF
        exit 1
      fi
    fi
  fi
}

run_cargo_near() {
  local description="$1"
  shift
  local logfile
  logfile="$(mktemp -t cargo_near.XXXXXX)"
  if ! "$@" >"$logfile" 2>&1; then
    echo "❌ ${description} failed. Full output:" >&2
    cat "$logfile" >&2
    rm -f "$logfile"
    exit 1
  fi
  rm -f "$logfile"
}

if "${RUN_UNIT}"; then
  echo "🧪 Running ${CRATE} unit tests..."
  unit_cmd=(cargo test -p "${CRATE}" --lib)
  if [[ -n "${SUITE_PATTERN}" ]]; then
    unit_cmd+=("${SUITE_PATTERN}")
  fi
  if ((${#EXTRA_ARGS[@]} > 0)); then
    unit_cmd+=("${EXTRA_ARGS[@]}")
  fi
  "${unit_cmd[@]}"
  if [[ -n "${SUITE_PATTERN}" ]]; then
    echo "✅ Unit tests passed for ${CRATE} (${SUITE_PATTERN})."
  else
    echo "✅ Unit tests passed for ${CRATE}."
  fi
fi

if "${RUN_INTEGRATION}"; then
  if [[ -z "${NEAR_SANDBOX_BIN_PATH:-}" ]]; then
    cat <<EOF
ℹ️  NEAR_SANDBOX_BIN_PATH is not set; near-workspaces will download the sandbox binary.
    Set it to reuse a local binary:
      export NEAR_SANDBOX_BIN_PATH="${ROOT_DIR}/bin/near-sandbox"
EOF
  elif [[ ! -x "${NEAR_SANDBOX_BIN_PATH}" ]]; then
    echo "❌ NEAR_SANDBOX_BIN_PATH points to '${NEAR_SANDBOX_BIN_PATH}', but it is not executable." >&2
    exit 1
  fi

  if ! command -v cargo near >/dev/null 2>&1; then
    echo "❌ cargo-near is required. Install it with 'cargo install cargo-near'." >&2
    exit 1
  fi

  if ! command -v wasm-opt >/dev/null 2>&1; then
    cat >&2 <<'EOF'
❌ wasm-opt not found. Install Binaryen using your package manager:
    macOS:                 brew install binaryen
    Ubuntu/Debian:         sudo apt install binaryen
    Fedora/RHEL/CentOS:    sudo dnf install binaryen
    RHEL/CentOS (legacy):  sudo yum install binaryen
    Arch Linux:            sudo pacman -S binaryen
EOF
    exit 1
  fi

  if "${NEEDS_PREPARE_RES}"; then
    "${ROOT_DIR}/scripts/prepare_res_dirs.sh" >/dev/null || true
  fi

  mkdir -p "${WASM_OUT_DIR}"

  detect_toolchain_override

  if [[ "${MODULE}" == "factory" ]] && [[ ! -f "res/vault.wasm" ]]; then
    echo "ℹ️  res/vault.wasm not found; rebuilding vault dependency..."
    dep_build_cmd=(
      cargo near build non-reproducible-wasm
      --locked
      --manifest-path contracts/vault/Cargo.toml
      --out-dir res
    )
    if [[ -n "${TOOLCHAIN_OVERRIDE}" ]]; then
      dep_build_cmd+=(--override-toolchain "${TOOLCHAIN_OVERRIDE}")
    fi
    run_cargo_near "Rebuilding res/vault.wasm" "${dep_build_cmd[@]}"
    echo "✅ Dependency res/vault.wasm rebuilt."
  fi

  echo "🔧 Rebuilding ${WASM_OUT_DIR}/${WASM_FILE} for integration tests..."
  build_cmd=(
    cargo near build non-reproducible-wasm
    --locked
    --manifest-path "${MANIFEST}"
    --out-dir "${WASM_OUT_DIR}"
  )
  if ((${#INTEGRATION_BUILD_FEATURES[@]} > 0)); then
    build_features="$(IFS=,; echo "${INTEGRATION_BUILD_FEATURES[*]}")"
    build_cmd+=(--features "${build_features}")
  fi
  if [[ -n "${TOOLCHAIN_OVERRIDE}" ]]; then
    build_cmd+=(--override-toolchain "${TOOLCHAIN_OVERRIDE}")
  fi
  run_cargo_near "Rebuilding ${WASM_OUT_DIR}/${WASM_FILE}" "${build_cmd[@]}"
  echo "✅ ${WASM_OUT_DIR}/${WASM_FILE} rebuilt."

  NATIVE_TARGET="$(rustc -vV | awk '/host:/ {print $2}')"
  echo "🧪 Running ${CRATE} integration tests on ${NATIVE_TARGET}..."
  test_cmd=(
    cargo test
    -p "${CRATE}"
    --release
    --target "${NATIVE_TARGET}"
  )
  if ((${#INTEGRATION_TEST_FEATURES[@]} > 0)); then
    test_features="$(IFS=,; echo "${INTEGRATION_TEST_FEATURES[*]}")"
    test_cmd+=(--features "${test_features}")
  fi
  if ((${#INTEGRATION_TEST_TARGETS[@]} > 0)); then
    for target in "${INTEGRATION_TEST_TARGETS[@]}"; do
      test_cmd+=(--test "${target}")
    done
  else
    test_cmd+=(--tests)
  fi
  if [[ -n "${SUITE_PATTERN}" ]]; then
    test_cmd+=("${SUITE_PATTERN}")
  fi
  if ((${#EXTRA_ARGS[@]} > 0)); then
    test_cmd+=("${EXTRA_ARGS[@]}")
  fi
  RUST_TEST_THREADS="${RUST_TEST_THREADS:-1}" \
    RUSTFLAGS="-C panic=unwind" \
    "${test_cmd[@]}"
  if ((${#INTEGRATION_TEST_TARGETS[@]} > 0)); then
    targets_readable="$(IFS=,; echo "${INTEGRATION_TEST_LABELS[*]}")"
    echo "✅ Integration tests passed for ${CRATE} (${targets_readable})."
  elif [[ -n "${SUITE_PATTERN}" ]]; then
    echo "✅ Integration tests passed for ${CRATE} (${SUITE_PATTERN})."
  else
    echo "✅ Integration tests passed for ${CRATE}."
  fi
fi
