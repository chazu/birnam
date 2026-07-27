#!/usr/bin/env bash
# Design 66 Z1.2: run the Rust host's negative artifact-preflight corpus.
set -euo pipefail

cd "$(dirname "$0")/.."
export LC_ALL=C

output=$(mktemp "${TMPDIR:-/tmp}/birnam-rust-artifact-preflight.XXXXXX")
trap 'rm -f "$output"' EXIT

cargo test -p birnam-host --lib artifact_preflight --offline >"$output" 2>&1
grep -F 'test result: ok. 3 passed; 0 failed' "$output" >/dev/null
printf '%s\n' \
  'rust-artifact-preflight: dependency order, identity/missing sibling, and wrong-kind rejection passed'
