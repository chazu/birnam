#!/usr/bin/env bash
# Focused debug check of the independently loaded String methods.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-id -p birnam-trace -p birnam-collect -p birnam-integer -p birnam-float -p birnam-word -p birnam-object-model -p birnam-string -p birnam-range -p birnam-collection --locked --offline
string_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-string.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v7 -Iscripts/fixtures scripts/fixtures/string-module.c -o "$scratch/string"
"$scratch/string" "$string_target/debug/libbirnam_id.dylib" "$string_target/debug/libbirnam_object_model.dylib" \
  "$string_target/debug/libbirnam_string.dylib" "$string_target/debug/libbirnam_collection.dylib"
