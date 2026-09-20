#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build --offline --locked -p birnam-id -p birnam-object-model -p birnam-method-observation -p birnam-root-registry -p birnam-gc-settings -p birnam-namespace -p birnam-integer -p birnam-float -p birnam-word -p birnam-string -p birnam-trace -p birnam-collect -p birnam-store -p birnam-tcc -p birnam-codegen-runtime
enumeration_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-vtable-enumeration.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
for fixture in vtable-enumeration vtable-enumeration-consumers vtable-enumeration-boundary; do
  cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v16 -Iscripts/fixtures "scripts/fixtures/$fixture.c" -o "$scratch/$fixture"
done
"$scratch/vtable-enumeration" "$enumeration_target/debug/libbirnam_id.dylib"
"$scratch/vtable-enumeration-consumers" "$enumeration_target/debug/libbirnam_id.dylib" "$enumeration_target/debug/libbirnam_store.dylib" "$enumeration_target/debug/libbirnam_tcc.dylib" "$enumeration_target/debug/libbirnam_codegen_runtime.dylib"
python3 - "$scratch/vtable-enumeration-boundary" "$enumeration_target/debug/libbirnam_id.dylib" <<'PY'
import subprocess,sys
for operation in ('count','at'):
    for mode,diagnostic in [('owner','Rust floor entered from a non-owner thread'),('observation','Id access during storage observation'),('collection',None)]:
        result=subprocess.run([sys.argv[1],sys.argv[2],operation+'-'+mode],capture_output=True,text=True)
        if diagnostic:
            assert result.returncode<0 and not result.stdout and result.stderr=='birnam-id invariant failed: '+diagnostic+'\n',result
        else:
            assert result.returncode==0 and not result.stderr and result.stdout=='enumeration remains callable during root collection\n',result
print('vtable enumeration: owner/observation admission and root callbacks passed')
PY
