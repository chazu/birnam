#!/usr/bin/env bash
# Focused debug Range method extraction and independent native oracle.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-id -p birnam-trace -p birnam-collect -p birnam-integer -p birnam-float -p birnam-word -p birnam-object-model -p birnam-string -p birnam-range -p birnam-vector -p birnam-collection-value -p birnam-map -p birnam-collection --locked --offline
range_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-range.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v7 -Iscripts/fixtures scripts/fixtures/range-module.c -o "$scratch/range"
"$scratch/range" "$range_target/debug/libbirnam_id.dylib" "$range_target/debug/libbirnam_object_model.dylib" "$range_target/debug/libbirnam_collection.dylib"
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v7 -Iscripts/fixtures scripts/fixtures/range-storage.c -o "$scratch/storage"
for mode in normal adopted missing-prototype rebound-prototype range-only-exports; do
  "$scratch/storage" "$range_target/debug/libbirnam_id.dylib" "$range_target/debug/libbirnam_object_model.dylib" "$range_target/debug/libbirnam_collection.dylib" "$mode" >"$scratch/$mode.log"
  tail -1 "$scratch/$mode.log"
done
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v7 -Iscripts/fixtures scripts/fixtures/range-vector.c -o "$scratch/vector"
"$scratch/vector" "$range_target/debug/libbirnam_id.dylib" "$range_target/debug/libbirnam_object_model.dylib" "$range_target/debug/libbirnam_collection.dylib" >"$scratch/vector.log"
tail -1 "$scratch/vector.log"
