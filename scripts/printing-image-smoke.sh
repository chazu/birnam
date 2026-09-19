#!/usr/bin/env bash
# Focused debug startup, compiled artifact, saved-image and baked-image GC check.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: printing-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-printing-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'printing image smoke root: %s\n' "$scratch"
fi
cat >"$scratch/probe.birnam" <<'BIRNAM'
let object := [Object clone];
let symbol := ["café" asSymbol];
[System gc];
[System println: [object printString]];
[System println: [object asString]];
[System println: [symbol asString]];
[System println: "printing-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
grep -Fxq '#café' "$scratch/startup.out"
grep -Fxq 'printing-startup-ok' "$scratch/startup.out"
grep -Fq '/libbirnam_printing.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"

cat >"$scratch/save.birnam" <<BIRNAM
let object := [Object clone];
let objectAlias := object;
let symbol := ["a b#café" asSymbol];
let symbolAlias := symbol;
let rendered := [symbol printString];
let main := [|
  [System gc];
  [System println: [object printString]];
  [System println: [object asString]];
  [System println: [[[object printString] == [object printString]] printString]];
  [System println: [symbol printString]];
  [System println: [symbol asString]];
  [System println: rendered];
  [System println: [[symbol == symbolAlias] printString]];
  [System println: [[["a b#café" asSymbol] == symbol] printString]];
  [System println: [[object == objectAlias] printString]];
  [Reflect defineMethod: #printString on: object as: [:self | "first"]];
  [System println: [object asString]];
  [Reflect defineMethod: #printString on: object as: [:self | "second"]];
  [System gc];
  [System println: [object asString]];
  [Reflect defineMethod: #printString on: SymbolVt as: [:self | "changed Symbol"]];
  [System println: [symbol asString]];
  [System println: rendered];
  [System println: [nil asString]];
  [System println: [42 asString]];
  [System println: [true asString]];
  [System println: "printing-image-ok"]
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
printf 'anObject\nanObject\nfalse\n#a b#café\n#a b#café\n#a b#café\ntrue\ntrue\ntrue\nfirst\nsecond\nchanged Symbol\n#a b#café\nnil\n42\ntrue\nprinting-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_printing.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_printing.dylib' "$scratch/baked.err"
printf 'printing-image-smoke: dynamic startup, source-free compiled module, native Code image and baked executable preserve Object/Symbol printing, aliases, copied text, live dispatch and GC\n'
