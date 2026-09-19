#!/usr/bin/env bash
# Focused debug startup, compiled module, saved-image and baked-image file IO.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: file-io-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-file-io-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'file IO image smoke root: %s\n' "$scratch"
fi
printf 'a\000\377\r\n' >"$scratch/input"
cp "$scratch/input" "$scratch/original"
cat >"$scratch/probe.birnam" <<BIRNAM
let payload := [System readFile: "$scratch/input"];
[System println: [[[System writeFile: "$scratch/output" contents: payload] == System] printString]];
[System println: [[[System readFile: "$scratch/output"] = payload] printString]];
[System println: [[System readFile: "$scratch/missing"] kind]];
[System println: [[System writeFile: "$scratch" contents: payload] message]];
[System println: "file-io-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
printf 'true\ntrue\n#ioError\ncould not write file\nfile-io-startup-ok\n' >"$scratch/startup-expected.out"
cmp "$scratch/startup-expected.out" "$scratch/startup.out"
cmp "$scratch/input" "$scratch/output"
grep -Fq '/libbirnam_file_io.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam" "$scratch/output"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"
cmp "$scratch/input" "$scratch/output"
cat >"$scratch/save.birnam" <<BIRNAM
let receiver := [System clone];
let alias := receiver;
let path := "$scratch/input";
let pathAlias := path;
let payload := [System readFile: path];
let payloadAlias := payload;
let answer := [Object clone];
let main := [|
  [System autoGc: false];
  [System preciseGc: true];
  [System gc];
  [System println: [[receiver == alias] printString]];
  [System println: [[path == pathAlias] printString]];
  [System println: [[payload == payloadAlias] printString]];
  [System println: [[[receiver writeFile: "$scratch/output" contents: payload] == alias] printString]];
  [System println: [[[receiver readFile: "$scratch/output"] = payload] printString]];
  [System println: [receiver readFile: path]];
  [receiver writeFile: "$scratch/empty" contents: ""];
  [System println: [[[receiver readFile: "$scratch/empty"] = ""] printString]];
  [System println: [[receiver readFile: "$scratch/missing"] kind]];
  [System println: [[receiver readFile: "$scratch/missing"] message]];
  [System println: [[receiver writeFile: "$scratch" contents: payload] message]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | answer]];
  [System println: [[[receiver readFile: "$scratch/missing"] == answer] printString]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | nil]];
  [System println: [[[receiver writeFile: "$scratch" contents: payload] == nil] printString]];
  [Reflect defineMethod: #readFile: on: receiver as: [:self :name | "replacement"]];
  [Reflect defineMethod: ["writeFile:contents:" asSymbol] on: receiver as: [:self :name :data | answer]];
  [System gc];
  [System println: [receiver readFile: path]];
  [System println: [[[receiver writeFile: path contents: "ignored"] == answer] printString]];
  [System println: [System readFile: path]];
  [System preciseGc: false];
  [System autoGc: true];
  [System println: "file-io-image-ok"]
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
printf 'restored' >"$scratch/input"
rm "$scratch/output"
printf 'true\ntrue\ntrue\ntrue\ntrue\nrestored\ntrue\n#ioError\ncould not read file\ncould not write file\ntrue\ntrue\nreplacement\ntrue\nrestored\nfile-io-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
cmp "$scratch/original" "$scratch/output"
test ! -s "$scratch/empty"
grep -Fq '/libbirnam_file_io.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
printf 'baked' >"$scratch/input"
rm "$scratch/output"
printf 'true\ntrue\ntrue\ntrue\ntrue\nbaked\ntrue\n#ioError\ncould not read file\ncould not write file\ntrue\ntrue\nreplacement\ntrue\nbaked\nfile-io-image-ok\n' >"$scratch/baked-expected.out"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/baked-expected.out" "$scratch/baked.out"
cmp "$scratch/original" "$scratch/output"
grep -Fq '/libbirnam_file_io.dylib' "$scratch/baked.err"
printf 'file-io-image-smoke: dynamic startup, source-free compiled module, native Code image and baked executable preserve binary IO, errors, aliases, retained bytes, live file lookup, precise GC and method/factory replacement\n'
