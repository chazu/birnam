#!/usr/bin/env bash
# Focused debug startup, compiled artifact, saved-image and baked-image GC check.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: system-gc-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-system-gc-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'system gc image smoke root: %s\n' "$scratch"
fi
cat >"$scratch/probe.birnam" <<'BIRNAM'
[System println: [[[System gc] >= 0] printString]];
[System println: [[[System liveObjects] > 0] printString]];
[System println: [[[System autoGc: false] == System] printString]];
[System println: [[[System preciseGc: false] == System] printString]];
[System println: "system-gc-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
grep -Fxq 'true' "$scratch/startup.out"
grep -Fxq 'system-gc-startup-ok' "$scratch/startup.out"
grep -Fq '/libbirnam_system_gc.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"

cat >"$scratch/save.birnam" <<BIRNAM
let receiver := System;
let alias := receiver;
let retained := #[17 "alive" #symbol];
let retainedAlias := retained;
let main := [|
  [System println: [[[receiver autoGc: false] == receiver] printString]];
  [System println: [[[receiver preciseGc: true] == receiver] printString]];
  [System println: [[[receiver gc] >= 0] printString]];
  [System println: [[[receiver liveObjects] > 0] printString]];
  [System println: [[receiver == alias] printString]];
  [System println: [[retained == retainedAlias] printString]];
  [System println: [retained at: 1]];
  [System println: [[[receiver autoGc: nil] == nil] printString]];
  [System println: [[[receiver preciseGc: 0] == nil] printString]];
  [System println: [[[receiver preciseGc: false] == receiver] printString]];
  [System println: [[[receiver autoGc: true] == receiver] printString]];
  [System gc];
  [System println: [[retained at: 0] printString]];
  [Reflect defineMethod: #gc on: SystemVt as: [:self | 777]];
  [System println: [[receiver gc] printString]];
  [Reflect defineMethod: #liveObjects on: SystemVt as: [:self | 888]];
  [System println: [[receiver liveObjects] printString]];
  [Reflect defineMethod: ["autoGc:" asSymbol] on: SystemVt as: [:self :enabled | 99]];
  [System println: [[receiver autoGc: false] printString]];
  [Reflect defineMethod: ["preciseGc:" asSymbol] on: SystemVt as: [:self :enabled | 66]];
  [System println: [[receiver preciseGc: false] printString]];
  [System println: "system-gc-image-ok"]
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
printf 'true\ntrue\ntrue\ntrue\ntrue\ntrue\nalive\ntrue\ntrue\ntrue\ntrue\n17\n777\n888\n99\n66\nsystem-gc-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_system_gc.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_system_gc.dylib' "$scratch/baked.err"
printf 'system-gc-image-smoke: dynamic startup, source-free compiled module, native Code image and baked executable preserve collector controls, aliases, retained values, invalid arguments and live dispatch\n'
