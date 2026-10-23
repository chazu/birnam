#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
export MISE_TRUSTED_CONFIG_PATHS="$root"
export CARGO_TARGET_DIR="${BIRNAM_OOP_TARGET:-$root/.cargo-target/oop-slice}"
export BIRNAM_OOP_TARGET="$CARGO_TARGET_DIR"
fixture="$CARGO_TARGET_DIR/fixtures"
logs="$CARGO_TARGET_DIR/qualification"
mkdir -p "$fixture" "$logs"
mise exec -- cargo test --offline --locked -p birnam-id -p birnam-root-registry -p birnam-namespace -p birnam-object-model -p birnam-word --lib > "$logs/unit.log" 2>&1
mise exec -- cargo build --offline --locked --release --lib -p birnam-id -p birnam-root-registry -p birnam-collect -p birnam-namespace -p birnam-storage -p birnam-boolean -p birnam-nil -p birnam-object-model -p birnam-word > "$logs/build.log" 2>&1
mise exec -- cargo build --offline --locked --release -p birnam-siphon --example oop_slice > "$logs/caller-build.log" 2>&1
python3 - "$CARGO_TARGET_DIR/release" <<'PY_EXPORTS'
from pathlib import Path
import json,subprocess,sys
for name in ['id','root-registry','collect','namespace','storage','boolean','nil','object-model','word']:
    manifest=json.loads(Path('rust/birnam-'+name+'/exports.json').read_text())
    library=Path(sys.argv[1])/('libbirnam_'+name.replace('-','_')+'.dylib')
    actual=set(subprocess.check_output(['nm','-gjU',str(library)],text=True).splitlines())
    assert actual == {'_'+s for s in manifest['exported_symbols']}, (name,actual)
print('Oop module export audit: passed')
PY_EXPORTS
for name in fixture failure rejected rejected-null; do
    clang -dynamiclib -O2 -Itests/fixtures/oop-id-v2 "tests/fixtures/oop-id-v2/$name.c" -o "$fixture/$name.dylib"
done
clang -O2 -DV2 -Itests/fixtures/oop-id-v2 tests/fixtures/oop-id-v2/bench.c -o "$fixture/bench-v2"
clang -O2 -Itests/fixtures/oop-id-v2 tests/fixtures/oop-id-v2/bench.c -o "$fixture/bench-v1"
# Explicit paths supply the entire runtime closure. There is no installed-prefix
# lookup, upper-language library, source compilation or image bootstrap fallback.
BIRNAM_MODULES_DIR=/nonexistent BIRNAM_TEST_MODULE_DIRECTORY=/nonexistent \
    "$CARGO_TARGET_DIR/release/examples/oop_slice" "$root" > "$logs/runtime.log" 2>&1
BIRNAM_MODULES_DIR=/nonexistent BIRNAM_TEST_MODULE_DIRECTORY=/nonexistent \
    "$CARGO_TARGET_DIR/release/examples/oop_slice" "$root" finalizer-failure > "$logs/finalizer-failure.log" 2>&1
BIRNAM_MODULES_DIR=/nonexistent BIRNAM_TEST_MODULE_DIRECTORY=/nonexistent \
    "$CARGO_TARGET_DIR/release/examples/oop_slice" "$root" storage-init-failure > "$logs/storage-init-failure.log" 2>&1
BIRNAM_MODULES_DIR=/nonexistent BIRNAM_TEST_MODULE_DIRECTORY=/nonexistent \
    "$CARGO_TARGET_DIR/release/examples/oop_slice" "$root" retirement-failure > "$logs/retirement-failure.log" 2>&1
BIRNAM_MODULES_DIR=/nonexistent BIRNAM_TEST_MODULE_DIRECTORY=/nonexistent \
    "$CARGO_TARGET_DIR/release/examples/oop_slice" "$root" allocation-policy > "$logs/allocation-policy.log" 2>&1
BIRNAM_MODULES_DIR=/nonexistent BIRNAM_TEST_MODULE_DIRECTORY=/nonexistent \
    "$CARGO_TARGET_DIR/release/examples/oop_slice" "$root" global-lookup > "$logs/global-lookup.log" 2>&1
"$fixture/bench-v2" "$CARGO_TARGET_DIR/release/libbirnam_id.dylib" \
    "$CARGO_TARGET_DIR/release/libbirnam_root_registry.dylib" \
    "$CARGO_TARGET_DIR/release/libbirnam_collect.dylib" > "$logs/independent-c.log" 2>&1
python3 - "$logs" <<'PY'
from pathlib import Path
import sys
p=Path(sys.argv[1])
assert 'OOP_SLICE_OK' in (p/'runtime.log').read_text()
assert 'FINALIZER_FAILURE_CONTAINED' in (p/'finalizer-failure.log').read_text()
assert 'STORAGE_PUBLICATION_FAILURE_CONTAINED' in (p/'storage-init-failure.log').read_text()
assert 'RETIREMENT_FAILURE_CONTAINED' in (p/'retirement-failure.log').read_text()
assert 'ALLOCATION_POLICY_OK' in (p/'allocation-policy.log').read_text()
assert 'GLOBAL_LOOKUP_OK' in (p/'global-lookup.log').read_text()
assert 'collect-garbage' in (p/'independent-c.log').read_text()
print((p/'runtime.log').read_text(),end='')
print((p/'finalizer-failure.log').read_text(),end='')
print('Independent C cold caller and collection: passed')
print('Receipts:',p)
PY
