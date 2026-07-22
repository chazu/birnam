#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
target_dir="$repo/.cargo-target/ffi-test-hooks"
dylib="$target_dir/debug/libbirnam_id.dylib"
scratch=$(mktemp -d /tmp/birnam-rust-ffi-abort.XXXXXX)
trap 'rm -rf "$scratch"' EXIT

CARGO_TARGET_DIR="$target_dir" \
  cargo build -p birnam-id --features ffi-test-hooks --locked --offline
test -f "$dylib"

probe="$scratch/rust-ffi-abort-probe"
zig build-exe "$repo/scripts/rust-ffi-abort-probe.zig" -femit-bin="$probe"

expect_abort() {
  local symbol=$1
  local diagnostic=$2
  local output="$scratch/$symbol.log"

  set +e
  "$probe" "$dylib" "$symbol" >"$output" 2>&1
  local status=$?
  set -e

  if [[ $status -ne 134 ]]; then
    echo "rust-ffi-abort: $symbol exited $status, expected SIGABRT status 134" >&2
    cat "$output" >&2
    exit 1
  fi
  if ! grep -q "$diagnostic" "$output"; then
    echo "rust-ffi-abort: $symbol omitted its boundary diagnostic" >&2
    cat "$output" >&2
    exit 1
  fi
}

expect_abort birnam_id_test_panic_across_ffi "intentional Rust FFI panic"
expect_abort birnam_id_test_invariant_abort "intentional Rust FFI invariant failure"

echo "rust-ffi-abort: panic and invariant hooks terminated with SIGABRT"
