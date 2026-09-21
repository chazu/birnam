#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo test --offline --locked -p birnam-storage --lib
mise exec -- cargo test --offline --locked -p birnam-runtime --lib allocation_identity_tests
mise exec -- cargo test --offline --locked -p birnam-runtime --lib heap::tests
mise exec -- cargo test --offline --locked -p birnam-runtime --lib runtime::guard
mise exec -- cargo test --offline --locked -p birnam-runtime --lib runtime::dispatch_cache
mise exec -- cargo test --offline --locked -p birnam-runtime --lib runtime::native_tier
mise exec -- cargo build --offline --locked --workspace
cache_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
cache_lib=${1:+$1/lib}
cache_lib=${cache_lib:-$cache_target/debug}
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-allocation-identity.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v20 scripts/fixtures/allocation-identity.c -o "$scratch/index"
"$scratch/index" "$cache_lib/libbirnam_id.dylib" normal
python3 - "$cache_lib" <<'PY'
from pathlib import Path
import json,subprocess,sys
proof=json.loads(Path('abi/rust-storage-dylib-v1.json').read_text())
image=Path(sys.argv[1])/Path(proof['install_name']).name
actual={name.removeprefix('_') for name in subprocess.check_output(['nm','-gjU',str(image)],text=True).splitlines()}
assert actual==set(proof['exported_symbols'])
identity=subprocess.check_output(['otool','-D',str(image)],text=True).splitlines()[1]
assert identity==proof['install_name']
print('Storage exports and install name match their manifest')
PY
