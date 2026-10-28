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
mise exec -- cargo test --offline --locked -p birnam-id -p birnam-root-registry -p birnam-namespace -p birnam-object-model -p birnam-word -p birnam-bytes -p birnam-string -p birnam-float -p birnam-integer -p birnam-vector -p birnam-map -p birnam-prelude -p birnam-system-input -p birnam-environment -p birnam-system-output -p birnam-file-io -p birnam-peg -p birnam-artifact -p birnam-execution -p birnam-codegen -p birnam-native-backend -p birnam-codegen-runtime --lib > "$logs/unit.log" 2>&1
mise exec -- cargo build --offline --locked --release --lib -p birnam-id -p birnam-root-registry -p birnam-collect -p birnam-namespace -p birnam-storage -p birnam-boolean -p birnam-nil -p birnam-object-model -p birnam-word -p birnam-bytes -p birnam-string -p birnam-float -p birnam-integer -p birnam-vector -p birnam-map -p birnam-prelude -p birnam-system-input -p birnam-environment -p birnam-system-output -p birnam-file-io -p birnam-peg -p birnam-artifact -p birnam-tcc -p birnam-execution -p birnam-lang-seed -p birnam-native-backend -p birnam-codegen-runtime -p birnam-portable-code > "$logs/build.log" 2>&1
mise exec -- cargo build --offline --locked --release -p birnam-siphon --example oop_slice > "$logs/caller-build.log" 2>&1
# Code-construction purity uses this explicit native floor and build-only grammar
# seed, not a published prefix or its old generated artifacts.
BIRNAM_TEST_MODULE_DIRECTORY="$CARGO_TARGET_DIR/release" mise exec -- cargo test --offline --locked -p birnam-siphon --test owned_code_construction --test native_code_construction -- --include-ignored > "$logs/owned-code-construction.log" 2>&1
python3 - "$CARGO_TARGET_DIR/release" <<'PY_EXPORTS'
from pathlib import Path
import json,subprocess,sys
for name in ['id','root-registry','collect','namespace','storage','boolean','nil','object-model','word','bytes','string','float','integer','vector','map','prelude','system-input','environment','system-output','file-io','peg','artifact','tcc','execution','lang-seed','native-backend','codegen-runtime']:
    manifest=json.loads(Path('rust/birnam-'+name+'/exports.json').read_text())
    library=Path(sys.argv[1])/('libbirnam_'+name.replace('-','_')+'.dylib')
    actual=set(subprocess.check_output(['nm','-gjU',str(library)],text=True).splitlines())
    assert actual == {'_'+s for s in manifest['exported_symbols']}, (name,actual)
print('Oop module export audit: passed')
PY_EXPORTS
for name in fixture failure rejected rejected-null; do
    clang -dynamiclib -O2 -Itests/fixtures/oop-id-v2 "tests/fixtures/oop-id-v2/$name.c" -o "$fixture/$name.dylib"
done
clang -dynamiclib -O2 -Itests/fixtures/oop-continuation tests/fixtures/oop-continuation/module.c -o "$fixture/continuation.dylib"
clang -dynamiclib -O2 -Itests/fixtures/oop-continuation tests/fixtures/oop-continuation/native.c -o "$fixture/continuation-native.dylib"
clang -O2 -DV2 -Itests/fixtures/oop-id-v2 tests/fixtures/oop-id-v2/bench.c -o "$fixture/bench-v2"
clang -O2 -Itests/fixtures/oop-id-v2 tests/fixtures/oop-id-v2/bench.c -o "$fixture/bench-v1"
# Explicit paths supply the entire runtime closure. There is no installed-prefix
# lookup or image bootstrap fallback. The numeric gate separately compiles
# the retained control sources directly from this checkout.
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
python3 - "$logs/input.bin" <<'PY_INPUT'
from pathlib import Path
import sys
Path(sys.argv[1]).write_bytes(b'a\x00\xff\r\n\nchanged\nlast')
PY_INPUT
BIRNAM_MODULES_DIR=/nonexistent BIRNAM_TEST_MODULE_DIRECTORY=/nonexistent \
python3 - "$CARGO_TARGET_DIR/release/examples/oop_slice" "$root" "$logs" <<'PY_NUMERIC'
import os,subprocess,sys
from pathlib import Path
exe,root,logs=sys.argv[1:]
env=dict(os.environb)
for name in [b'BIRNAM_OOP_TEST_MISSING',b'fixture-hello']:
    env.pop(name,None)
env.update({b'BIRNAM_OOP_TEST_PRESENT':'héllo'.encode(),b'BIRNAM_OOP_TEST_EMPTY':b'',b'BIRNAM_OOP_TEST_BINARY':b'\xff'})
with open(Path(logs)/'input.bin','rb') as source,open(Path(logs)/'numeric-integration.log','wb') as output:
    subprocess.run([exe,root,'numeric-integration'],stdin=source,stdout=output,stderr=subprocess.STDOUT,env=env,check=True)
PY_NUMERIC
BIRNAM_MODULES_DIR=/nonexistent BIRNAM_TEST_MODULE_DIRECTORY=/nonexistent \
    "$CARGO_TARGET_DIR/release/examples/oop_slice" "$root" numeric-input-error < "$root" > "$logs/input-error.log" 2>&1

