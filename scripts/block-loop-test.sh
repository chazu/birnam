#!/usr/bin/env bash
# Focused debug native Block loops, dispatch, dependency and callback checks.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-id -p birnam-string -p birnam-trace -p birnam-collect -p birnam-integer -p birnam-float -p birnam-word -p birnam-object-model -p birnam-printing -p birnam-system-gc -p birnam-text-view -p birnam-system-output -p birnam-system-input -p birnam-environment -p birnam-file-io -p birnam-block-loop -p birnam-error-value -p birnam-prelude --locked --offline
block_loop_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-block-loop.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v7 -Iscripts/fixtures scripts/fixtures/block-loop.c -o "$scratch/block-loop"
for mode in normal first-while-intern-reentry first-while-binding-reentry; do
  "$scratch/block-loop" "$block_loop_target/debug/libbirnam_id.dylib" "$block_loop_target/debug/libbirnam_object_model.dylib" "$block_loop_target/debug/libbirnam_prelude.dylib" "$mode" >"$scratch/$mode.log"
  tail -1 "$scratch/$mode.log"
done
