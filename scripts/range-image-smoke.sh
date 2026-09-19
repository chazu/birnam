#!/usr/bin/env bash
# Focused debug startup, compiled artifact, saved-image and baked-image GC check.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: range-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-range-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'range image smoke root: %s\n' "$scratch"
fi
cat >"$scratch/probe.birnam" <<'BIRNAM'
let interval := [-2 to: 2];
[System gc];
[System println: [[interval size] printString]];
[System println: [[interval inject: 0 into: [:sum :each | [sum + each]]] printString]];
[System println: "range-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
grep -Fxq '5' "$scratch/startup.out"
grep -Fxq 'range-startup-ok' "$scratch/startup.out"
grep -Fq '/libbirnam_range.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"

cat >"$scratch/save.birnam" <<BIRNAM
let retained := [-2 to: 2];
let alias := retained;
let main := [|
  [System gc];
  [System println: [[retained size] printString]];
  [System println: [[retained == alias] printString]];
  [System println: [[retained includes: 0] printString]];
  [System println: [[retained at: 2] printString]];
  [System println: [retained printString]];
  [System println: [[retained reversed] printString]];
  [System println: [[retained , #[9]] printString]];
  [System println: [[[5 to: 5] size] printString]];
  [System println: [[[5 to: 3] size] printString]];
  [System println: [[retained inject: 0 into: [:sum :each | [sum + each]]] printString]];
  [System println: [[[retained do: [:each | [System gc]]] == retained] printString]];
  [Reflect defineMethod: #size on: RangeVt as: [:self | 99]];
  [System gc];
  [System println: [[retained size] printString]];
  [System println: "range-image-ok"]
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
# The captured alias pair is one object with the unchanged two-RAW-word codec.
name = b'RangeVt'
record = bytes([9]) + struct.pack('<I', len(name)) + name + struct.pack('<I', 2)
assert data.count(record) == 1
offset = data.index(record) + len(record)
assert data[offset] == 3 and data[offset + 9] == 3
assert struct.unpack_from('<q', data, offset + 1)[0] == -2
assert struct.unpack_from('<q', data, offset + 10)[0] == 2
PY
rm "$scratch/save.birnam"
printf '5\ntrue\ntrue\n0\n#[-2 -1 0 1 2]\n#[2 1 0 -1 -2]\n#[-2 -1 0 1 2 9]\n1\n0\n0\ntrue\n99\nrange-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_range.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_range.dylib' "$scratch/baked.err"
printf 'range-image-smoke: dynamic startup, source-free compiled module, native Code image and baked executable preserve Range bounds, aliases, Vector results, stackless iteration, live dispatch and GC\n'
