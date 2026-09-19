#!/usr/bin/env bash
# Focused debug startup, compiled artifact, saved-image and baked-image GC check.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: string-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-string-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'string image smoke root: %s\n' "$scratch"
fi
cat >"$scratch/probe.birnam" <<'BIRNAM'
[System gc];
[System println: [["birnam" size] printString]];
[System println: "string-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
grep -Fxq '6' "$scratch/startup.out"
grep -Fxq 'string-startup-ok' "$scratch/startup.out"
grep -Fq '/libbirnam_string.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"

cat >"$scratch/save.birnam" <<BIRNAM
let retained := "birnam";
let alias := retained;
let main := [|
  [System gc];
  [System println: [[retained size] printString]];
  [System println: [[retained == alias] printString]];
  [System println: [retained copyFrom: 1 to: 4]];
  [System println: [[" 42 " asInteger] printString]];
  [System println: [["1.5" asNumber] printString]];
  [System println: [[["a,b" split: ","] size] printString]];
  [Reflect defineMethod: #size on: String as: [:self | 99]];
  [System gc];
  [System println: [[retained size] printString]];
  [System println: "string-image-ok"]
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
printf '6\ntrue\nirn\n42\n1.5\n2\n99\nstring-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_string.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_string.dylib' "$scratch/baked.err"
printf 'string-image-smoke: dynamic startup, compiled module, saved compiled image and baked image preserve String bytes, conversions, alias identity, Array splitting, live dispatch and GC\n'
