#!/usr/bin/env bash
# Focused debug System native loading, dependency and callback checks.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-namespace -p birnam-id -p birnam-string -p birnam-trace -p birnam-collect -p birnam-integer -p birnam-float -p birnam-word -p birnam-object-model -p birnam-method-observation -p birnam-root-registry -p birnam-gc-settings -p birnam-payload-layout -p birnam-trace-hooks -p birnam-printing -p birnam-system-gc -p birnam-text-view -p birnam-system-output -p birnam-system-input -p birnam-environment -p birnam-file-io -p birnam-block-loop -p birnam-native-loader -p birnam-source-loader -p birnam-error-value -p birnam-prelude --locked --offline
native_loader_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-native-loader.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v17 -Iscripts/fixtures scripts/fixtures/native-loader.c -o "$scratch/native-loader"
for mode in normal first-loadmodule-intern-reentry first-print-binding-reentry; do
  "$scratch/native-loader" "$native_loader_target/debug/libbirnam_id.dylib" "$native_loader_target/debug/libbirnam_object_model.dylib" "$native_loader_target/debug/libbirnam_prelude.dylib" "$mode" >"$scratch/$mode.log"
  tail -1 "$scratch/$mode.log"
done
