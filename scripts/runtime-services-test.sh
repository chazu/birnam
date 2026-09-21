#!/usr/bin/env bash
# Existing independent C service oracles, entered through production Id.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:-}
group=${2:-all}
CARGO_PROFILE_DEV_PANIC=unwind CARGO_TARGET_DIR=.cargo-target/runtime-services-tests mise exec -- cargo test --offline --locked -p birnam-root-registry -p birnam-gc-settings -p birnam-native-stack -p birnam-namespace -p birnam-object-model --lib
if [[ -n $prefix ]]; then
  mise exec -- cargo build --offline --locked -p birnam-host -p birnam-abi
else
  mise exec -- cargo build --offline --locked --workspace
fi
services_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
lib=${prefix:+$prefix/lib}
lib=${lib:-$services_target/debug}
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-runtime-services.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
mise exec -- rustc --edition=2024 -Cpanic=abort scripts/fixtures/runtime-services-host.rs \
  --extern "birnam_host=$services_target/debug/libbirnam_host.rlib" \
  --extern "birnam_abi=$services_target/debug/libbirnam_abi.rlib" \
  -L "dependency=$services_target/debug/deps" -o "$scratch/host"
for fixture in root-registry gc-settings native-stack native-stack-scan namespace-boundary vtable-enumeration vtable-enumeration-boundary selector-enumeration; do
  cc -std=c11 -Wall -Wextra -Werror -dynamiclib -Iabi/fixtures/idapi-v20 -Iscripts/fixtures \
    "scripts/fixtures/$fixture.c" -o "$scratch/$fixture.dylib"
done
python3 scripts/fixtures/runtime-services-host.py "$scratch/host" "$lib" "$scratch" "$group"
