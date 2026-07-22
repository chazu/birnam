#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
target_dir="$repo/.cargo-target/ffi-test-hooks"
dylib="$target_dir/release/libbirnam_id.dylib"

CARGO_TARGET_DIR="$target_dir" \
  cargo build -p birnam-id --features ffi-test-hooks --release --locked --offline
test -f "$dylib"
zig run -O ReleaseSafe "$repo/scripts/rust-membership-bench.zig" -- "$dylib"
