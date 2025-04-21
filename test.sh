#!/bin/bash
set -e

./build.sh
cargo test -p vault --release --features integration-test
cargo test -p factory --release