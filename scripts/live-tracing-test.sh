#!/usr/bin/env bash
# Focused debug GC boundary checks; no installation or release validation.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-namespace -p birnam-id -p birnam-string -p birnam-trace -p birnam-collect -p birnam-integer -p birnam-float -p birnam-word -p birnam-error-value -p birnam-object-model -p birnam-method-observation -p birnam-root-registry -p birnam-gc-settings -p birnam-payload-layout -p birnam-trace-hooks --locked --offline
trace_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-live-tracing.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror scripts/fixtures/live-tracing.c -o "$scratch/live"
"$scratch/live" "$trace_target/debug/libbirnam_id.dylib" "$trace_target/debug/libbirnam_trace.dylib"
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v17 -Iscripts/fixtures abi/fixtures/idapi-v17/client.c -o "$scratch/client"
cc -std=c11 -Wall -Wextra -Werror -fvisibility=hidden -dynamiclib abi/fixtures/idapi-v17/module.c -o "$scratch/module.dylib"
"$scratch/client" "$trace_target/debug/libbirnam_id.dylib" "$scratch/module.dylib"
