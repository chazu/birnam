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

run_case() {
  local name=$1
  local source=$2
  local expected=$3
  local probe="$scratch/$name.birnam"
  printf '%s\n' "$source" >"$probe"
  local zig_output rust_output zig_rc rust_rc
  if zig_output=$(run_with_floor "$zig_floor" zig-id-v1 "$zig_digest" "$probe"); then
    zig_rc=0
  else
    zig_rc=$?
  fi
  if rust_output=$(run_with_floor "$rust_floor" rust-id-v1 "$rust_digest" "$probe"); then
    rust_rc=0
  else
    rust_rc=$?
  fi
  # Error-valued expressions may make the host reject the enclosing script
  # after printing the diagnostic values.  The values are the contract; a
  # nonzero script status is accepted only when stdout is exactly that case's
  # expected kind/message pair.
  if [[ $zig_output != "$expected" || $rust_output != "$expected" ||
    ( $zig_rc -ne 0 && $zig_rc -ne 1 ) || ( $rust_rc -ne 0 && $rust_rc -ne 1 ) ]]; then
    printf 'cross-floor-errors: %s kind/message mismatch\n--- expected ---\n%s\n--- zig ---\n%s\n--- rust ---\n%s\n' \
      "$name" "$expected" "$zig_output" "$rust_output" >&2
    exit 1
  fi
}

# Keep each error-valued expression in its own script.  The host rejects a
# script whose final evaluation is an error value; the language-level error
# contract is still exercised by inspecting kind/message before the script
# completes successfully.
run_case \
  dnu \
  $'let dnu := [nil frobnicate];\n[System println: [dnu kind]];\n[System println: [dnu message]]' \
  $'#doesNotUnderstand\ndoes not understand #frobnicate'
run_case \
  zero-divide \
  $'let zero := [10 / 0];\n[System println: [zero kind]];\n[System println: [zero message]]' \
  $'#zeroDivide\ndivision by zero'
run_case \
  must-be-boolean \
  $'let boolean := [[| 42] whileTrue: [| nil]];\n[System println: [boolean kind]];\n[System println: [boolean message]]' \
  $'#mustBeBoolean\nwhile loop condition must answer true or false'
run_case \
  not-a-symbol \
  $'let symbol := [Reflect nameOf: "not-a-symbol"];\n[System println: [symbol kind]];\n[System println: [symbol message]]' \
  $'#notASymbol\nnameOf: expects a symbol'

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
