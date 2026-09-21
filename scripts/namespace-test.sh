#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-namespace -p birnam-object-model -p birnam-method-observation -p birnam-root-registry -p birnam-gc-settings -p birnam-payload-layout -p birnam-trace-hooks -p birnam-finalizers -p birnam-native-stack -p birnam-native-mark -p birnam-allocation-budget -p birnam-dispatch-cache -p birnam-storage -p birnam-id -p birnam-trace -p birnam-collect -p birnam-integer -p birnam-float -p birnam-word -p birnam-string --locked --offline
namespace_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-namespace.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v20 -Iscripts/fixtures scripts/fixtures/namespace.c -o "$scratch/namespace"
"$scratch/namespace" "$namespace_target/debug/libbirnam_id.dylib" provider
bash scripts/runtime-services-test.sh "${1:-}" namespace

cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v20 -Iscripts/fixtures scripts/fixtures/namespace-export-selection.c -o "$scratch/export-selection"
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
