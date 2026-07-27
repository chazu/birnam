#!/usr/bin/env bash
# Design 66 Z1.2: diff normalized Rust/Zig results for the shared preflight
# corpus. The rows are emitted by each implementation from its observed plan
# or typed failure, not copied from the other runner's output.
set -euo pipefail

cd "$(dirname "$0")/.."
export LC_ALL=C

scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-artifact-preflight-parity.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
sentinel="$scratch/sentinel.dylib"
zig_rows="$scratch/zig.rows"
rust_rows="$scratch/rust.rows"
: >"$zig_rows"
: >"$rust_rows"

zig build-lib -dynamic -lc src/id/artifact_preflight_sentinel.zig -femit-bin="$sentinel"
BIRNAM_PREFLIGHT_SENTINEL_DYLIB="$sentinel" \
  BIRNAM_PREFLIGHT_PARITY_OUTPUT="$zig_rows" \
  zig build test-artifact-preflight \
    -Dfloor=rust -Dartifact-inspector-impl=rust -Doptimize=ReleaseSafe -j1 \
    --summary all >"$scratch/zig.out" 2>&1
BIRNAM_PREFLIGHT_SENTINEL_DYLIB="$sentinel" \
  BIRNAM_PREFLIGHT_PARITY_OUTPUT="$rust_rows" \
  cargo test -p birnam-host --lib artifact_preflight_shared_fixture_matrix_matches_zig_cases \
    --locked --offline >"$scratch/rust.out" 2>&1
grep -F 'test result: ok. 1 passed; 0 failed' "$scratch/rust.out" >/dev/null

cmp "$zig_rows" "$rust_rows"
printf '%s\n' \
  'artifact-preflight-parity: eight normalized case/outcome/detail/first-failure rows match across Zig and Rust'
