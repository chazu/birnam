#!/usr/bin/env bash
# Focused debug startup, compiled artifact, saved-image and baked-image GC check.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: vector-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-vector-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'vector image smoke root: %s\n' "$scratch"
fi
cat >"$scratch/probe.birnam" <<'BIRNAM'
let vector := [[0 to: 1056] inject: [Vector new] into: [:acc :each | [acc add: each]]];
[System gc];
[System println: [[vector size] printString]];
[System println: [[vector at: 1056] printString]];
[System println: "vector-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
grep -Fxq '1057' "$scratch/startup.out"
grep -Fxq 'vector-startup-ok' "$scratch/startup.out"
grep -Fq '/libbirnam_vector.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"

cat >"$scratch/save.birnam" <<BIRNAM
let prefix := [[0 to: 31] inject: [Vector new] into: [:acc :each | [acc add: each]]];
let retained := [[32 to: 1056] inject: prefix into: [:acc :each | [acc add: each]]];
let alias := retained;
let main := [|
  [System gc];
  [System println: [[retained size] printString]];
  [System println: [[retained == alias] printString]];
  [System println: [[prefix size] printString]];
  [System println: [[retained at: 1056] printString]];
  [System println: [[retained inject: 0 into: [:sum :each | [sum + each]]] printString]];
  [System println: [[[retained add: 1057] size] printString]];
  [System println: [[retained size] printString]];
  [System println: [[[prefix reversed] at: 0] printString]];
  [System println: [[prefix copyFrom: 30 to: 32] printString]];
  [System println: [[[[1 to: 3] , prefix] size] printString]];
  [System println: [[[[[Map new] at: 7 put: 9] keys] at: 0] printString]];
  [System println: [[["a,b" split: ","] size] printString]];
  [System println: [[[prefix sortBy: [:each | [System gc]; [0 - each]]] at: 0] printString]];
  [System println: [[[prefix do: [:each | [System gc]]] == prefix] printString]];
  [Reflect defineMethod: #size on: VectorVt as: [:self | 99]];
  [System gc];
  [System println: [[retained size] printString]];
  [System println: "vector-image-ok"]
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
printf '1057\ntrue\n32\n1056\n558096\n1058\n1057\n31\n#[30 31]\n35\n7\n2\n31\ntrue\n99\nvector-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_vector.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_vector.dylib' "$scratch/baked.err"
printf 'vector-image-smoke: dynamic startup, source-free compiled module, native Code image and baked executable preserve persistent vectors, aliases, consumers, iteration, live dispatch and GC\n'
