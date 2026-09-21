#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
CARGO_PROFILE_DEV_PANIC=unwind CARGO_TARGET_DIR=.cargo-target/83-owner-tests mise exec -- cargo test --offline --locked -p birnam-storage --lib
CARGO_PROFILE_DEV_PANIC=unwind CARGO_TARGET_DIR=.cargo-target/83-owner-tests mise exec -- cargo test --offline --locked -p birnam-abi --lib deferred_
CARGO_PROFILE_DEV_PANIC=unwind CARGO_TARGET_DIR=.cargo-target/83-owner-tests mise exec -- cargo test --offline --locked -p birnam-runtime --lib heap::
CARGO_PROFILE_DEV_PANIC=unwind CARGO_TARGET_DIR=.cargo-target/83-owner-tests mise exec -- cargo test --offline --locked -p birnam-runtime --lib runtime::native_tier
mise exec -- cargo build --offline --locked --workspace
cache_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
cache_lib=${1:+$1/lib}
cache_lib=${cache_lib:-$cache_target/debug}
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-deferred-storage.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
mise exec -- rustc --edition=2024 -Cpanic=abort scripts/fixtures/runtime-support-host.rs --extern "birnam_host=$cache_target/debug/libbirnam_host.rlib" --extern "birnam_abi=$cache_target/debug/libbirnam_abi.rlib" -L "dependency=$cache_target/debug/deps" -o "$scratch/host-probe"
cc -std=c11 -Wall -Wextra -Werror -dynamiclib scripts/fixtures/runtime-support-native.c -o "$scratch/runtime-support.dylib"
python3 scripts/fixtures/runtime-support-host.py "$scratch/host-probe" "$cache_lib" "$scratch/runtime-support.dylib"
python3 - "$cache_lib" <<'PY'
from pathlib import Path
import json,subprocess,sys
proof=json.loads(Path('abi/rust-storage-dylib-v1.json').read_text())
image=Path(sys.argv[1])/Path(proof['install_name']).name
actual={name.removeprefix('_') for name in subprocess.check_output(['nm','-gjU',str(image)],text=True).splitlines()}
assert actual==set(proof['exported_symbols'])
assert subprocess.check_output(['otool','-D',str(image)],text=True).splitlines()[1]==proof['install_name']
print('Storage exports and install name match their manifest')
PY
