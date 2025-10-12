#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="${ROOT_DIR}/bin"
SANDBOX_VERSION="${SANDBOX_VERSION:-2.6.2}"
SANDBOX_PATH="${BIN_DIR}/near-sandbox"

log_info()  { printf 'ℹ️  %s\n' "$*"; }
log_warn()  { printf '⚠️  %s\n' "$*" >&2; }
log_error() { printf '❌ %s\n' "$*" >&2; }
log_ok()    { printf '✅ %s\n' "$*"; }

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    log_error "Required command '$1' not found."
    return 1
  fi
}

detect_platform() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "${os}-${arch}" in
    Darwin-arm64) echo "Darwin-arm64" ;;
    Linux-x86_64) echo "Linux-x86_64" ;;
    *)
      log_error "Unsupported platform '${os}-${arch}'. near-sandbox binaries are published only for Darwin-arm64 and Linux-x86_64."
      return 1
      ;;
  esac
}

current_sandbox_version() {
  if [[ -x "${SANDBOX_PATH}" ]]; then
    "${SANDBOX_PATH}" --version 2>/dev/null | sed -E -n 's/^neard \(release ([0-9.]+)\).*/\1/p'
  fi
}

ensure_prerequisites() {
  require_command cargo || exit 1

  if command -v cargo-near >/dev/null 2>&1 || cargo near --help >/dev/null 2>&1; then
    log_ok "cargo-near detected."
  else
    log_warn "cargo-near is not installed. Install it with 'cargo install cargo-near'."
  fi

  if command -v docker >/dev/null 2>&1; then
    log_ok "Docker detected."
  else
    log_warn "Docker is not installed or not on PATH. Reproducible builds require Docker Desktop (or another daemon) to be running."
  fi

  if command -v wasm-opt >/dev/null 2>&1; then
    log_ok "wasm-opt detected."
  else
    log_warn "wasm-opt not found. Install Binaryen (e.g. 'brew install binaryen' or 'sudo apt install binaryen')."
  fi
}

download_near_sandbox() {
  local platform archive tmp_dir extracted_path
  platform="$(detect_platform)" || exit 1

  if [[ -x "${SANDBOX_PATH}" ]]; then
    local existing
    existing="$(current_sandbox_version)"
    if [[ -n "${existing}" && "${existing}" == "${SANDBOX_VERSION}" && "${SANDBOX_FORCE:-0}" != "1" ]]; then
      log_info "near-sandbox ${existing} already present at ${SANDBOX_PATH}."
      return
    fi
  fi

  require_command curl || exit 1
  mkdir -p "${BIN_DIR}"

  tmp_dir="$(mktemp -d)"
  archive="${tmp_dir}/near-sandbox.tar.gz"
  trap 'rm -rf "${tmp_dir}"' EXIT

  # Sandbox binaries are published to the NEAR build bucket; override SANDBOX_URL_BASE if the location changes.
  local SANDBOX_URL_BASE="${SANDBOX_URL_BASE:-https://s3-us-west-1.amazonaws.com/build.nearprotocol.com/nearcore}";
  local url="${SANDBOX_URL_BASE}/${platform}/${SANDBOX_VERSION}/near-sandbox.tar.gz"
  log_info "Downloading near-sandbox ${SANDBOX_VERSION} for ${platform}..."
  if ! curl -fLsS "${url}" -o "${archive}"; then
    log_error "Failed to download near-sandbox from ${url}"
    exit 1
  fi

  log_info "Extracting archive..."
  tar -xzf "${archive}" -C "${tmp_dir}"

  extracted_path="$(find "${tmp_dir}" -type f -name near-sandbox -perm -111 -print -quit)"
  if [[ -z "${extracted_path}" ]]; then
    log_error "Extracted archive does not contain a near-sandbox binary."
    exit 1
  fi

  cp "${extracted_path}" "${SANDBOX_PATH}"
  chmod +x "${SANDBOX_PATH}"
  log_ok "Installed near-sandbox ${SANDBOX_VERSION} to ${SANDBOX_PATH}"

  trap - EXIT
  rm -rf "${tmp_dir}"
}

print_env_hint() {
  cat <<EOF

Add the following to your shell profile to make the sandbox available automatically:
  export NEAR_SANDBOX_BIN_PATH="${SANDBOX_PATH}"

You can override the downloaded version by setting SANDBOX_VERSION or force a re-download with SANDBOX_FORCE=1.
EOF
}

main() {
  log_info "Checking prerequisites..."
  ensure_prerequisites
  download_near_sandbox
  print_env_hint
}

main "$@"
