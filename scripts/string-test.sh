#!/usr/bin/env bash
# Focused debug check of the independently loaded String methods.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-namespace -p birnam-id -p birnam-trace -p birnam-collect -p birnam-integer -p birnam-float -p birnam-word -p birnam-error-value -p birnam-object-model -p birnam-method-observation -p birnam-root-registry -p birnam-gc-settings -p birnam-payload-layout -p birnam-trace-hooks -p birnam-finalizers -p birnam-native-stack -p birnam-native-mark -p birnam-allocation-budget -p birnam-string -p birnam-range -p birnam-vector -p birnam-collection-value -p birnam-map -p birnam-string-collection -p birnam-collection --locked --offline
string_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-string.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v20 -Iscripts/fixtures scripts/fixtures/string-module.c -o "$scratch/string"
"$scratch/string" "$string_target/debug/libbirnam_id.dylib" "$string_target/debug/libbirnam_object_model.dylib" \
  "$string_target/debug/libbirnam_string.dylib" "$string_target/debug/libbirnam_collection.dylib"
