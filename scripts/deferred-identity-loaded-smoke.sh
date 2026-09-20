#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: deferred-identity-loaded-smoke.sh PREFIX}
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-deferred-identity-loaded.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then trap 'rm -rf "$scratch"' EXIT; else printf 'deferred-identity loaded root: %s\n' "$scratch"; fi
mkdir "$scratch/lib"
python3 - "$prefix/lib" "$scratch/lib" <<'PY'
from pathlib import Path
import shutil,sys
source,target=map(Path,sys.argv[1:])
for library in source.glob('*.dylib'):
    if library.name=='libbirnam_storage.dylib':continue
    if library.name=='libbirnam_id.dylib':shutil.copy2(library,target/library.name)
    else:(target/library.name).symlink_to(library.resolve())
PY
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -Iabi/fixtures/idapi-v20 scripts/fixtures/deferred-identity-storage-spy.c -o "$scratch/lib/libbirnam_storage.dylib"
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -Iabi/fixtures/idapi-v20 scripts/fixtures/deferred-identity-native.c -o "$scratch/probe.dylib"
printf '[System loadModule: "%s/probe.dylib"];\n' "$scratch" >"$scratch/probe.birnam"
cat scripts/fixtures/deferred-identity-workload.birnam >>"$scratch/probe.birnam"
export BIRNAM_LIB_DIR="$scratch/lib" BIRNAM_DEFERRED_REAL_STORAGE="$prefix/lib/libbirnam_storage.dylib"
unset BIRNAM_DEV_CLOSURE_FORCE_AT
"$prefix/bin/birnam" --compile-module "$scratch/probe.birnam" "$scratch/compiled.dylib"
for mode in disabled enabled recovery; do
  export BIRNAM_DEV_CLOSURE_ELISION=1 BIRNAM_DEV_CLOSURE_REPORT=1 BIRNAM_DEV_CLOSURE_FORCE_RECOVERY=0 BIRNAM_DEV_UNBOXED_NUMERIC=1 BIRNAM_DEV_UNBOXED_NUMERIC_FORCE_AT=-1
  if [[ $mode == disabled ]]; then export BIRNAM_DEV_CLOSURE_ELISION=0 BIRNAM_DEV_UNBOXED_NUMERIC=0; fi
  if [[ $mode == recovery ]]; then export BIRNAM_DEV_CLOSURE_FORCE_RECOVERY=1 BIRNAM_DEV_UNBOXED_NUMERIC_FORCE_AT=1; fi
  for carrier in source compiled; do
    artifact="$scratch/probe.birnam";if [[ $carrier == compiled ]]; then artifact="$scratch/compiled.dylib";fi
    "$prefix/bin/birnam" "$artifact" >"$scratch/$mode-$carrier.out" 2>"$scratch/$mode-$carrier.err"
  done
done
rm "$scratch/probe.birnam"
"$prefix/bin/birnam" "$scratch/compiled.dylib" >"$scratch/source-free.out" 2>"$scratch/source-free.err"
python3 - "$scratch" <<'PY'
from pathlib import Path
import sys
root=Path(sys.argv[1])
counts={}
for out in sorted(root.glob('*.out')):
    err=out.with_suffix('.err')
    lines=(out.read_text()+err.read_text()).splitlines()
    reports=[s for s in lines if s.startswith(('closure-elision: ', 'deferred-identity: phase='))]
    assert [s for s in lines if s not in reports]==['deferred-identity-workload-ok'],(out,lines)
    assert sum(s.startswith('deferred-identity: phase=1 ') for s in reports)==1,(out,reports)
    assert sum(s.startswith('deferred-identity: phase=3 ') for s in reports)==1,(out,reports)
    counts[out.stem]=tuple(int(s.rsplit('=',1)[1]) for phase in [1,3] for s in reports if s.startswith(f'deferred-identity: phase={phase} '))
    if out.name.startswith('enabled'):
        assert 'closure-elision: hit materialized=false' in reports
        assert 'closure-elision: hit materialized=true' in reports
    if out.name.startswith(('recovery','source-free')):
        assert 'closure-elision: forced constructor recovery' in reports
    print(out.stem,*(s for s in reports if s.startswith('deferred-identity: phase=')))
for mode in ['disabled','enabled','recovery']:
    assert counts[mode+'-source']==counts[mode+'-compiled'],counts
assert counts['disabled-source']==(0,0),counts
assert counts['enabled-source'][0]==counts['recovery-source'][0]>0,counts
assert counts['enabled-source'][1]==2*counts['recovery-source'][1]>0,counts
assert counts['source-free']==counts['recovery-compiled'],counts
print('deferred-identity-loaded-smoke: actual Heap cell has exact successor writes through both journals, fallback, forced recovery and source-free callers')
PY
