#!/usr/bin/env bash
# Focused Integer error policy, arithmetic oracle and native admission checks.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-namespace -p birnam-id -p birnam-string -p birnam-trace -p birnam-collect -p birnam-integer -p birnam-float -p birnam-word -p birnam-object-model -p birnam-printing -p birnam-system-gc -p birnam-text-view -p birnam-system-output -p birnam-system-input -p birnam-environment -p birnam-file-io -p birnam-block-loop -p birnam-native-loader -p birnam-source-loader -p birnam-error-value -p birnam-prelude -p birnam-error --locked --offline
integer_error_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
lib="$integer_error_target/debug"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-integer-error.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
for fixture in integer-error-value integer-module native-numeric-api-context; do
  cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v12 -Iscripts/fixtures "scripts/fixtures/$fixture.c" -o "$scratch/$fixture"
done
"$scratch/integer-error-value" "$lib/libbirnam_id.dylib" "$lib/libbirnam_object_model.dylib" "$lib/libbirnam_integer.dylib" >"$scratch/oracle.log"
tail -1 "$scratch/oracle.log"
"$scratch/integer-module" "$lib/libbirnam_id.dylib" "$lib/libbirnam_integer.dylib" "$lib/libbirnam_error.dylib" "$lib/libbirnam_object_model.dylib" "$lib/libbirnam_prelude.dylib"
"$scratch/native-numeric-api-context" "$lib/libbirnam_id.dylib" "$lib/libbirnam_integer.dylib"
