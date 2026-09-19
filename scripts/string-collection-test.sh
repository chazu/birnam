#!/usr/bin/env bash
# Focused debug String split byte oracle, callback timing and installation reentry.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-id -p birnam-trace -p birnam-collect -p birnam-integer -p birnam-float -p birnam-word -p birnam-error-value -p birnam-object-model -p birnam-string -p birnam-range -p birnam-vector -p birnam-collection-value -p birnam-map -p birnam-string-collection -p birnam-collection --locked --offline
adapter_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-string-collection.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v7 -Iscripts/fixtures scripts/fixtures/string-collection.c -o "$scratch/adapter"
for mode in normal first-install-reentry; do
  "$scratch/adapter" "$adapter_target/debug/libbirnam_id.dylib" "$adapter_target/debug/libbirnam_object_model.dylib" "$adapter_target/debug/libbirnam_collection.dylib" "$mode" >"$scratch/$mode.log"
  tail -1 "$scratch/$mode.log"
done
