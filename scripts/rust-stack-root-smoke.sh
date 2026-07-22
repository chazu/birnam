#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
target_dir="$repo/.cargo-target"
scratch=$(mktemp -d /tmp/birnam-rust-stack-root.XXXXXX)
trap 'rm -rf "$scratch"' EXIT

cc -O2 -Wall -Wextra -Werror \
  -c "$repo/scripts/rust-stack-root-hold.c" \
  -o "$scratch/rust-stack-root-hold.o"
zig build-exe -O ReleaseSafe \
  --dep idapi \
  -Mroot="$repo/scripts/rust-stack-root-probe.zig" \
  -Midapi="$repo/src/id/idapi.zig" \
  "$scratch/rust-stack-root-hold.o" \
  -femit-bin="$scratch/rust-stack-root-probe"

probe_profile() {
  local profile=$1
  local cargo_args=(build --workspace --locked --offline)
  if [[ $profile == release ]]; then
    cargo_args+=(--release)
  fi
  CARGO_TARGET_DIR="$target_dir" cargo "${cargo_args[@]}"
  "$scratch/rust-stack-root-probe" "$target_dir/$profile/libbirnam_id.dylib"
}

probe_profile debug
probe_profile release
