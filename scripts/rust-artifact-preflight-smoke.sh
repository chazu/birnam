#!/usr/bin/env bash
# Design 66 Z1.2: run the Rust host's negative artifact-preflight corpus.
set -euo pipefail

cd "$(dirname "$0")/.."
export LC_ALL=C

output=$(mktemp "${TMPDIR:-/tmp}/birnam-rust-artifact-preflight.XXXXXX")
sentinel=${BIRNAM_PREFLIGHT_SENTINEL_DYLIB:-}
owned_sentinel=0
if [[ -z "$sentinel" ]]; then
  sentinel=$(mktemp "${TMPDIR:-/tmp}/birnam-rust-artifact-preflight-sentinel.XXXXXX.dylib")
  owned_sentinel=1
fi
trap 'rm -f "$output"; if [[ "$owned_sentinel" == 1 ]]; then rm -f "$sentinel"; fi' EXIT

if [[ ! -f "$sentinel" ]]; then
  if [[ "$owned_sentinel" == 1 ]]; then
    zig build-lib -dynamic -lc src/id/artifact_preflight_sentinel.zig -femit-bin="$sentinel"
  else
    echo "rust-artifact-preflight: sentinel dylib does not exist: $sentinel" >&2
    exit 1
  fi
fi

profile=${BIRNAM_RUST_ARTIFACT_PROFILE:-debug}
case "$profile" in
  debug) cargo_profile=() ;;
  release) cargo_profile=(--release) ;;
  *) echo "rust-artifact-preflight: BIRNAM_RUST_ARTIFACT_PROFILE must be debug or release" >&2; exit 2 ;;
esac

BIRNAM_PREFLIGHT_SENTINEL_DYLIB="$sentinel" \
  cargo test -p birnam-host --lib artifact_preflight --locked --offline "${cargo_profile[@]}" >"$output" 2>&1
grep -F 'test result: ok. 11 passed; 0 failed' "$output" >/dev/null
printf '%s\n' \
  "rust-artifact-preflight: shared eight-case dependency, identity, format, path, native, and real-sentinel corpus passed ($profile)"
