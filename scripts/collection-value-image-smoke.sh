#!/usr/bin/env bash
# Focused debug startup, compiled artifact, saved-image and baked-image GC check.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: collection-value-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-collection-value-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'collection value image smoke root: %s\n' "$scratch"
fi
cat >"$scratch/probe.birnam" <<'BIRNAM'
let keys := #[0 0.0 #alpha "alpha"];
let table := [[[[[Map new] at: 0 put: "integer"] at: 0.0 put: "float"] at: #alpha put: "symbol"] at: "alpha" put: "string"];
[System gc];
[System println: [[keys indexOf: ["al" , "pha"]] printString]];
[System println: [table at: -0.0]];
[System println: [[table size] printString]];
[System println: "collection-value-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
grep -Fxq '3' "$scratch/startup.out"
grep -Fxq 'collection-value-startup-ok' "$scratch/startup.out"
grep -Fq '/libbirnam_collection_value.dylib' "$scratch/startup.err"
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
  [System println: "collection-value-image-ok"]
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
printf 'true\n5\ninteger\nfloat\nsymbol\nstring\nutf8\n0\n1\n2\n3\n5\ntrue\nstring\nchanged\n5\n99\ncollection-value-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_collection_value.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_collection_value.dylib' "$scratch/baked.err"
printf 'collection-value-image-smoke: dynamic startup, source-free compiled module, native Code image and baked executable preserve typed value keys, indexing, Map persistence, aliases, live dispatch and GC\n'
