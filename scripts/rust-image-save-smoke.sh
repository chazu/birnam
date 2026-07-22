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
  local dylib="$target_dir/$profile/libbirnam_id.dylib"
  local digest
  digest=$(shasum -a 256 "$dylib" | awk '{print $1}')
  BIRNAM_BIN="$bin" \
    BIRNAM_LIB_DIR="$libdir" \
    BIRNAM_ID_DYLIB="$dylib" \
    BIRNAM_ID_FLOOR_IDENTITY=rust-id-v1 \
    BIRNAM_ID_FLOOR_SHA256="$digest" \
    bash "$repo/scripts/image-save-smoke.sh"
  echo "rust-image-save: $profile selected $digest"
}

probe_profile debug
probe_profile release
