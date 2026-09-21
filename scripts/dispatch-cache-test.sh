#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo test --offline --locked -p birnam-dispatch-cache --lib
mise exec -- cargo test --offline --locked -p birnam-abi --lib dispatch_cache
mise exec -- cargo test --offline --locked -p birnam-runtime --lib runtime::dispatch_cache
mise exec -- cargo test --offline --locked -p birnam-runtime --lib runtime::guarded_lookup
mise exec -- cargo test --offline --locked -p birnam-runtime --lib runtime::collection_phase_tests
mise exec -- cargo build --offline --locked --workspace
cache_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
cache_lib=${1:+$1/lib}
cache_lib=${cache_lib:-$cache_target/debug}
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-dispatch-cache.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v20 scripts/fixtures/dispatch-cache.c -o "$scratch/cache"
"$scratch/cache" "$cache_lib/libbirnam_id.dylib" "$cache_lib/libbirnam_trace.dylib"
mise exec -- rustc --edition=2024 -Cpanic=abort scripts/fixtures/runtime-support-host.rs --extern "birnam_host=$cache_target/debug/libbirnam_host.rlib" --extern "birnam_abi=$cache_target/debug/libbirnam_abi.rlib" -L "dependency=$cache_target/debug/deps" -o "$scratch/host-probe"
cc -std=c11 -Wall -Wextra -Werror -dynamiclib scripts/fixtures/runtime-support-native.c -o "$scratch/runtime-support.dylib"
python3 scripts/fixtures/runtime-support-host.py "$scratch/host-probe" "$cache_lib" "$scratch/runtime-support.dylib"
