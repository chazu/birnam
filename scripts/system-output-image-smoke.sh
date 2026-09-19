#!/usr/bin/env bash
# Focused debug startup, compiled artifact, saved-image and baked-image output check.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: system-output-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-system-output-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'system output image smoke root: %s\n' "$scratch"
fi
cat >"$scratch/probe.birnam" <<'BIRNAM'
[System print: "café"];
[System println: " output"];
[System println: [[[System print: ""] == System] printString]];
[System println: "system-output-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
grep -Fxq 'true' "$scratch/startup.out"
grep -Fxq 'system-output-startup-ok' "$scratch/startup.out"
grep -Fq '/libbirnam_system_output.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"

cat >"$scratch/save.birnam" <<BIRNAM
let receiver := [System clone];
let alias := receiver;
let payload := "retained café";
let payloadAlias := payload;
let object := [Object clone];
let main := [|
  [System autoGc: false];
  [System preciseGc: true];
  [System gc];
  [System println: [[receiver == alias] printString]];
  [System println: [[payload == payloadAlias] printString]];
  [System println: [[[receiver print: payload] == receiver] printString]];
  [System println: [[[receiver println: " line"] == receiver] printString]];
  [receiver println: ""];
  [Reflect defineMethod: #printString on: object as: [:self | "first"]];
  [receiver println: object];
  [Reflect defineMethod: #printString on: object as: [:self | "second"]];
  [System gc];
  [receiver println: object];
  [Reflect defineMethod: ["print:" asSymbol] on: receiver as: [:self :value | "replacement print"]];
  [System println: [receiver print: "ignored"]];
  [Reflect defineMethod: ["println:" asSymbol] on: receiver as: [:self :value | "replacement println"]];
  [System println: [receiver println: "ignored"]];
  [System preciseGc: false];
  [System autoGc: true];
  [System println: "system-output-image-ok"]
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
printf 'true\ntrue\nretained cafétrue\n line\ntrue\n\nfirst\nsecond\nreplacement print\nreplacement println\nsystem-output-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_system_output.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_system_output.dylib' "$scratch/baked.err"
printf 'system-output-image-smoke: dynamic startup, source-free compiled module, native Code image and baked executable preserve byte output, receiver aliases, retained values, precise GC and live method replacement\n'
