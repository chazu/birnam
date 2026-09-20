#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo test --offline --locked -p birnam-object-model --lib
mise exec -- cargo test --offline --locked -p birnam-abi --lib mutation_epoch
mise exec -- cargo test --offline --locked -p birnam-id --lib mutation_epoch_tests
mise exec -- cargo test --offline --locked -p birnam-id --lib vtable::tests
mise exec -- cargo test --offline --locked -p birnam-id --lib runtime::guard
mise exec -- cargo test --offline --locked -p birnam-id --lib runtime::dispatch_cache
mise exec -- cargo test --offline --locked -p birnam-id --lib runtime::native_tier
mise exec -- cargo build --offline --locked --workspace
cache_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
cache_lib=${1:+$1/lib}
cache_lib=${cache_lib:-$cache_target/debug}
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-mutation-epoch.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v20 scripts/fixtures/mutation-epoch.c -o "$scratch/epoch"
"$scratch/epoch" "$cache_lib/libbirnam_id.dylib"
mise exec -- rustc --edition=2024 -Cpanic=abort scripts/fixtures/mutation-epoch-host.rs --extern "birnam_host=$cache_target/debug/libbirnam_host.rlib" -L "dependency=$cache_target/debug/deps" -o "$scratch/host-probe"
python3 scripts/fixtures/mutation-epoch-host.py "$scratch/host-probe" "$cache_lib"
python3 - "$cache_lib" <<'PY'
from pathlib import Path
import json,subprocess,sys
proof=json.loads(Path('abi/rust-object-model-dylib-v1.json').read_text())
image=Path(sys.argv[1])/proof['library_name']
actual={name.removeprefix('_') for name in subprocess.check_output(['nm','-gjU',str(image)],text=True).splitlines()}
assert actual==set(proof['exported_symbols'])
identity=subprocess.check_output(['otool','-D',str(image)],text=True).splitlines()[1]
assert proof['install_name_policy']=='cargo-output-path' and Path(identity).is_absolute() and Path(identity).name==proof['library_name']
print('Object Model exports match; its existing Cargo output-path install-name policy is preserved')
PY
