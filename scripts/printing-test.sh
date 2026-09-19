#!/usr/bin/env bash
# Focused debug byte, dispatch and installation oracle for printing defaults.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-id -p birnam-string -p birnam-trace -p birnam-collect -p birnam-integer -p birnam-float -p birnam-word -p birnam-object-model -p birnam-printing -p birnam-system-gc -p birnam-text-view -p birnam-system-output -p birnam-system-input -p birnam-environment -p birnam-file-io -p birnam-block-loop -p birnam-native-loader -p birnam-source-loader -p birnam-error-value -p birnam-prelude --locked --offline
printing_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-printing.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v7 -Iscripts/fixtures scripts/fixtures/printing.c -o "$scratch/printing"
for mode in normal first-intern-reentry; do
  "$scratch/printing" "$printing_target/debug/libbirnam_id.dylib" "$printing_target/debug/libbirnam_object_model.dylib" "$printing_target/debug/libbirnam_prelude.dylib" "$mode" >"$scratch/$mode.log"
  tail -1 "$scratch/$mode.log"
done
