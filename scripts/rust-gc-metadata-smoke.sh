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
  "$probe" "$dylib" finalizer-cycles
  echo "rust-gc-metadata: $profile passed 256 finalizer/collection cycles"
}

expect_abort() {
  local mode=$1
  local diagnostic=$2
  "$repo/scripts/assert-child-abort.sh" \
    "rust-gc-metadata: $mode" \
    "$diagnostic" \
    "$probe" "$target_dir/debug/libbirnam_id.dylib" "$mode"
}

probe_profile debug
probe_profile release
"$probe" "$target_dir/debug/libbirnam_id.dylib" finalizer-reentrant-collect
"$probe" "$target_dir/release/libbirnam_id.dylib" finalizer-reentrant-collect
expect_abort provider-overflow "gc: too many root providers"
expect_abort invalid-layout "setLayout kinds is null with nonzero count"
expect_abort retained-mark "GC mark callback escaped its invocation"
expect_abort finalizer-allocation "id allocation attempted during collection"

echo "rust-gc-metadata: finalizer reentrancy was refused and forbidden allocation terminated with SIGABRT"
