#!/usr/bin/env bash
# Design 66 Z1.2: run the Rust host's negative artifact-preflight corpus.
set -euo pipefail

cd "$(dirname "$0")/.."
export LC_ALL=C

output=$(mktemp "${TMPDIR:-/tmp}/birnam-rust-artifact-preflight.XXXXXX")
sentinel=$(mktemp "${TMPDIR:-/tmp}/birnam-rust-artifact-preflight-sentinel.XXXXXX.dylib")
trap 'rm -f "$output" "$sentinel"' EXIT

zig build-lib -dynamic -lc src/id/artifact_preflight_sentinel.zig -femit-bin="$sentinel"
BIRNAM_PREFLIGHT_SENTINEL_DYLIB="$sentinel" \
  cargo test -p birnam-host --lib artifact_preflight --locked --offline >"$output" 2>&1
grep -F 'test result: ok. 11 passed; 0 failed' "$output" >/dev/null
printf '%s\n' \
  'rust-artifact-preflight: shared seven-case dependency, identity, format, path, native, and real-sentinel corpus passed'
