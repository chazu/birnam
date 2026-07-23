#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
target_dir="$repo/.cargo-target/ffi-test-hooks"
scratch=$(mktemp -d /tmp/birnam-rust-ffi-abort.XXXXXX)
trap 'rm -rf "$scratch"' EXIT

probe="$scratch/rust-ffi-abort-probe"
zig build-exe "$repo/scripts/rust-ffi-abort-probe.zig" -femit-bin="$probe"

expect_abort() {
  local profile=$1
  local dylib=$2
  local symbol=$3
  local diagnostic=$4
  local output="$scratch/$profile-$symbol.log"

  set +e
  "$probe" "$dylib" "$symbol" >"$output" 2>&1
  local status=$?
  set -e

  if [[ $status -ne 134 ]]; then
    echo "rust-ffi-abort: $profile $symbol exited $status, expected SIGABRT status 134" >&2
    cat "$output" >&2
    exit 1
  fi
  if ! grep -q "$diagnostic" "$output"; then
    echo "rust-ffi-abort: $profile $symbol omitted its boundary diagnostic" >&2
    cat "$output" >&2
    exit 1
  fi
}

probe_profile() {
  local profile=$1
  local cargo_args=(build -p birnam-id --features ffi-test-hooks --locked --offline)
  if [[ $profile == release ]]; then cargo_args+=(--release); fi
  CARGO_TARGET_DIR="$target_dir" cargo "${cargo_args[@]}"
  local dylib="$target_dir/$profile/libbirnam_id.dylib"
  test -f "$dylib"
  expect_abort "$profile" "$dylib" birnam_id_test_panic_across_ffi "intentional Rust FFI panic"
  expect_abort "$profile" "$dylib" birnam_id_test_invariant_abort "intentional Rust FFI invariant failure"
  echo "rust-ffi-abort: $profile panic and invariant hooks terminated with SIGABRT"
}

probe_profile debug
probe_profile release
