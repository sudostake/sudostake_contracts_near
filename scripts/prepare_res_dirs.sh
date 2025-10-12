#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RES_DIR="${ROOT_DIR}/res"
THIRD_PARTY_DIR="${ROOT_DIR}/third_party/wasm"

mkdir -p "${RES_DIR}"

if [[ -d "${THIRD_PARTY_DIR}" ]]; then
  shopt -s nullglob
  copied_any=false
  for wasm in "${THIRD_PARTY_DIR}"/*.wasm; do
    copied_any=true
    cp "${wasm}" "${RES_DIR}/"
  done
  shopt -u nullglob
  if [[ "${copied_any}" == "true" ]]; then
    echo "ℹ️  Copied third-party wasm assets into res/."
  fi
fi
