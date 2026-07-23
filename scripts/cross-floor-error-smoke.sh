#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
scratch=$(mktemp -d /tmp/birnam-cross-floor-errors.XXXXXX)
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
zig_digest=$(shasum -a 256 "$zig_floor" | awk '{print $1}')
rust_digest=$(shasum -a 256 "$rust_floor" | awk '{print $1}')

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

probe="$scratch/errors.birnam"
printf '%s\n' \
  'let dnu := [nil frobnicate];' \
  '[System println: [dnu kind]];' \
  '[System println: [dnu message]];' \
  'let zero := [10 / 0];' \
  '[System println: [zero kind]];' \
  '[System println: [zero message]];' \
  'let boolean := [[| 42] whileTrue: [| nil]];' \
  '[System println: [boolean kind]];' \
  '[System println: [boolean message]];' \
  'let symbol := [Reflect nameOf: "not-a-symbol"];' \
  '[System println: [symbol kind]];' \
  '[System println: [symbol message]]' >"$probe"

expected=$'#doesNotUnderstand\ndoes not understand #frobnicate\n#zeroDivide\ndivision by zero\n#mustBeBoolean\nwhile loop condition must answer true or false\n#notASymbol\nnameOf: expects a symbol'
zig_output=$(run_with_floor "$zig_floor" zig-id-v1 "$zig_digest" "$probe")
rust_output=$(run_with_floor "$rust_floor" rust-id-v1 "$rust_digest" "$probe")
if [[ $zig_output != "$expected" || $rust_output != "$expected" ]]; then
  printf 'cross-floor-errors: kind/message mismatch\n--- expected ---\n%s\n--- zig ---\n%s\n--- rust ---\n%s\n' \
    "$expected" "$zig_output" "$rust_output" >&2
  exit 1
fi

zig_syntax=$(printf '[1 + ]\n:quit\n' | \
  run_with_floor "$zig_floor" zig-id-v1 "$zig_digest" --repl 2>&1 1>/dev/null)
rust_syntax=$(printf '[1 + ]\n:quit\n' | \
  run_with_floor "$rust_floor" rust-id-v1 "$rust_digest" --repl 2>&1 1>/dev/null)
expected_syntax=$'1:6: syntax error\n[1 + ]\n     ^'
if [[ $zig_syntax != "$expected_syntax" || $rust_syntax != "$expected_syntax" ]]; then
  printf 'cross-floor-errors: syntax diagnostic mismatch\n--- expected ---\n%s\n--- zig ---\n%s\n--- rust ---\n%s\n' \
    "$expected_syntax" "$zig_syntax" "$rust_syntax" >&2
  exit 1
fi

echo "cross-floor-errors: specified kinds, messages, and syntax diagnostics match"
