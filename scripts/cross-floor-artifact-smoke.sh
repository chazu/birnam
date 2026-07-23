#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
scratch=$(mktemp -d /tmp/birnam-cross-floor-artifact.XXXXXX)
trap 'rm -rf "$scratch"' EXIT

prefix="$scratch/prefix"
zig_cache="$scratch/zig-cache"
zig_global_cache="$scratch/zig-global-cache"
zig_tmp="$scratch/zig-tmp"
cargo_target="$scratch/cargo-target"
mkdir -p "$prefix" "$zig_cache" "$zig_global_cache" "$zig_tmp"

TMPDIR="$zig_tmp" zig build install \
  -Dfloor=zig \
  --prefix "$prefix" \
  --cache-dir "$zig_cache" \
  --global-cache-dir "$zig_global_cache"
CARGO_TARGET_DIR="$cargo_target" \
  cargo build --workspace --locked --offline --release

bin="$prefix/bin/birnam"
zig_floor="$prefix/lib/libbirnam_id.dylib"
rust_floor="$cargo_target/release/libbirnam_id.dylib"
test -x "$bin"
test -f "$zig_floor"
test -f "$rust_floor"

zig_digest=$(shasum -a 256 "$zig_floor" | awk '{print $1}')
rust_digest=$(shasum -a 256 "$rust_floor" | awk '{print $1}')

source="$scratch/cross-floor.birnam"
zig_artifact="$scratch/zig/libbirnam_crossFloor.dylib"
rust_artifact="$scratch/rust/libbirnam_crossFloor.dylib"
mkdir -p "$(dirname "$zig_artifact")" "$(dirname "$rust_artifact")"
printf '%s\n' \
  '[Module named: #crossFloor requires: #[]];' \
  '[System println: "cross-floor-artifact-ok"]' >"$source"

run_with_floor() {
  local floor=$1
  local identity=$2
  local digest=$3
  shift 3
  BIRNAM_LIB_DIR="$prefix/lib" \
    BIRNAM_ID_DYLIB="$floor" \
    BIRNAM_ID_FLOOR_IDENTITY="$identity" \
    BIRNAM_ID_FLOOR_SHA256="$digest" \
    "$bin" "$@"
}

run_with_floor "$zig_floor" zig-id-v1 "$zig_digest" \
  --compile-module "$source" "$zig_artifact"
run_with_floor "$rust_floor" rust-id-v1 "$rust_digest" \
  --compile-module "$source" "$rust_artifact"

for producer in zig rust; do
  artifact_var="${producer}_artifact"
  artifact=${!artifact_var}
  for consumer in zig rust; do
    floor_var="${consumer}_floor"
    digest_var="${consumer}_digest"
    floor=${!floor_var}
    digest=${!digest_var}
    identity="${consumer}-id-v1"
    output=$(run_with_floor "$floor" "$identity" "$digest" "$artifact")
    if [[ $output != cross-floor-artifact-ok ]]; then
      echo "cross-floor-artifact: $producer producer -> $consumer consumer returned: $output" >&2
      exit 1
    fi
    echo "cross-floor-artifact: $producer producer -> $consumer consumer passed"
  done
done

echo "cross-floor-artifact: both same-floor and both cross-floor cells passed"
