#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: storage-observation-scope-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-storage-observation-scope-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then trap 'rm -rf "$scratch"' EXIT; else printf 'storage-observation-scope image root: %s\n' "$scratch"; fi
scope_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
mise exec -- cargo build --offline --locked -p birnam-host -p birnam-abi
mise exec -- rustc --edition=2024 -Cpanic=abort scripts/fixtures/storage-observation-scope-image-host.rs --extern "birnam_host=$scope_target/debug/libbirnam_host.rlib" --extern "birnam_abi=$scope_target/debug/libbirnam_abi.rlib" -L "dependency=$scope_target/debug/deps" -o "$scratch/host-observer"
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -Iabi/fixtures/idapi-v20 scripts/fixtures/storage-observation-scope-native.c -o "$scratch/storage-observation-scope-native.dylib"
cat >"$scratch/probe.birnam" <<BIRNAM
[System loadModule: "$scratch/storage-observation-scope-native.dylib"];
let value := [Object clone];
[Globals at: #ObservationSample put: value];
[Globals at: #ObservationAlias put: value];
let total := [0 asCell];
[1 to: 30 do: [:i | [total set: [[total value] + i]]]];
[System println: [total value]];
[System println: [[[System observationScopeExercise: value] == value] printString]];
[System println: [[[System observationScopeExercise: value] == value] printString]];
[System println: "storage-observation-scope-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
printf '465\ntrue\ntrue\nstorage-observation-scope-startup-ok\n' >"$scratch/startup-expected.out"
cmp "$scratch/startup-expected.out" "$scratch/startup.out"
rg -Fq '/libbirnam_storage.dylib' "$scratch/startup.err"
"$scratch/host-observer" "$prefix" source "$scratch/probe.birnam" >"$scratch/host-source.out" 2>"$scratch/host-source.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"
test ! -s "$scratch/compiled.err"
"$scratch/host-observer" "$prefix" compiled "$scratch/probe.dylib" >"$scratch/host-compiled.out" 2>"$scratch/host-compiled.err"
cat >"$scratch/save.birnam" <<BIRNAM
[System loadModule: "$scratch/storage-observation-scope-native.dylib"];
let value := [Object clone];
[Globals at: #ObservationSample put: value];
[Globals at: #ObservationAlias put: value];
let alias := value;
[System observationScopeExercise: value];
let main := [|
  [System loadModule: "$scratch/storage-observation-scope-native.dylib"];
  let total := [0 asCell];
  [1 to: 30 do: [:i | [total set: [[total value] + i]]]];
  [System println: [total value]];
  [System preciseGc: true];
  [System gc];
  [System println: [[value == alias] printString]];
  [System println: [[[System observationScopeExercise: value] == alias] printString]];
  [System loadModule: "$scratch/storage-observation-scope-native.dylib"];
  [System println: [[[System observationScopeExercise: alias] == value] printString]];
  [System println: "storage-observation-scope-image-ok"]
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
printf '465\ntrue\ntrue\ntrue\nstorage-observation-scope-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
rg -Fq '/libbirnam_storage.dylib' "$scratch/image.err"
"$bin" --rebuild "$scratch/app.image" >"$scratch/rebuilt.out" 2>"$scratch/rebuilt.err"
cmp "$scratch/expected.out" "$scratch/rebuilt.out"
test ! -s "$scratch/rebuilt.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
rg -Fq '/libbirnam_storage.dylib' "$scratch/baked.err"
for mode in mapped rebuilt baked; do
  artifact="$scratch/app.image"
  if [[ $mode == baked ]]; then artifact="$scratch/app"; fi
  "$scratch/host-observer" "$prefix" "$mode" "$artifact" >"$scratch/host-$mode.out" 2>"$scratch/host-$mode.err"
done
for mode in source compiled mapped rebuilt baked; do
  test ! -s "$scratch/host-$mode.err"
  expected="$scratch/expected.out"
  if [[ $mode == source || $mode == compiled ]]; then expected="$scratch/startup-expected.out"; fi
  { cat "$expected"; printf 'observation-host-%s: epochs 1..5, nested BUSY, visitor failure cleanup, aliases and allocation identity after GC\n' "$mode"; } >"$scratch/host-$mode-expected.out"
  cmp "$scratch/host-$mode-expected.out" "$scratch/host-$mode.out"
done
printf "storage-observation-scope-image-smoke: dynamic startup, source-free compiled caller, mapped/rebuilt native Code image and baked execution; protected host observations, nested rejection, failure cleanup, roots, aliases and reload passed\n"
