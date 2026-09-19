#!/usr/bin/env bash
# Focused debug startup, compiled artifact, saved-image and baked-image Error policy check.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: error-value-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-error-value-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'error value image smoke root: %s\n' "$scratch"
fi
cat >"$scratch/probe.birnam" <<BIRNAM
let error := [System readFile: "$scratch/missing"];
[System println: [error isError]];
[System println: [error kind]];
[System println: [error message]];
[System println: "error-value-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
printf 'true\n#ioError\ncould not read file\nerror-value-startup-ok\n' >"$scratch/startup-expected.out"
cmp "$scratch/startup-expected.out" "$scratch/startup.out"
grep -Fxq 'error-value-startup-ok' "$scratch/startup.out"
grep -Fq '/libbirnam_error_value.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"

cat >"$scratch/save.birnam" <<BIRNAM
let path := "$scratch/missing";
let sourcePath := "$scratch/source-data";
let retained := [Error kind: #savedError message: "alive"];
let alias := retained;
let main := [|
  [System autoGc: false];
  [System preciseGc: true];
  [System gc];
  [System println: [[retained == alias] printString]];
  [System println: [retained message]];
  let error := [System readFile: path];
  [System println: [error kind]];
  [System println: [error message]];
  [System writeFile: sourcePath contents: "nil"];
  [Reflect defineMethod: ["evalPersisting:from:" asSymbol] on: Codegen as: [:self :source :file | retained]];
  [System println: [[[System load: sourcePath] == retained] printString]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | retained]];
  [System println: [[[System readFile: path] == retained] printString]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | "replacement"]];
  [System gc];
  [System println: [System readFile: path]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | nil]];
  [System println: [[[System readFile: path] == nil] printString]];
  [System preciseGc: false];
  [System autoGc: true];
  [System println: "error-value-image-ok"]
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
printf 'true\nalive\n#ioError\ncould not read file\ntrue\ntrue\nreplacement\ntrue\nerror-value-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq "birnam: load $scratch/source-data: error(#savedError: alive)" "$scratch/image.err"
grep -Fq '/libbirnam_error_value.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq "birnam: load $scratch/source-data: error(#savedError: alive)" "$scratch/baked.err"
grep -Fq '/libbirnam_error_value.dylib' "$scratch/baked.err"
printf 'error-value-image-smoke: dynamic startup, source-free compiled module, native Code image and baked executable preserve lazy Error construction, exact recognition, aliases, precise GC and fresh factory replacement\n'
