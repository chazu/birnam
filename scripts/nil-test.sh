#!/usr/bin/env bash
# Focused debug check of the independently loaded Object/nil methods.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-id -p birnam-string -p birnam-trace -p birnam-collect -p birnam-integer -p birnam-float -p birnam-word -p birnam-object-model -p birnam-nil -p birnam-collection --locked --offline
nil_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-nil.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v7 -Iscripts/fixtures scripts/fixtures/nil-module.c -o "$scratch/nil"
"$scratch/nil" "$nil_target/debug/libbirnam_id.dylib" "$nil_target/debug/libbirnam_object_model.dylib" \
  "$nil_target/debug/libbirnam_nil.dylib" "$nil_target/debug/libbirnam_collection.dylib"
