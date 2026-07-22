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

output="$scratch/unsupported.log"
set +e
"$probe" "$target_dir/debug/libbirnam_id.dylib" unsupported >"$output" 2>&1
status=$?
set -e
if [[ $status -ne 134 ]]; then
  echo "rust-id-spike: unsupported field exited $status, expected SIGABRT status 134" >&2
  cat "$output" >&2
  exit 1
fi
if ! grep -q "Rust R2.6 IdApi field is unsupported: setMethodOverwriteHook" "$output"; then
  echo "rust-id-spike: unsupported field omitted its exact diagnostic" >&2
  cat "$output" >&2
  exit 1
fi

echo "rust-id-spike: unsupported setMethodOverwriteHook terminated with SIGABRT"
