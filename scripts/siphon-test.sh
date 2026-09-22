#!/usr/bin/env bash
# Test the production Siphon/Id boundary without the upper language stack.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/.cargo-target/siphon-tests}"
mkdir -p "$CARGO_TARGET_DIR"
CARGO_TARGET_DIR=$(cd "$CARGO_TARGET_DIR" && pwd)
export CARGO_TARGET_DIR
cargo build --offline --locked -p birnam-id
case $(uname -s) in
  Darwin) library=libbirnam_id.dylib ;;
  *) library=libbirnam_id.so ;;
esac
export BIRNAM_TEST_ID_LIBRARY="$CARGO_TARGET_DIR/debug/$library"
CARGO_PROFILE_DEV_PANIC=unwind cargo test --offline --locked -p birnam-siphon --test id -- --include-ignored