"$fixture/bench-v2" "$CARGO_TARGET_DIR/release/libbirnam_id.dylib" \
    "$CARGO_TARGET_DIR/release/libbirnam_root_registry.dylib" \
    "$CARGO_TARGET_DIR/release/libbirnam_collect.dylib" > "$logs/independent-c.log" 2>&1
# Architectural contract: feature tests above cannot waive this requirement.
# Ordinary sends must enter their selected Method without a second lookup.
if ! BIRNAM_MODULES_DIR=/nonexistent BIRNAM_TEST_MODULE_DIRECTORY=/nonexistent \
    "$CARGO_TARGET_DIR/release/examples/oop_slice" "$root" dispatch-contract > "$logs/dispatch-contract.log" 2>&1; then
    cat "$logs/dispatch-contract.log"
    exit 1
fi
python3 - "$logs" <<'PY'
from pathlib import Path
import sys
p=Path(sys.argv[1])
assert 'OOP_SLICE_OK' in (p/'runtime.log').read_text()
assert 'DISPATCH_CONTRACT_OK' in (p/'dispatch-contract.log').read_text()
numeric=(p/'numeric-integration.log').read_bytes()
assert b'NUMERIC_INTEGRATION_OK' in numeric
assert b'TCC_OOP_OK' in numeric
assert b'COMPILER_EXECUTION_OOP_OK' in numeric
assert b'RESUMABLE_EXECUTION_OOP_OK' in numeric
for marker in [b'COMPILED_LOOKUP_SUSPENSION_REENTRY_REPLACEMENT_OK', b'COMPILED_INTERLEAVING_GC_OK', b'COMPILED_AUTOMATIC_GC_OK', b'COMPILED_NATIVE_CALLBACK_SAME_MACHINE_OK', b'COMPILED_NESTED_MACHINE_NATIVE_BOUNDARY_OK', b'RESUMABLE_ERROR_CONVENTION_OK']:
    assert marker in numeric, marker
for marker in [b'COMPILED_EXPLICIT_CONTROL_OK capture=false mode=resume', b'COMPILED_EXPLICIT_CONTROL_OK capture=true mode=resume', b'COMPILED_EXPLICIT_CONTROL_OK capture=false mode=budget', b'COMPILED_CONTROL_DISCARD_OK capture=false', b'COMPILED_CONTROL_DISCARD_OK capture=true', b'COMPILED_CONTROL_BARRIER_OK capture=false mode=native', b'COMPILED_CONTROL_BARRIER_OK capture=true mode=native', b'COMPILED_CONTROL_BARRIER_OK capture=false mode=callback', b'COMPILED_CONTROL_BARRIER_OK capture=true mode=callback']:
    assert marker in numeric, marker
for mode in ['exit', 'budget', 'native', 'return', 'ingress']:
    assert ('COMPILED_BOUNDARY_EXIT_OK mode='+mode).encode() in numeric, mode
for mode in ['return', 'leave', 'nested', 'park']:
    for bounded in ['false', 'true']:
        marker=f'COMPILED_CONTROL_ADAPTER_OK mode={mode} bounded={bounded}'.encode()
        assert marker in numeric, marker
for mode in ['eligible', 'native_eligible', 'native_park', 'native_leave']:
    marker=('COMPILED_CONTROL_NATIVE_OK mode='+mode).encode()
    assert marker in numeric, marker
for marker in [b'DEFAULT_COMPILER_ENTRYPOINTS_OK', b'DEFAULT_EVAL_SUSPEND_GC_OK bounded=false', b'DEFAULT_EVAL_SUSPEND_GC_OK bounded=true']:
    assert marker in numeric, marker
for marker in [b'SOURCE_METHOD_DEFINITION_OK', b'SOURCE_TASK_CONTROL_OK checks=177', b'SOURCE_HANDLER_CLEANUP_OK', b'#[#taskControlChecks 177 #failures 0]']:
    assert marker in numeric, marker
assert b'#[#FAILED ' not in numeric
for marker in [b'SOURCE_REGISTRY_METADATA_OWNERSHIP_OK', b'SOURCE_REGISTRY_LIVE_SUSPENSION_OK']:
    assert marker in numeric, marker
for marker in [b'SOURCE_FILE_LOADING_DIAGNOSTICS_OK', b'SOURCE_BACKEND_MODES_OK', b'SOURCE_FILE_LIVE_OWNERS_FILENAME_OK', b'SOURCE_FILE_EMBEDDING_OK', b'SOURCE_FILE_SUSPENSION_GC_OK']:
    assert marker in numeric, marker
for marker in [b'ARTIFACT_COLD_ERROR_OWNER_OK', b'ARTIFACT_PARSER_FREE_ACTIVATION_OK', b'ARTIFACT_REJECTION_BEFORE_EFFECTS_OK', b'ARTIFACT_FROZEN_SNAPSHOT_OK', b'ARTIFACT_TASK_CODE_LIFETIME_OK']:
    assert marker in numeric, marker
for marker in [b'ARTIFACT_SYNCHRONOUS_BODIES_OK', b'ARTIFACT_LIBRARY_PACKAGING_OK']:
    assert marker in numeric, marker
for expected in [b'OOP_STDOUT:\x00\xff\n', b'OOP_NUMBER:2\n', b'OOP_REENTRY;OOP_REENTRY;\n', 'OOP_STDERR:\x00\ufffd'.encode()]:
    assert expected in numeric, repr(expected)
assert 'SYSTEM_INPUT_ERROR_OK' in (p/'input-error.log').read_text()
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
