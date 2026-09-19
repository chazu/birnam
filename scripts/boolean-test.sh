#!/usr/bin/env bash
# Focused debug check of the independently loaded Boolean methods.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-id -p birnam-string -p birnam-trace -p birnam-collect -p birnam-integer -p birnam-float -p birnam-word -p birnam-object-model -p birnam-boolean -p birnam-printing -p birnam-system-gc -p birnam-text-view -p birnam-system-output -p birnam-error-value -p birnam-prelude --locked --offline
boolean_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-boolean.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v7 -Iscripts/fixtures scripts/fixtures/boolean-module.c -o "$scratch/boolean"
"$scratch/boolean" "$boolean_target/debug/libbirnam_id.dylib" "$boolean_target/debug/libbirnam_object_model.dylib" \
  "$boolean_target/debug/libbirnam_boolean.dylib" "$boolean_target/debug/libbirnam_prelude.dylib"
