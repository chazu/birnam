#!/usr/bin/env bash
# Focused debug production coordinator and independent native contract probe.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-namespace -p birnam-object-model -p birnam-method-observation -p birnam-root-registry -p birnam-id -p birnam-string -p birnam-trace -p birnam-collect --locked --offline
collect_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-collection.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror scripts/fixtures/collection.c -o "$scratch/collection"
"$scratch/collection" "$collect_target/debug/libbirnam_id.dylib" "$collect_target/debug/libbirnam_trace.dylib" "$collect_target/debug/libbirnam_collect.dylib"
