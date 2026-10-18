#!/bin/sh
set -eu
# No runtime sockets, account homes or network are exposed to this build.
docker run --rm --network none \
  -v "$PWD:/work" \
  -v /mnt/user/appdata/codex-switcher-build/cargo/registry:/usr/local/cargo/registry:ro \
  -e CARGO_TARGET_DIR=/work/target-ui -e RUST_TEST_THREADS=2 \
  -w /work sessionsation-build-tools:local \
  sh -c 'cargo fmt --check && cargo clippy --offline --locked --all-targets -- -D warnings && cargo test --offline --locked'
