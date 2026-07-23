#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
target_dir="$repo/.cargo-target"
bin=${BIRNAM_BIN:-$repo/zig-out/bin/birnam}
libdir=${BIRNAM_LIB_DIR:-$repo/zig-out/lib}

probe_profile() {
  local profile=$1
  local cargo_args=(build --workspace --locked --offline)
  if [[ $profile == release ]]; then
    cargo_args+=(--release)
  fi

  CARGO_TARGET_DIR="$target_dir" cargo "${cargo_args[@]}"
  local dylib="$target_dir/$profile/libbirnam_store.dylib"
  test -f "$dylib"

  BIRNAM_BIN="$bin" \
    BIRNAM_LIB_DIR="$libdir" \
    BIRNAM_STORE_DYLIB="$dylib" \
    bash "$repo/scripts/image-save-smoke.sh"
  BIRNAM_BIN="$bin" \
    BIRNAM_LIB_DIR="$libdir" \
    BIRNAM_STORE_DYLIB="$dylib" \
    bash "$repo/scripts/repl-golden.sh"
  echo "rust-store-stack: $profile passed image-save and REPL integration"
}

probe_profile debug
probe_profile release
