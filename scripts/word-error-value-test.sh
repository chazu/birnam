#!/usr/bin/env bash
# Focused Word overflow policy and dependency activation checks.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-namespace -p birnam-id -p birnam-string -p birnam-trace -p birnam-collect -p birnam-integer -p birnam-float -p birnam-word -p birnam-object-model -p birnam-method-observation -p birnam-error-value --locked --offline
word_error_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-word-error.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v14 -Iscripts/fixtures scripts/fixtures/word-error-value.c -o "$scratch/word-error"
"$scratch/word-error" "$word_error_target/debug/libbirnam_id.dylib" "$word_error_target/debug/libbirnam_object_model.dylib" "$word_error_target/debug/libbirnam_word.dylib" >"$scratch/oracle.log"
tail -1 "$scratch/oracle.log"
