#!/usr/bin/env bash
# Focused debug startup, compiled artifact, saved-image and baked-image Error policy check.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: float-error-value-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-float-error-value-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'float error value image smoke root: %s\n' "$scratch"
fi
cat >"$scratch/probe.birnam" <<BIRNAM
let incoming := [Error kind: #incoming message: "keep"];
[System println: [[[1.0 + incoming] == incoming] printString]];
[System println: [[1.0 / 0.0] kind]];
[System println: [[-1.0 sqrt] message]];
[System println: [[9223372036854775808.0 asInteger] kind]];
[System println: "float-error-value-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
printf 'true\n#zeroDivide\nsquare root of a negative number\n#overflow\nfloat-error-value-startup-ok\n' >"$scratch/startup-expected.out"
cmp "$scratch/startup-expected.out" "$scratch/startup.out"
grep -Fxq 'float-error-value-startup-ok' "$scratch/startup.out"
grep -Fq '/libbirnam_error_value.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"

cat >"$scratch/save.birnam" <<BIRNAM
let left := 2.5;
let alias := left;
let high := 9223372036854775808.0;
let negative := -1.0;
let retained := [Error kind: #incoming message: "keep"];
let answer := [Object clone];
let main := [|
  [System autoGc: false];
  [System preciseGc: true];
  [System gc];
  [System println: [[left == alias] printString]];
  [System println: [left + 1.5]];
  [System println: [[[left + retained] == retained] printString]];
  [System println: [[[left < retained] == retained] printString]];
  [System println: [[left = retained] printString]];
  [System println: [[left + true] kind]];
  [System println: [[left / 0.0] kind]];
  [System println: [[negative sqrt] kind]];
  [System println: [[high asInteger] kind]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | answer]];
  [System println: [[[left / 0.0] == answer] printString]];
  [System println: [[[high floor] == answer] printString]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | "replacement"]];
  [System gc];
  [System println: [left + true]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | nil]];
  [System println: [[[negative sqrt] == nil] printString]];
  [System println: [[[left / retained] == retained] printString]];
  [System preciseGc: false];
  [System autoGc: true];
  [System println: "float-error-value-image-ok"]
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
printf 'true\n4.0\ntrue\ntrue\nfalse\n#typeMismatch\n#zeroDivide\n#domain\n#overflow\ntrue\ntrue\nreplacement\ntrue\ntrue\nfloat-error-value-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_error_value.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_error_value.dylib' "$scratch/baked.err"
printf 'float-error-value-image-smoke: dynamic startup, source-free compiled module, native Code image and baked executable preserve Float error propagation, failure kinds, numeric output, aliases, precise GC and fresh Error factory replacement\n'
