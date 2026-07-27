#!/usr/bin/env bash
# Design 66 Z1.1: exercise the Rust and Zig executable-relative resource
# contracts independently, using the same three frozen path/override cases.
#
# This is an observation-boundary test. It does not select a provider or alter
# runtime policy; each implementation's unit tests assert the exact resolved
# paths and the narrow override semantics.
set -euo pipefail

cd "$(dirname "$0")/.."
export LC_ALL=C

zig_output=$(mktemp "${TMPDIR:-/tmp}/birnam-resource-zig.XXXXXX")
rust_output=$(mktemp "${TMPDIR:-/tmp}/birnam-resource-rust.XXXXXX")
sentinel=$(mktemp "${TMPDIR:-/tmp}/birnam-resource-sentinel.XXXXXX.dylib")
trap 'rm -f "$zig_output" "$rust_output" "$sentinel"' EXIT

zig test src/id/runtime_resources.zig -OReleaseSafe >"$zig_output" 2>&1
grep -F '1/3 runtime_resources.test.runtime resources use one normalized executable-relative layout...OK' "$zig_output" >/dev/null
grep -F '2/3 runtime_resources.test.runtime resource overrides remain narrow and exact...OK' "$zig_output" >/dev/null
grep -F '3/3 runtime_resources.test.a lib override supplies the default id dylib...OK' "$zig_output" >/dev/null
grep -F 'All 3 tests passed.' "$zig_output" >/dev/null

zig build-lib -dynamic -lc src/id/artifact_preflight_sentinel.zig -femit-bin="$sentinel"
BIRNAM_PREFLIGHT_SENTINEL_DYLIB="$sentinel" \
  cargo test -p birnam-host --lib --offline >"$rust_output"
for test_name in \
  uses_one_normalized_executable_relative_layout \
  overrides_remain_narrow_and_exact \
  a_lib_override_supplies_the_default_id_dylib
do
  grep -F "test tests::$test_name ... ok" "$rust_output" >/dev/null
done

printf '%s\n' \
  'rust-resource-resolution: Zig and Rust normalized-layout cases passed' \
  'rust-resource-resolution: narrow override cases passed' \
  'rust-resource-resolution: default id dylib follows lib override in both floors'
