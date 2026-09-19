#!/usr/bin/env bash
# Focused debug startup, compiled artifact, saved-image and baked-image GC check.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: string-collection-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-string-collection-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'string collection image smoke root: %s\n' "$scratch"
fi
cat >"$scratch/probe.birnam" <<'BIRNAM'
let source := ",a,,café,";
let pieces := [source split: ","];
[System gc];
[System println: [[pieces size] printString]];
[System println: [pieces at: 3]];
[System println: [[[[source split: nil] at: 0] == source] printString]];
[System println: [[[[source split: ""] at: 0] == source] printString]];
[System println: "string-collection-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
grep -Fxq '5' "$scratch/startup.out"
grep -Fxq 'string-collection-startup-ok' "$scratch/startup.out"
grep -Fq '/libbirnam_string_collection.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"

cat >"$scratch/save.birnam" <<BIRNAM
let source := ",a,,café,";
let separator := ",";
let pieces := [source split: separator];
let alias := pieces;
let extended := [pieces add: "tail"];
let main := [|
  [System gc];
  [System println: [[pieces == alias] printString]];
  [System println: [[pieces size] printString]];
  [System println: [[extended size] printString]];
  [System println: [extended at: 5]];
  [System println: [pieces at: 0]];
  [System println: [pieces at: 1]];
  [System println: [pieces at: 2]];
  [System println: [pieces at: 3]];
  [System println: [pieces at: 4]];
  let fresh := [source split: separator];
  [System gc];
  [System println: [[fresh size] printString]];
  [System println: [fresh at: 3]];
  [System println: [[[[source split: nil] at: 0] == source] printString]];
  let copied := [source split: ""];
  [System println: [[copied size] printString]];
  [System println: [[[copied at: 0] == source] printString]];
  [System println: [copied at: 0]];
  let overlap := ["aaaaa" split: "aa"];
  [System println: [[overlap size] printString]];
  [System println: [overlap at: 2]];
  let empty := ["" split: ","];
  [System println: [[empty size] printString]];
  [System println: [[[empty at: 0] size] printString]];
  [System println: [[["caféécafé" split: "é"] size] printString]];
  [Reflect defineMethod: #new on: VectorVt as: [:self | nil]];
  [Reflect defineMethod: ["add:" asSymbol] on: VectorVt as: [:self :value | nil]];
  [System gc];
  [System println: [[[source split: separator] size] printString]];
  [Reflect defineMethod: ["split:" asSymbol] on: StringVt as: [:self :value | 42]];
  [System println: [[source split: separator] printString]];
  [System println: "string-collection-image-ok"]
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
printf 'true\n5\n6\ntail\n\na\n\ncafé\n\n5\ncafé\ntrue\n1\nfalse\n,a,,café,\n3\na\n1\n0\n4\n5\n42\nstring-collection-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_string_collection.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_string_collection.dylib' "$scratch/baked.err"
printf 'string-collection-image-smoke: dynamic startup, source-free compiled module, native Code image and baked executable preserve split parts, persistent vectors, aliases, UTF8 bytes, separator edge cases, native construction, live dispatch and GC\n'
