#!/bin/sh
set -eu
check_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$check_root"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$check_root/target-ui}"
export RUST_TEST_THREADS="${RUST_TEST_THREADS:-2}"
export TOKIO_WORKER_THREADS="${TOKIO_WORKER_THREADS:-2}"
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
