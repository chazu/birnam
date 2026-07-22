#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
target_dir="$repo/.cargo-target"
scratch=$(mktemp -d /tmp/birnam-rust-id-spike.XXXXXX)
trap 'rm -rf "$scratch"' EXIT
probe="$scratch/rust-id-spike-probe"

zig build-exe \
  --dep idapi \
  -Mroot="$repo/scripts/rust-id-spike-probe.zig" \
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
  test -f "$dylib"
  "$probe" "$dylib" functional
}

probe_profile debug
probe_profile release
