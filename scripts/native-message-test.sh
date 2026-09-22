#!/usr/bin/env bash
# Exercise converted production modules through Siphon and Id, without old tables.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/.cargo-target/native-message-tests}"
mkdir -p "$CARGO_TARGET_DIR"
CARGO_TARGET_DIR=$(cd "$CARGO_TARGET_DIR" && pwd)
export CARGO_TARGET_DIR
cargo build --offline --locked -p birnam-id -p birnam-root-registry -p birnam-namespace -p birnam-boolean -p birnam-nil
export BIRNAM_TEST_MODULE_DIRECTORY="$CARGO_TARGET_DIR/debug"
CARGO_PROFILE_DEV_PANIC=unwind cargo test --offline --locked -p birnam-siphon --test modules -- --include-ignored
