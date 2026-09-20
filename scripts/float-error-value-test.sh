#!/usr/bin/env bash
# Focused Float Error recognition/construction and activation checks.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-namespace -p birnam-id -p birnam-string -p birnam-trace -p birnam-collect -p birnam-integer -p birnam-float -p birnam-word -p birnam-object-model -p birnam-method-observation -p birnam-root-registry -p birnam-gc-settings -p birnam-payload-layout -p birnam-trace-hooks -p birnam-error-value --locked --offline
float_error_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-float-error.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v18 -Iscripts/fixtures scripts/fixtures/float-error-value.c -o "$scratch/float-error"
"$scratch/float-error" "$float_error_target/debug/libbirnam_id.dylib" "$float_error_target/debug/libbirnam_object_model.dylib" "$float_error_target/debug/libbirnam_float.dylib" >"$scratch/oracle.log"
tail -1 "$scratch/oracle.log"
