#!/usr/bin/env bash
# Focused debug check of the tracer over actual production storage; no install.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-id -p birnam-string -p birnam-storage -p birnam-integer -p birnam-float -p birnam-word -p birnam-trace --locked --offline
trace_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-storage-tracing.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror scripts/fixtures/storage-tracing.c -o "$scratch/probe"
"$scratch/probe" "$trace_target/debug/libbirnam_id.dylib" \
  "$trace_target/debug/libbirnam_storage.dylib" "$trace_target/debug/libbirnam_integer.dylib" \
  "$trace_target/debug/libbirnam_trace.dylib"
