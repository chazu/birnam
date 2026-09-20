#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: method-observation-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-method-observation-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then trap 'rm -rf "$scratch"' EXIT; else printf 'method-observation image root: %s\n' "$scratch"; fi
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -Iabi/fixtures/idapi-v15 scripts/fixtures/method-observation-native.c -o "$scratch/method-observation-native.dylib"
cat >"$scratch/probe.birnam" <<BIRNAM
[System loadModule: "$scratch/method-observation-native.dylib"];
let value := [Object clone];
[System println: [[[System observationExercise: value] == value] printString]];
[System println: [[[System observationExercise: value] == value] printString]];
[System println: "method-observation-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
printf 'true\ntrue\nmethod-observation-startup-ok\n' >"$scratch/startup-expected.out"
cmp "$scratch/startup-expected.out" "$scratch/startup.out"
grep -Fq '/libbirnam_method_observation.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"
test ! -s "$scratch/compiled.err"
cat >"$scratch/save.birnam" <<BIRNAM
[System loadModule: "$scratch/method-observation-native.dylib"];
let value := [Object clone];
let alias := value;
[System observationExercise: value];
let main := [|
  [System loadModule: "$scratch/method-observation-native.dylib"];
  [System preciseGc: true];
  [System gc];
  [System println: [[value == alias] printString]];
  [System println: [[[System observationExercise: value] == alias] printString]];
  [System loadModule: "$scratch/method-observation-native.dylib"];
  [System println: [[[System observationExercise: alias] == value] printString]];
  [System println: "method-observation-image-ok"]
];
[System saveImage: "$scratch/app.image"];
[System println: true]
BIRNAM
"$bin" "$scratch/save.birnam" >"$scratch/save.out" 2>"$scratch/save.err"
test ! -s "$scratch/save.err"
printf 'true\n' >"$scratch/save-expected.out"
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
rm "$scratch/save.birnam"
printf 'true\ntrue\ntrue\nmethod-observation-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_method_observation.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_method_observation.dylib' "$scratch/baked.err"
printf 'method-observation-image-smoke: startup, source-free compiled caller, v8 native Code image and baked executable preserve same-address overwrite/removal, Store mirror retraction, basis, roots, aliases and reload\n'
