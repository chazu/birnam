#!/usr/bin/env bash
# Focused debug startup, compiled artifact, saved-image and baked-image Error policy check.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: word-error-value-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-word-error-value-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'word error value image smoke root: %s\n' "$scratch"
fi
export BIRNAM_WORD_ERROR_DYLIB="$prefix/lib/libbirnam_word.dylib"
cc -dynamiclib -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v7 -Iabi/fixtures/word-v1 scripts/fixtures/word-error-values.c -o "$scratch/values.dylib"
cat >"$scratch/probe.birnam" <<BIRNAM
[System loadModule: "$scratch/values.dylib"];
[System println: [OverflowWord printString]];
let error := [OverflowWord asInteger];
[System println: [error kind]];
[System println: [error message]];
[System println: [[BoundaryWord asInteger] printString]];
[System println: "word-error-value-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
printf '0xffffffffffffffff\n#overflow\nword out of Integer range\n9223372036854775807\nword-error-value-startup-ok\n' >"$scratch/startup-expected.out"
cmp "$scratch/startup-expected.out" "$scratch/startup.out"
grep -Fxq 'word-error-value-startup-ok' "$scratch/startup.out"
grep -Fq '/libbirnam_error_value.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"

cat >"$scratch/save.birnam" <<BIRNAM
[System loadModule: "$scratch/values.dylib"];
let retained := [OverflowWord clone];
let alias := retained;
let boundary := [BoundaryWord clone];
let answer := [Object clone];
let main := [|
  [System autoGc: false];
  [System preciseGc: true];
  [System gc];
  [System println: [[retained == alias] printString]];
  [System println: [retained printString]];
  let error := [retained asInteger];
  [System println: [error kind]];
  [System println: [error message]];
  [System println: [[boundary asInteger] printString]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | answer]];
  [System println: [[[retained asInteger] == answer] printString]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | "replacement"]];
  [System gc];
  [System println: [retained asInteger]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | nil]];
  [System println: [[[retained asInteger] == nil] printString]];
  [System println: [[boundary asInteger] printString]];
  [System preciseGc: false];
  [System autoGc: true];
  [System println: "word-error-value-image-ok"]
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
rm "$scratch/save.birnam" "$scratch/values.dylib"
printf 'true\n0xffffffffffffffff\n#overflow\nword out of Integer range\n9223372036854775807\ntrue\nreplacement\ntrue\n9223372036854775807\nword-error-value-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_error_value.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_error_value.dylib' "$scratch/baked.err"
printf 'word-error-value-image-smoke: dynamic startup, source-free compiled module, native Code image and baked executable preserve Word overflow, boundary conversion, aliases, precise GC and fresh Error factory replacement\n'
