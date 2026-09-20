#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: mutation-epoch-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-mutation-epoch-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then trap 'rm -rf "$scratch"' EXIT; else printf 'mutation-epoch image root: %s\n' "$scratch"; fi
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -Iabi/fixtures/idapi-v20 scripts/fixtures/mutation-epoch-native.c -o "$scratch/mutation-epoch-native.dylib"
cat >"$scratch/probe.birnam" <<BIRNAM
[System loadModule: "$scratch/mutation-epoch-native.dylib"];
let value := [Object clone];
let block := [Codegen materializeResumable: [CodeObject fromProgram: [CodeProgram parameters: #["argument"] captures: #[] statements: #[[CodeStatement expression: [CodeNode send: #mutationEpochIdentity operands: #[[CodeNode lexicalReadAt: 0]]]]]]] captures: #[]];
let total := [0 asCell];
[1 to: 30 do: [:i | [total set: [[total value] + i]]]];
[System println: [total value]];
[System println: [[[System mutationEpochExercise: block value: value] == value] printString]];
[System println: [[[System mutationEpochExercise: block value: value] == value] printString]];
[System println: "mutation-epoch-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
printf '465\ntrue\ntrue\nmutation-epoch-startup-ok\n' >"$scratch/startup-expected.out"
cmp "$scratch/startup-expected.out" "$scratch/startup.out"
rg -Fq '/libbirnam_object_model.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"
test ! -s "$scratch/compiled.err"
cat >"$scratch/save.birnam" <<BIRNAM
[System loadModule: "$scratch/mutation-epoch-native.dylib"];
let value := [Object clone];
let block := [Codegen materializeResumable: [CodeObject fromProgram: [CodeProgram parameters: #["argument"] captures: #[] statements: #[[CodeStatement expression: [CodeNode send: #mutationEpochIdentity operands: #[[CodeNode lexicalReadAt: 0]]]]]]] captures: #[]];
let alias := value;
[System mutationEpochExercise: block value: value];
[System preciseGc: true];
[System gc];
let main := [|
  [System loadModule: "$scratch/mutation-epoch-native.dylib"];
  let total := [0 asCell];
  [1 to: 30 do: [:i | [total set: [[total value] + i]]]];
  [System println: [total value]];
  [System preciseGc: true];
  [System gc];
  [System println: [[value == alias] printString]];
  [System println: [[[System mutationEpochExercise: block value: value] == alias] printString]];
  [System loadModule: "$scratch/mutation-epoch-native.dylib"];
  [System println: [[[System mutationEpochExercise: block value: alias] == value] printString]];
  [System println: "mutation-epoch-image-ok"]
];
[Globals at: #main put: main];
[System saveImage: "$scratch/app.image"];
[System println: true]
BIRNAM
"$bin" --compile-module "$scratch/save.birnam" "$scratch/save.dylib"
rm "$scratch/save.birnam"
"$bin" "$scratch/save.dylib" >"$scratch/save.out" 2>"$scratch/save.err"
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
rm "$scratch/save.dylib"
printf '465\ntrue\ntrue\ntrue\nmutation-epoch-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
rg -Fq '/libbirnam_object_model.dylib' "$scratch/image.err"
"$bin" --rebuild "$scratch/app.image" >"$scratch/rebuilt.out" 2>"$scratch/rebuilt.err"
cmp "$scratch/expected.out" "$scratch/rebuilt.out"
test ! -s "$scratch/rebuilt.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
rg -Fq '/libbirnam_object_model.dylib' "$scratch/baked.err"
cache_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
mise exec -- cargo build --offline --locked -p birnam-host
mise exec -- rustc --edition=2024 -Cpanic=abort scripts/fixtures/dispatch-cache-image-host.rs --extern "birnam_host=$cache_target/debug/libbirnam_host.rlib" -L "dependency=$cache_target/debug/deps" -o "$scratch/image-host"
for mode in mapped rebuilt baked; do
  artifact="$scratch/app.image"
  if [[ $mode == baked ]]; then artifact="$scratch/app"; fi
  "$scratch/image-host" "$prefix" "$mode" "$artifact" >"$scratch/host-$mode.out" 2>"$scratch/host-$mode.err"
  cmp "$scratch/expected.out" "$scratch/host-$mode.out"
  test ! -s "$scratch/host-$mode.err"
done
printf "mutation-epoch-image-smoke: dynamic startup, source-free compiled caller, mapped/rebuilt native Code image and baked execution preserve epoch-driven canonical and overridden lookup, active Frame reads/writes, parent and behavior replacement, absent removal, aliases, GC and reload\n"
