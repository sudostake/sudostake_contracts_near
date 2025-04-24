#!/bin/bash
set -e

NATIVE_TARGET=$(rustc -vV | grep "host:" | awk '{print $2}')
RUSTFLAGS="-C panic=unwind" cargo test -p factory --release --target "$NATIVE_TARGET"

