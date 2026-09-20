#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-id -p birnam-namespace -p birnam-object-model -p birnam-method-observation -p birnam-root-registry -p birnam-image -p birnam-alien -p birnam-portable-code -p birnam-codegen-runtime --offline --locked
names_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-namespace-names-native.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v15 -Iscripts/fixtures scripts/fixtures/namespace-names.c -o "$scratch/names"
"$scratch/names" "$names_target/debug/libbirnam_id.dylib" "$names_target/debug/libbirnam_image.dylib" "$names_target/debug/libbirnam_alien.dylib" "$names_target/debug/libbirnam_portable_code.dylib" "$names_target/debug/libbirnam_codegen_runtime.dylib"
