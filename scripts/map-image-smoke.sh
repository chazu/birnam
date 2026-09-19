#!/usr/bin/env bash
# Focused debug startup, compiled artifact, saved-image and baked-image GC check.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: map-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-map-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'map image smoke root: %s\n' "$scratch"
fi
cat >"$scratch/probe.birnam" <<'BIRNAM'
let keys := #[0 0.0 #alpha "alpha"];
let table := [[[[[Map new] at: 0 put: "integer"] at: 0.0 put: "float"] at: #alpha put: "symbol"] at: "alpha" put: "string"];
[System gc];
[System println: [[keys indexOf: ["al" , "pha"]] printString]];
[System println: [table at: -0.0]];
[System println: [[table size] printString]];
[System println: "map-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
grep -Fxq '3' "$scratch/startup.out"
grep -Fxq 'map-startup-ok' "$scratch/startup.out"
grep -Fq '/libbirnam_map.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"

cat >"$scratch/save.birnam" <<BIRNAM
let object := [Object clone];
let keys := [#[0 0.0 #alpha "alpha" "café"] add: object];
let table := [[[[[[Map new] at: 0 put: "integer"] at: 0.0 put: "float"] at: #alpha put: "symbol"] at: "alpha" put: "string"] at: "café" put: "utf8"];
let alias := table;
let prefix := [[0 to: 31] inject: [Map new] into: [:acc :each | [acc at: each put: [each * 2]]]];
let retained := [[32 to: 256] inject: prefix into: [:acc :each | [acc at: each put: [each * 2]]]];
let persistentAlias := retained;
let main := [|
  [System gc];
  [System println: [[table == alias] printString]];
  [System println: [[table size] printString]];
  [System println: [table at: 0]];
  [System println: [table at: -0.0]];
  [System println: [table at: #alpha]];
  [System println: [table at: ["al" , "pha"]]];
  [System println: [table at: "café"]];
  [System println: [[keys indexOf: 0] printString]];
  [System println: [[keys indexOf: -0.0] printString]];
  [System println: [[keys indexOf: #alpha] printString]];
  [System println: [[keys indexOf: ["al" , "pha"]] printString]];
  [System println: [[keys indexOf: object] printString]];
  [System println: [[[keys indexOf: [Object clone]] == nil] printString]];
  let newer := [table at: ["al" , "pha"] put: "changed"];
  [System gc];
  [System println: [table at: "alpha"]];
  [System println: [newer at: "alpha"]];
  [System println: [[newer size] printString]];
  [Reflect defineMethod: ["indexOf:" asSymbol] on: VectorVt as: [:self :needle | 99]];
  [System println: [[keys indexOf: object] printString]];
  [System println: [[retained size] printString]];
  [System println: [[prefix size] printString]];
  [System println: [[retained == persistentAlias] printString]];
  [System println: [[retained at: 256] printString]];
  let changed := [retained at: 128 put: 999];
  [System gc];
  [System println: [[retained at: 128] printString]];
  [System println: [[changed at: 128] printString]];
  [System println: [[[retained keys] size] printString]];
  [System println: [[[retained values] size] printString]];
  [Reflect defineMethod: #size on: MapVt as: [:self | 777]];
  [System println: [[retained size] printString]];
  [System println: "map-image-ok"]
];
[System saveImage: "$scratch/app.image"]
BIRNAM
"$bin" "$scratch/save.birnam" >"$scratch/save.out" 2>"$scratch/save.err"
test ! -s "$scratch/save.err"
python3 - "$scratch/app.image" <<'PY'
import struct, sys
with open(sys.argv[1], 'rb') as f:
    data = f.read()
assert data[:8] == b'BIRNAMIM' and struct.unpack_from('<I', data, 8)[0] == 8
offset, records = 28, []
while offset < len(data):
    kind, length = struct.unpack_from('<IQ', data, offset)
    offset += 12 + length
    assert offset <= len(data)
    records.append((kind, length))
assert offset == len(data) and records[-1][0] == 4 and records[-1][1] > 72, records

PY
rm "$scratch/save.birnam"
printf 'true\n5\ninteger\nfloat\nsymbol\nstring\nutf8\n0\n1\n2\n3\n5\ntrue\nstring\nchanged\n5\n99\n257\n32\ntrue\n512\n256\n999\n257\n257\n777\nmap-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_map.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_map.dylib' "$scratch/baked.err"
printf 'map-image-smoke: dynamic startup, source-free compiled module, native Code image and baked executable preserve typed keys, persistent HAMTs, older versions, aliases, projections, live dispatch and GC\n'
