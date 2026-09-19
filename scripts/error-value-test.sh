#!/usr/bin/env bash
# Focused debug lazy Error value and Prelude-consumer oracle.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-namespace -p birnam-id -p birnam-string -p birnam-trace -p birnam-collect -p birnam-integer -p birnam-float -p birnam-word -p birnam-object-model -p birnam-printing -p birnam-system-gc -p birnam-text-view -p birnam-system-output -p birnam-system-input -p birnam-environment -p birnam-file-io -p birnam-block-loop -p birnam-native-loader -p birnam-source-loader -p birnam-error-value -p birnam-prelude --locked --offline
error_value_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-error-value.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v8 -Iscripts/fixtures scripts/fixtures/error-value.c -o "$scratch/error-value"
"$scratch/error-value" "$error_value_target/debug/libbirnam_id.dylib" "$error_value_target/debug/libbirnam_object_model.dylib" "$error_value_target/debug/libbirnam_prelude.dylib" >"$scratch/oracle.log"
tail -1 "$scratch/oracle.log"

cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v8 -Iscripts/fixtures scripts/fixtures/error-value-presence-admission.c -o "$scratch/presence-admission"
lib="$error_value_target/debug"
"$scratch/presence-admission" "$lib/libbirnam_id.dylib" "$lib/libbirnam_error_value.dylib" \
  "$lib/libbirnam_integer.dylib" "$lib/libbirnam_float.dylib" "$lib/libbirnam_word.dylib" \
  "$lib/libbirnam_environment.dylib" "$lib/libbirnam_system_input.dylib" "$lib/libbirnam_file_io.dylib" \
  "$lib/libbirnam_source_loader.dylib" "$lib/libbirnam_native_loader.dylib" "$lib/libbirnam_block_loop.dylib"
