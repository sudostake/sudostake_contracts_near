#!/bin/bash

./build.sh
RUST_LOG=debug cargo test --release -- --nocapture