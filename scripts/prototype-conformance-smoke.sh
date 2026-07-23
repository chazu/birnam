#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
target_dir="$repo/.cargo-target"
scratch=$(mktemp -d /tmp/birnam-prototype-conformance.XXXXXX)
trap 'rm -rf "$scratch"' EXIT
probe="$scratch/prototype-conformance-probe"

zig build-exe \
  --dep idapi \
  -Mroot="$repo/scripts/prototype-conformance-probe.zig" \
  -Midapi="$repo/src/id/idapi.zig" \
  -femit-bin="$probe"

zig_floor=${BIRNAM_ZIG_ID_DYLIB:-}
if [[ -z "$zig_floor" ]]; then
  mkdir -p "$scratch/zig-cache" "$scratch/zig-global-cache"
  zig build bootstrap-install \
    -Dfloor=zig \
    --prefix "$scratch/zig-prefix" \
    --cache-dir "$scratch/zig-cache" \
    --global-cache-dir "$scratch/zig-global-cache"
  zig_floor="$scratch/zig-prefix/lib/libbirnam_id_zig.dylib"
fi
test -f "$zig_floor"
"$probe" "$zig_floor" zig

probe_rust_profile() {
  local profile=$1
  local dylib="$target_dir/$profile/libbirnam_id.dylib"
  local cargo_args=(build --workspace --locked --offline)
  if [[ $profile == release ]]; then
    cargo_args+=(--release)
  fi
  CARGO_TARGET_DIR="$target_dir" cargo "${cargo_args[@]}"
  "$probe" "$dylib" "rust-$profile"
}

probe_rust_profile debug
probe_rust_profile release
