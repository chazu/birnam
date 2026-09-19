#!/usr/bin/env bash
# Focused debug borrowed-byte and Prelude-consumer oracle.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-id -p birnam-string -p birnam-trace -p birnam-collect -p birnam-integer -p birnam-float -p birnam-word -p birnam-object-model -p birnam-printing -p birnam-system-gc -p birnam-text-view -p birnam-system-output -p birnam-system-input -p birnam-environment -p birnam-file-io -p birnam-error-value -p birnam-prelude --locked --offline
text_view_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-text-view.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v7 -Iscripts/fixtures scripts/fixtures/text-view.c -o "$scratch/text-view"
"$scratch/text-view" "$text_view_target/debug/libbirnam_id.dylib" "$text_view_target/debug/libbirnam_object_model.dylib" "$text_view_target/debug/libbirnam_prelude.dylib" >"$scratch/oracle.log"
tail -1 "$scratch/oracle.log"
