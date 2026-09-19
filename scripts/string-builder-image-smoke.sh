#!/usr/bin/env bash
# Focused debug startup, source-free compiled module and compiled-image checks.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: string-builder-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-string-builder-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'StringBuilder image smoke root: %s\n' "$scratch"
fi
cat >"$scratch/probe.birnam" <<'BIRNAM'
let builder := [StringBuilder new];
[1 to: 130 do: [:i | [builder add: "x"]]];
[builder addByte: 321];
[System gc];
[System println: [[builder size] printString]];
[System println: [[[builder asString] byteAt: 130] printString]];
[System println: [[[builder clone] size] printString]];
[System println: "string-builder-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
printf '131\n65\n0\nstring-builder-startup-ok\n' >"$scratch/startup-expected.out"
cmp "$scratch/startup-expected.out" "$scratch/startup.out"
grep -Fq '/libbirnam_string_builder.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"

cat >"$scratch/save.birnam" <<BIRNAM
let retained := [[[StringBuilder new] add: "seed"] asString];
let alias := retained;
let main := [|
  [System gc];
  [System println: retained];
  [System println: [[retained == alias] printString]];
  let builder := [StringBuilder new];
  [builder addAll: #[retained]];
  [builder addByte: 321];
  let detached := [builder asString];
  [1 to: 130 do: [:i | [builder add: "x"]]];
  [System gc];
  [System println: [[builder size] printString]];
  [System println: detached];
  [System println: [[[builder clone] size] printString]];
  [System println: [[[builder new] == builder] printString]];
  [System println: [[[StringBuilder new] addLine: "ok"] asString]];
  [System println: [#["a" "b" 3] joined]];
  [System println: ["birnam" asUppercase]];
  [System println: ["BIRNAM" asLowercase]];
  [Reflect defineMethod: #size on: StringBuilderVt as: [:self | 99]];
  [System gc];
  [System println: [[builder size] printString]];
  [System println: "string-builder-image-ok"]
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
printf 'seed\ntrue\n135\nseedA\n0\nfalse\nok\n\nab3\nBIRNAM\nbirnam\n99\nstring-builder-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_string_builder.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_string_builder.dylib' "$scratch/baked.err"

# Record the existing RAW capture limitation, without executing/restoring a
# nonportable pointer or changing Image's serialization/rejection policy.
cat >"$scratch/raw-capture.birnam" <<BIRNAM
let captured := [[StringBuilder new] add: "raw"];
let main := [| [System println: [[captured size] printString]]];
[System saveImage: "$scratch/raw-capture.image"]
BIRNAM
"$bin" "$scratch/raw-capture.birnam" >"$scratch/raw-capture.out" 2>"$scratch/raw-capture.err"
test ! -s "$scratch/raw-capture.err"
python3 - "$scratch/raw-capture.image" <<'PY'
import struct, sys
from pathlib import Path
data = Path(sys.argv[1]).read_bytes()
name = b'StringBuilderVt'
record = bytes([9]) + struct.pack('<I', len(name)) + name + struct.pack('<I', 3)
assert data.count(record) == 1
offset = data.index(record) + len(record)
words = []
for _ in range(3):
    assert data[offset] == 3
    words.append(struct.unpack_from('<Q', data, offset + 1)[0])
    offset += 9
assert words[0] != 0 and words[1:] == [3, 64], words
print('StringBuilder RAW capture remains three words with a native address; not executed')
PY
printf 'string-builder-image-smoke: startup, source-free module, native Code image and baked executable preserve buffers, copies, clone/new, stdlib clients, live dispatch and GC\n'
