#!/usr/bin/env bash
set -euo pipefail

# Applies the recommended NEAR sandbox kernel parameters on Linux hosts.

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "Detected non-Linux platform ($(uname -s)); no kernel tweaks required."
  exit 0
fi

if [[ "${EUID}" -ne 0 ]]; then
  echo "This script must be run as root. Re-run with: sudo bash $0" >&2
  exit 1
fi

declare -a PARAMS=(
  "net.core.rmem_max=8388608"
  "net.core.wmem_max=8388608"
  "net.ipv4.tcp_rmem=4096 87380 8388608"
  "net.ipv4.tcp_wmem=4096 16384 8388608"
  "net.ipv4.tcp_slow_start_after_idle=0"
)

for param in "${PARAMS[@]}"; do
  key="${param%%=*}"
  value="${param#*=}"
  sysctl -w "${key}=${value}" >/dev/null
  echo "Applied ${key}=${value}"
done

echo "All kernel parameters applied successfully."
