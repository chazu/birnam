#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build --offline --locked -p birnam-id -p birnam-object-model -p birnam-method-observation -p birnam-root-registry -p birnam-gc-settings -p birnam-payload-layout -p birnam-trace-hooks -p birnam-finalizers -p birnam-native-stack -p birnam-native-mark -p birnam-allocation-budget -p birnam-namespace -p birnam-integer -p birnam-float -p birnam-word -p birnam-string -p birnam-trace -p birnam-collect -p birnam-store
selector_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-selector-enumeration.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
for fixture in selector-enumeration selector-enumeration-consumers; do
  cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v20 -Iscripts/fixtures "scripts/fixtures/$fixture.c" -o "$scratch/$fixture"
done
"$scratch/selector-enumeration-consumers" "$selector_target/debug/libbirnam_id.dylib" "$selector_target/debug/libbirnam_store.dylib"
python3 - "$scratch/selector-enumeration" "$selector_target/debug/libbirnam_id.dylib" <<'PYTHON'
import subprocess,sys
cases=['ordered','owner','observation','collection']+[f'invalid-{kind}{suffix}' for kind in ['null','instance','stale','garbage'] for suffix in ['', '-max']]
for case in cases:
    result=subprocess.run([sys.argv[1],sys.argv[2],case],capture_output=True,text=True)
    if case in ['ordered','collection']:
        assert result.returncode==0 and not result.stderr,(case,result)
        print(result.stdout,end='')
    else:
        expected='Rust floor entered from a non-owner thread' if case=='owner' else 'Id access during storage observation' if case=='observation' else 'vtable is not registered'
        assert result.returncode<0 and not result.stdout and result.stderr=='birnam-id invariant failed: '+expected+'\n',(case,result)
print('selector enumeration: owner/observation admission, invalid membership before bounds and GC callbacks passed')
PYTHON
