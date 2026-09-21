#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build --offline --locked -p birnam-id -p birnam-object-model -p birnam-method-observation -p birnam-root-registry -p birnam-gc-settings -p birnam-payload-layout -p birnam-trace-hooks -p birnam-finalizers -p birnam-native-stack -p birnam-native-mark -p birnam-allocation-budget -p birnam-dispatch-cache -p birnam-storage -p birnam-namespace -p birnam-integer -p birnam-float -p birnam-word -p birnam-string -p birnam-trace -p birnam-collect -p birnam-store -p birnam-tcc -p birnam-codegen-runtime
enumeration_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-vtable-enumeration.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
bash scripts/runtime-services-test.sh "${1:-}" vtable-enumeration
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v20 -Iscripts/fixtures scripts/fixtures/vtable-enumeration-consumers.c -o "$scratch/vtable-enumeration-consumers"
"$scratch/vtable-enumeration-consumers" "$enumeration_target/debug/libbirnam_id.dylib" "$enumeration_target/debug/libbirnam_store.dylib" "$enumeration_target/debug/libbirnam_tcc.dylib" "$enumeration_target/debug/libbirnam_codegen_runtime.dylib"
