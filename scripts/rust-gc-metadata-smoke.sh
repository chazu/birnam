#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
target_dir="$repo/.cargo-target"
scratch=$(mktemp -d /tmp/birnam-rust-gc-metadata.XXXXXX)
trap 'rm -rf "$scratch"' EXIT
probe="$scratch/rust-gc-metadata-probe"

zig build-exe \
  --dep idapi \
  -Mroot="$repo/scripts/rust-gc-metadata-probe.zig" \
  -Midapi="$repo/src/id/idapi.zig" \
  -femit-bin="$probe"

probe_profile() {
  local profile=$1
  local dylib="$target_dir/$profile/libbirnam_id.dylib"
  local cargo_args=(build --workspace --locked --offline)
  if [[ $profile == release ]]; then
    cargo_args+=(--release)
  fi
  CARGO_TARGET_DIR="$target_dir" cargo "${cargo_args[@]}"
  "$probe" "$dylib" functional
}

expect_abort() {
  local mode=$1
  local diagnostic=$2
  local output="$scratch/$mode.log"
  set +e
  "$probe" "$target_dir/debug/libbirnam_id.dylib" "$mode" >"$output" 2>&1
  local status=$?
  set -e
  if [[ $status -ne 134 ]]; then
    echo "rust-gc-metadata: $mode exited $status, expected SIGABRT status 134" >&2
    cat "$output" >&2
    exit 1
  fi
  if ! grep -q "$diagnostic" "$output"; then
    echo "rust-gc-metadata: $mode omitted its diagnostic" >&2
    cat "$output" >&2
    exit 1
  fi
}

probe_profile debug
probe_profile release
expect_abort provider-overflow "gc: too many root providers"
expect_abort invalid-layout "setLayout kinds is null with nonzero count"

echo "rust-gc-metadata: capacity and validation failures terminated with SIGABRT"
