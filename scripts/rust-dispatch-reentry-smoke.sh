#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
target_dir="$repo/.cargo-target/ffi-test-hooks"

probe_profile() {
  local profile=$1
  local dylib="$target_dir/$profile/libbirnam_id.dylib"
  local cargo_args=(build -p birnam-id --features ffi-test-hooks --locked --offline)
  if [[ $profile == release ]]; then
    cargo_args+=(--release)
  fi

  CARGO_TARGET_DIR="$target_dir" cargo "${cargo_args[@]}"
  test -f "$dylib"
  zig run "$repo/scripts/rust-dispatch-reentry-probe.zig" -- "$dylib"
}

probe_profile debug
probe_profile release
