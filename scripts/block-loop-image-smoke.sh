#!/usr/bin/env bash
# Exercise the native bootstrap loops explicitly; full startup replaces them
# with the authored stackless-control definitions, whose normal suite also runs.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: block-loop-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-block-loop-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'block loop image smoke root: %s\n' "$scratch"
fi
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -Iabi/fixtures/idapi-v14 \
  -DEXPECT_BLOCK_LOOP_LIBRARY='"libbirnam_block_loop.dylib"' \
  scripts/fixtures/block-loop-native.c -o "$scratch/native-guard.dylib"
cat >"$scratch/probe.birnam" <<BIRNAM
[System loadModule: "$prefix/lib/libbirnam_id_prelude.dylib"];
[System loadModule: "$scratch/native-guard.dylib"];
let n := [0 asCell];
let r := [[| [[n value] < 10000]] whileTrue: [| [n inc]]];
[System println: [[n value] printString]];
[System println: [[r == nil] printString]];
[[| [[n value] >= 10003]] whileFalse: [| [n inc]]];
[System println: [[n value] printString]];
[System println: [[[| 42] whileTrue: [| 0]] kind]];
[System println: "block-loop-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
printf '10000\ntrue\n10003\n#mustBeBoolean\nblock-loop-startup-ok\n' >"$scratch/startup-expected.out"
cmp "$scratch/startup-expected.out" "$scratch/startup.out"
grep -Fq '/libbirnam_block_loop.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"
cat >"$scratch/save.birnam" <<BIRNAM
let n := [0 asCell];
let alias := n;
let condition := [| [[n value] < 6]];
let conditionAlias := condition;
let body := [| [System gc]; [n inc]];
let bodyAlias := body;
let answer := [Object clone];
let fault := [Error kind: #loopProbe message: "retained"];
let main := [|
  [System loadModule: "$prefix/lib/libbirnam_id_prelude.dylib"];
  [System loadModule: "$scratch/native-guard.dylib"];
  [System autoGc: false];
  [System preciseGc: true];
  [System gc];
  [System println: [[n == alias] printString]];
  [System println: [[condition == conditionAlias] printString]];
  [System println: [[body == bodyAlias] printString]];
  [System println: [[[condition whileTrue: body] == nil] printString]];
  [System println: [[n value] printString]];
  [[| [[n value] >= 10]] whileFalse: body];
  [System println: [[n value] printString]];
  [[| false] whileTrue: [| [n set: 99]]];
  [[| true] whileFalse: [| [n set: 99]]];
  [System println: [[n value] printString]];
  [System println: [[[[| fault] whileTrue: body] == fault] printString]];
  [System println: [[[[| true] whileTrue: [| fault]] == fault] printString]];
  [System println: [[[| 42] whileFalse: body] kind]];
  [System println: [[[| 42] whileTrue: body] message]];
  [Reflect defineMethod: #value on: condition as: [:self | true]];
  [condition whileTrue: [| [Reflect defineMethod: #value on: condition as: [:self | false]]]];
  [System println: [[condition value] printString]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | answer]];
  [System println: [[[[| 42] whileTrue: body] == answer] printString]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | nil]];
  [System println: [[[[| 42] whileFalse: body] == nil] printString]];
  [Reflect defineMethod: #whileTrue: on: condition as: [:self :action | answer]];
  [System gc];
  [System println: [[[condition whileTrue: body] == answer] printString]];
  [System println: [[[[| false] whileTrue: body] == nil] printString]];
  [System preciseGc: false];
  [System autoGc: true];
  [System println: "block-loop-image-ok"]
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
printf 'true\ntrue\ntrue\ntrue\n6\n10\n10\ntrue\ntrue\n#mustBeBoolean\nwhile loop condition must answer true or false\nfalse\ntrue\ntrue\ntrue\ntrue\nblock-loop-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_block_loop.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_block_loop.dylib' "$scratch/baked.err"
printf 'block-loop-image-smoke: native owner verified in startup, source-free compiled module, v8 native Code image and baked executable; counts, aliases, captured cells/blocks, errors, GC and fresh method/factory replacement preserved\n'
