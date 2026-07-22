#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
scratch=$(mktemp -d /tmp/birnam-rust-lower-rungs.XXXXXX)
trap 'rm -rf "$scratch"' EXIT

zig_root="$scratch/zig-floor"
rust_root="$scratch/rust-floor"
zig_prefix="$zig_root/prefix"
rust_prefix="$rust_root/prefix"
rust_target="$rust_root/cargo-target"
mkdir -p \
  "$zig_root/cache" "$zig_root/global-cache" "$zig_root/tmp" \
  "$rust_root/cache" "$rust_root/global-cache" "$rust_root/tmp" \
  "$zig_prefix" "$rust_prefix"

default_floor="$repo/zig-out/lib/libbirnam_id.dylib"
default_digest_before=""
if [[ -f "$default_floor" ]]; then
  default_digest_before=$(shasum -a 256 "$default_floor" | awk '{print $1}')
fi

rungs=(
  test-artifact
  test-artifact-preflight
  test-gc-layout-mark
  test-store
  test-store-image-backfill
  test-peg
  test-lang
  test-prelude
  test-collection
)

TMPDIR="$zig_root/tmp" zig build stage1 "${rungs[@]}" \
  --prefix "$zig_prefix" \
  --cache-dir "$zig_root/cache" \
  --global-cache-dir "$zig_root/global-cache"
zig_floor="$zig_prefix/lib/libbirnam_id.dylib"
test -f "$zig_floor"
zig_digest=$(shasum -a 256 "$zig_floor" | awk '{print $1}')
echo "lower-rung-matrix: zig-id-v1 passed 72 tests, dylib $zig_digest"

CARGO_TARGET_DIR="$rust_target" cargo build --workspace --locked --offline
CARGO_TARGET_DIR="$rust_target" cargo build --workspace --locked --offline --release
TMPDIR="$rust_root/tmp" zig build stage1 \
  --prefix "$rust_prefix" \
  --cache-dir "$rust_root/cache" \
  --global-cache-dir "$rust_root/global-cache"

probe_rust_profile() {
  local profile=$1
  local cargo_dylib="$rust_target/$profile/libbirnam_id.dylib"
  local selected_dir="$rust_prefix/lib/$profile"
  local selected_floor="$selected_dir/libbirnam_id.dylib"
  local profile_tmp="$rust_root/tmp/$profile"
  mkdir -p "$selected_dir" "$profile_tmp"
  cp "$cargo_dylib" "$selected_floor"

  TMPDIR="$profile_tmp" zig build "${rungs[@]}" \
    --prefix "$rust_prefix" \
    --cache-dir "$rust_root/cache" \
    --global-cache-dir "$rust_root/global-cache" \
    -Did-conformance-floor="$selected_floor" \
    -Did-conformance-identity=rust-id-v1
  local digest
  digest=$(shasum -a 256 "$selected_floor" | awk '{print $1}')
  echo "lower-rung-matrix: rust-id-v1 $profile passed 72 tests, dylib $digest"
}

probe_rust_profile debug
probe_rust_profile release

test "$zig_prefix" != "$rust_prefix"
test "$zig_root/cache" != "$rust_root/cache"
test -f "$zig_prefix/lib/libbirnam_store.dylib"
test -f "$rust_prefix/lib/libbirnam_store.dylib"
test -f "$zig_prefix/lib/libbirnam_stdlib.dylib"
test -f "$rust_prefix/lib/libbirnam_stdlib.dylib"
test -f "$rust_prefix/lib/debug/libbirnam_id.dylib"
test -f "$rust_prefix/lib/release/libbirnam_id.dylib"

if [[ -n "$default_digest_before" ]]; then
  default_digest_after=$(shasum -a 256 "$default_floor" | awk '{print $1}')
  if [[ "$default_digest_after" != "$default_digest_before" ]]; then
    echo "lower-rung-matrix: isolated runs changed the ordinary installed floor" >&2
    exit 1
  fi
fi

echo "lower-rung-matrix: isolated prefixes, caches, Cargo target, and temp roots preserved"
