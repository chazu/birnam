#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
target_dir="$repo/.cargo-target"
scratch=$(mktemp -d /tmp/birnam-rust-env-loader.XXXXXX)
trap 'rm -rf "$scratch"' EXIT
fixture="$scratch/libbirnam_reload_fixture.dylib"
probe="$scratch/rust-env-loader-probe"

zig build-lib -dynamic "$repo/scripts/rust-loader-fixture.zig" -femit-bin="$fixture"
zig build-exe \
  --dep idapi \
  -Mroot="$repo/scripts/rust-env-loader-probe.zig" \
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
  "$probe" "$dylib" "$fixture"
}

probe_profile debug
probe_profile release
