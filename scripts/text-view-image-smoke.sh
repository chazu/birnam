#!/usr/bin/env bash
# Focused debug startup, compiled artifact, saved-image and baked-image GC check.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: text-view-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-text-view-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'text view image smoke root: %s\n' "$scratch"
fi
export BIRNAM_TEXT_VIEW_IMAGE="from environment"
cat >"$scratch/probe.birnam" <<BIRNAM
[System writeFile: "$scratch/data" contents: "café"];
[System print: [System readFile: "$scratch/data"]];
[System println: " output"];
[System println: [System getEnv: "BIRNAM_TEXT_VIEW_IMAGE"]];
[System println: "text-view-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
grep -Fxq 'café output' "$scratch/startup.out"
grep -Fxq 'text-view-startup-ok' "$scratch/startup.out"
grep -Fq '/libbirnam_text_view.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"

cat >"$scratch/save.birnam" <<BIRNAM
let path := "$scratch/image-data";
let payload := "retained café";
let alias := payload;
let object := [Object clone];
let main := [|
  [System gc];
  [System println: [[payload == alias] printString]];
  [System writeFile: path contents: payload];
  let loaded := [System readFile: path];
  [System println: [[loaded = payload] printString]];
  [System print: loaded];
  [System println: " output"];
  [System println: [System getEnv: "BIRNAM_TEXT_VIEW_IMAGE"]];
  [Reflect defineMethod: #printString on: object as: [:self | "first"]];
  [System println: object];
  [Reflect defineMethod: #printString on: object as: [:self | "second"]];
  [System gc];
  [System println: object];
  [System println: ""];
  [System print: "joined"];
  [System println: " output"];
  [System println: "text-view-image-ok"]
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
printf 'true\ntrue\nretained café output\nfrom environment\nfirst\nsecond\n\njoined output\ntext-view-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_text_view.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_text_view.dylib' "$scratch/baked.err"
printf 'text-view-image-smoke: dynamic startup, source-free compiled module, native Code image and baked executable preserve byte output, file/environment arguments, aliases, live printing lookup and GC\n'
