#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: deferred-index-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
export BIRNAM_DEV_CLOSURE_ELISION=1 BIRNAM_DEV_CLOSURE_REPORT=1 BIRNAM_DEV_UNBOXED_NUMERIC=1
export BIRNAM_DEV_CLOSURE_FORCE_RECOVERY=${BIRNAM_DEV_CLOSURE_FORCE_RECOVERY:-0}
export BIRNAM_DEV_UNBOXED_NUMERIC_FORCE_AT=${BIRNAM_DEV_UNBOXED_NUMERIC_FORCE_AT:--1}
unset BIRNAM_DEV_CLOSURE_FORCE_AT
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-deferred-index-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then trap 'rm -rf "$scratch"' EXIT; else printf 'deferred-index image root: %s\n' "$scratch"; fi
filter_reports() {
  python3 - "$1" "$2" <<'PYREPORT'
from pathlib import Path
import sys
out,err=map(Path,sys.argv[1:])
lines=out.read_text().splitlines(keepends=True)
reports=[s for s in lines if s.startswith(('closure-elision: hit ', 'closure-elision: completed calls=', 'closure-elision: forced constructor recovery', 'deferred-index: phase='))]
out.write_text(''.join(s for s in lines if s not in reports))
err.write_text(err.read_text()+''.join(reports))
PYREPORT
}
check_reports() {
  if [[ $BIRNAM_DEV_CLOSURE_FORCE_RECOVERY == 1 ]]; then
    rg -Fxq 'closure-elision: forced constructor recovery' "$1"
  else
    rg -Fxq 'closure-elision: hit materialized=false' "$1"
    rg -Fxq 'closure-elision: hit materialized=true' "$1"
  fi
  if rg -v '^(closure-elision: (hit |completed calls=|forced constructor recovery)|deferred-index: phase=)' "$1"; then return 1; fi
}
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -Iabi/fixtures/idapi-v20 scripts/fixtures/deferred-index-native.c -o "$scratch/deferred-index-native.dylib"
service=deferred-index
source scripts/runtime-services-image-setup.sh
cat >"$scratch/probe.birnam" <<BIRNAM
[System loadModule: "$scratch/deferred-index-native.dylib"];
$(cat scripts/fixtures/deferred-index-workload.birnam)
let value := [Object clone];
let block := [Codegen materializeResumable: [CodeObject fromProgram: [CodeProgram parameters: #["argument"] captures: #[] statements: #[[CodeStatement expression: [CodeNode send: #deferredIndexIdentity operands: #[[CodeNode lexicalReadAt: 0]]]]]]] captures: #[]];
let total := [0 asCell];
[1 to: 30 do: [:i | [total set: [[total value] + i]]]];
[System println: [total value]];
[System println: [[[System deferredIndexExercise: block value: value] == value] printString]];
[System println: [[[System deferredIndexExercise: block value: value] == value] printString]];
[System println: "deferred-index-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
printf 'deferred-index-workload-ok\n465\ntrue\ntrue\ndeferred-index-startup-ok\n' >"$scratch/startup-expected.out"
filter_reports "$scratch/startup.out" "$scratch/startup.err"
cmp "$scratch/startup-expected.out" "$scratch/startup.out"
rg -Fq '/libbirnam_storage.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
filter_reports "$scratch/compiled.out" "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"
check_reports "$scratch/compiled.err"
cat >"$scratch/save.birnam" <<BIRNAM
[System loadModule: "$scratch/deferred-index-native.dylib"];
let value := [Object clone];
let block := [Codegen materializeResumable: [CodeObject fromProgram: [CodeProgram parameters: #["argument"] captures: #[] statements: #[[CodeStatement expression: [CodeNode send: #deferredIndexIdentity operands: #[[CodeNode lexicalReadAt: 0]]]]]]] captures: #[]];
let alias := value;
// Dynamic Reflect sends install the portable CodeBindings captured by image v8.
let reflector := Reflect;
let savedIndex := [Object clone];
let savedAlias := savedIndex;
[Globals at: #DeferredIndexSavedDictionary put: savedIndex];
$(for index in {0..23}; do printf '[reflector defineMethod: #deferredIndexSaved%s on: savedIndex as: [:self | %s]];\n' "$index" "$index"; done)
[System deferredIndexExercise: block value: value];
[System preciseGc: true];
[System gc];
let main := [|
$(for index in {0..23}; do printf '  [System println: [savedIndex deferredIndexSaved%s]];\n' "$index"; done)
  [System println: [[savedIndex == savedAlias] printString]];
  [System loadModule: "$scratch/deferred-index-native.dylib"];
$(cat scripts/fixtures/deferred-index-workload.birnam)
  let total := [0 asCell];
  [1 to: 30 do: [:i | [total set: [[total value] + i]]]];
  [System println: [total value]];
  [System preciseGc: true];
  [System gc];
  [System println: [[value == alias] printString]];
  [System println: [[[System deferredIndexExercise: block value: value] == alias] printString]];
  [System loadModule: "$scratch/deferred-index-native.dylib"];
  [System println: [[[System deferredIndexExercise: block value: alias] == value] printString]];
  [System println: "deferred-index-image-ok"]
];
[Globals at: #main put: main];
[System saveImage: "$scratch/app.image"];
[[[[System imageSaveReport] status] == #saved] ifTrue: [| nil] ifFalse: [| [System println: [[System imageSaveReport] detail]]; [OS exit: 40]]];
[System println: true]
BIRNAM
"$bin" --compile-module "$scratch/save.birnam" "$scratch/save.dylib"
rm "$scratch/save.birnam"
"$bin" "$scratch/save.dylib" >"$scratch/save.out" 2>"$scratch/save.err"
printf 'true\n' >"$scratch/save-expected.out"
filter_reports "$scratch/save.out" "$scratch/save.err"
test ! -s "$scratch/save.err"
cmp "$scratch/save-expected.out" "$scratch/save.out"
python3 - "$scratch/app.image" <<'PY'
import struct,sys
with open(sys.argv[1],'rb') as f:data=f.read()
assert data[:8]==b'BIRNAMIM' and struct.unpack_from('<I',data,8)[0]==8
offset,records=28,[]
while offset<len(data):
 kind,length=struct.unpack_from('<IQ',data,offset);offset+=12+length
 assert offset<=len(data);records.append((kind,length))
assert offset==len(data) and records[-1][0]==4 and records[-1][1]>72,records
PY
rm "$scratch/save.dylib"
for index in {0..23}; do printf '%s\n' "$index"; done >"$scratch/expected.out"
printf 'true\ndeferred-index-workload-ok\n465\ntrue\ntrue\ntrue\ndeferred-index-image-ok\n' >>"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
filter_reports "$scratch/image.out" "$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
rg -Fq '/libbirnam_storage.dylib' "$scratch/image.err"
"$bin" --rebuild "$scratch/app.image" >"$scratch/rebuilt.out" 2>"$scratch/rebuilt.err"
filter_reports "$scratch/rebuilt.out" "$scratch/rebuilt.err"
cmp "$scratch/expected.out" "$scratch/rebuilt.out"
check_reports "$scratch/rebuilt.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
filter_reports "$scratch/baked.out" "$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
rg -Fq '/libbirnam_storage.dylib' "$scratch/baked.err"
cache_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
mise exec -- cargo build --offline --locked -p birnam-host
mise exec -- rustc --edition=2024 -Cpanic=abort scripts/fixtures/deferred-image-host.rs --extern "birnam_host=$cache_target/debug/libbirnam_host.rlib" --extern "birnam_abi=$cache_target/debug/libbirnam_abi.rlib" -L "dependency=$cache_target/debug/deps" -o "$scratch/image-host"
for mode in mapped rebuilt baked; do
  artifact="$scratch/app.image"
  if [[ $mode == baked ]]; then artifact="$scratch/app"; fi
  "$scratch/image-host" "$prefix" "$mode" "$artifact" "$scratch/deferred-index-native.dylib" index >"$scratch/host-$mode.out" 2>"$scratch/host-$mode.err"
  filter_reports "$scratch/host-$mode.out" "$scratch/host-$mode.err"
  cmp "$scratch/expected.out" "$scratch/host-$mode.out"
  check_reports "$scratch/host-$mode.err"
done
# Additional harness runs observe the actual cell while every operation forwards
# to ordinary installed Storage. The six ordinary carrier checks above use no spy.
mkdir "$scratch/spy-lib"
python3 - "$BIRNAM_LIB_DIR" "$scratch/spy-lib" <<'PY'
from pathlib import Path
import shutil,sys
source,target=map(Path,sys.argv[1:])
for library in source.glob('*.dylib'):
    if library.name=='libbirnam_storage.dylib':continue
    if library.name=='libbirnam_id.dylib':shutil.copy2(library,target/library.name)
    else:(target/library.name).symlink_to(library.resolve())
PY
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -Iabi/fixtures/idapi-v20 scripts/fixtures/deferred-index-storage-spy.c -o "$scratch/spy-lib/libbirnam_storage.dylib"
for recovery in 0 1; do
  export BIRNAM_DEV_CLOSURE_FORCE_RECOVERY=$recovery
  export BIRNAM_DEV_UNBOXED_NUMERIC_FORCE_AT=$((2 * recovery - 1))
  for mode in mapped rebuilt baked; do
    artifact="$scratch/app.image";if [[ $mode == baked ]]; then artifact="$scratch/app";fi
    BIRNAM_LIB_DIR="$scratch/spy-lib" BIRNAM_DEFERRED_REAL_STORAGE="$prefix/lib/libbirnam_storage.dylib" \
      "$scratch/image-host" "$prefix" "$mode" "$artifact" "$scratch/deferred-index-native.dylib" index >"$scratch/spy-$recovery-$mode.out" 2>"$scratch/spy-$recovery-$mode.err"
    filter_reports "$scratch/spy-$recovery-$mode.out" "$scratch/spy-$recovery-$mode.err"
    cmp "$scratch/expected.out" "$scratch/spy-$recovery-$mode.out"
    check_reports "$scratch/spy-$recovery-$mode.err"
  done
done
python3 - "$scratch" <<'PY'
from pathlib import Path
import re,sys
root=Path(sys.argv[1]);counts={}
for recovery in [0,1]:
    for mode in ['mapped','rebuilt','baked']:
        lines=(root/f'spy-{recovery}-{mode}.err').read_text().splitlines()
        values=tuple(tuple(map(int,re.search(r'pushes=(\d+) removes=(\d+) writes=(\d+)',s).groups())) for phase in [1,3] for s in lines if s.startswith(f'deferred-index: phase={phase} '))
        assert len(values)==2 and min(values[0][:2])>0 and values[1]==(0,0,0),(mode,values)
        counts[recovery,mode]=values
        print('image actual clone-index operations',recovery,mode,values)
    assert counts[recovery,'mapped']==counts[recovery,'rebuilt']==counts[recovery,'baked'],counts
assert counts[0,'mapped'][0][0]==counts[1,'mapped'][0][0],counts
assert counts[0,'mapped'][1]==counts[1,'mapped'][1]==(0,0,0),counts
PY
printf "deferred-index-image-smoke: dynamic startup, source-free compiled caller, mapped/rebuilt native Code image and baked execution reconstruct a saved 24-method dictionary and execute real closure/numeric deferred construction and recovery, preserve physical identities, finalization, and large-dictionary canonical and overridden lookup, active Frame reads/writes, parent and behavior replacement, absent removal, aliases, GC and reload\n"
