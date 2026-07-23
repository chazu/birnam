#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
target_dir="$repo/.cargo-target"
scratch=$(mktemp -d /tmp/birnam-rust-store-mop.XXXXXX)
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/cache" "$scratch/global-cache" "$scratch/prefix"

probe_profile() {
  local profile=$1
  local dylib="$target_dir/$profile/libbirnam_id.dylib"
  local cargo_args=(build --workspace --locked --offline)
  if [[ $profile == release ]]; then
    cargo_args+=(--release)
  fi

  CARGO_TARGET_DIR="$target_dir" cargo "${cargo_args[@]}"
  zig build test-store \
    -Dfloor=zig \
    --prefix "$scratch/prefix" \
    --cache-dir "$scratch/cache" \
    --global-cache-dir "$scratch/global-cache" \
    -Did-conformance-floor="$dylib" \
    -Did-conformance-identity=rust-id-v1
  zig build test-store-image-backfill \
    -Dfloor=zig \
    --prefix "$scratch/prefix" \
    --cache-dir "$scratch/cache" \
    --global-cache-dir "$scratch/global-cache" \
    -Did-conformance-floor="$dylib" \
    -Did-conformance-identity=rust-id-v1
  echo "rust-store-mop: $profile passed the Store consistency-law suite"
}

probe_zig_oracle() {
  local dylib="$repo/zig-out/lib/libbirnam_id_zig.dylib"
  zig build test-store test-store-image-backfill \
    -Dfloor=zig \
    -Dartifact-inspector-impl=rust \
    --prefix "$scratch/prefix" \
    --cache-dir "$scratch/cache" \
    --global-cache-dir "$scratch/global-cache" \
    -Did-conformance-floor="$dylib" \
    -Did-conformance-identity=zig-id-v1
  echo "rust-store-mop: Zig Store + Zig-floor oracle passed the characterization suite"
}

probe_zig_oracle
probe_profile debug
probe_profile release
