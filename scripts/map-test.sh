#!/usr/bin/env bash
# Focused debug Map storage, methods and independent HAMT/callback oracles.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-namespace -p birnam-id -p birnam-trace -p birnam-collect -p birnam-integer -p birnam-float -p birnam-word -p birnam-error-value -p birnam-object-model -p birnam-string -p birnam-range -p birnam-vector -p birnam-collection-value -p birnam-map -p birnam-string-collection -p birnam-collection --locked --offline
map_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-map.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v12 -Iscripts/fixtures scripts/fixtures/map-storage.c -o "$scratch/storage"
for mode in normal adopted missing-prototype rebound-prototype map-only-exports; do
  "$scratch/storage" "$map_target/debug/libbirnam_id.dylib" "$map_target/debug/libbirnam_object_model.dylib" "$map_target/debug/libbirnam_collection.dylib" "$mode" >"$scratch/$mode.log"
  tail -1 "$scratch/$mode.log"
done
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v12 -Iscripts/fixtures scripts/fixtures/map-hamt.c -o "$scratch/hamt"
"$scratch/hamt" "$map_target/debug/libbirnam_id.dylib" "$map_target/debug/libbirnam_object_model.dylib" "$map_target/debug/libbirnam_collection.dylib" >"$scratch/hamt.log"
tail -1 "$scratch/hamt.log"
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v12 -Iscripts/fixtures scripts/fixtures/map-methods.c -o "$scratch/methods"
"$scratch/methods" "$map_target/debug/libbirnam_id.dylib" "$map_target/debug/libbirnam_object_model.dylib" "$map_target/debug/libbirnam_collection.dylib" >"$scratch/methods.log"
tail -1 "$scratch/methods.log"
