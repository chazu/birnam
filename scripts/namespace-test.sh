#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-namespace -p birnam-object-model -p birnam-method-observation -p birnam-root-registry -p birnam-gc-settings -p birnam-payload-layout -p birnam-id -p birnam-trace -p birnam-collect -p birnam-integer -p birnam-float -p birnam-word -p birnam-string --locked --offline
namespace_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-namespace.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v17 -Iscripts/fixtures scripts/fixtures/namespace.c -o "$scratch/namespace"
"$scratch/namespace" "$namespace_target/debug/libbirnam_id.dylib" provider
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v17 -Iscripts/fixtures scripts/fixtures/namespace-presence-boundary.c -o "$scratch/presence-boundary"
python3 - "$scratch/presence-boundary" "$namespace_target/debug/libbirnam_id.dylib" <<'PY'
import subprocess, sys
cases = {
    'null': 'IdApi.import name is null',
    'owner': 'Rust floor entered from a non-owner thread',
    'null-owner': 'IdApi.import name is null',
    'observation': 'Id access during storage observation',
    'null-observation': 'IdApi.import name is null',
}
for mode, diagnostic in cases.items():
    result = subprocess.run([sys.argv[1], sys.argv[2], mode], capture_output=True, text=True)
    assert result.returncode < 0, (mode, result)
    assert result.stderr == 'birnam-id invariant failed: ' + diagnostic + '\n', (mode, result.stderr)
result = subprocess.run([sys.argv[1], sys.argv[2], 'collection'], capture_output=True, text=True)
assert result.returncode == 0 and not result.stderr, result
assert result.stdout == 'presence remains callable during root collection\n', result.stdout
print('namespace presence: null/owner/observation precedence and collection callbacks passed')
PY
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v17 -Iscripts/fixtures scripts/fixtures/namespace-publication-boundary.c -o "$scratch/publication-boundary"
python3 - "$scratch/publication-boundary" "$namespace_target/debug/libbirnam_id.dylib" <<'PY'
import subprocess, sys
cases = {
    'null': 'IdApi.exportObj name is null',
    'owner': 'Rust floor entered from a non-owner thread',
    'null-owner': 'IdApi.exportObj name is null',
    'observation': 'Id access during storage observation',
    'null-observation': 'IdApi.exportObj name is null',
}
for mode, diagnostic in cases.items():
    result = subprocess.run([sys.argv[1], sys.argv[2], mode], capture_output=True, text=True)
    assert result.returncode < 0, (mode, result)
    assert result.stderr == 'birnam-id invariant failed: ' + diagnostic + '\n', (mode, result.stderr)
result = subprocess.run([sys.argv[1], sys.argv[2], 'collection'], capture_output=True, text=True)
assert result.returncode == 0 and not result.stderr, result
assert result.stdout == 'publication remains callable during root collection\n', result.stdout
print('namespace publication: null/owner/observation precedence and collection callbacks passed')
PY
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v17 -Iscripts/fixtures scripts/fixtures/namespace-import-boundary.c -o "$scratch/import-boundary"
python3 - "$scratch/import-boundary" "$namespace_target/debug/libbirnam_id.dylib" <<'PY'
import subprocess, sys
cases = {
    'null': 'IdApi.import name is null',
    'owner': 'Rust floor entered from a non-owner thread',
    'null-owner': 'IdApi.import name is null',
    'observation': 'Id access during storage observation',
    'null-observation': 'IdApi.import name is null',
}
for mode, diagnostic in cases.items():
    result = subprocess.run([sys.argv[1], sys.argv[2], mode], capture_output=True, text=True)
    assert result.returncode < 0, (mode, result)
    assert result.stderr == 'birnam-id invariant failed: ' + diagnostic + '\n', (mode, result.stderr)
result = subprocess.run([sys.argv[1], sys.argv[2], 'collection'], capture_output=True, text=True)
assert result.returncode == 0 and not result.stderr, result
assert result.stdout == 'import remains callable during root collection\n', result.stdout
print('namespace import: null/owner/observation precedence and collection callbacks passed')
PY

cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v17 -Iscripts/fixtures scripts/fixtures/namespace-export-selection.c -o "$scratch/export-selection"
python3 - "$scratch/export-selection" "$namespace_target/debug/libbirnam_id.dylib" <<'PYTEST'
import subprocess,sys
for family in ['Integer','Float','Word','String']:
    for mode in ['construction','between']:
        result=subprocess.run([sys.argv[1],sys.argv[2],family,mode],capture_output=True,text=True)
        expected=['construction replacement'] if mode=='construction' else []
        expected += [('first' if family=='Integer' or mode=='between' else 'second')+' '+family,
                     ('first' if family=='Integer' else 'second')+' '+family+'Vt',
                     'export selection and rooted identities passed']
        assert result.returncode==0 and not result.stderr and result.stdout.splitlines()==expected,(family,mode,result)
print('namespace export: Integer snapshot, live Float/Word/String reads, context replacement and callback GC passed')
PYTEST
