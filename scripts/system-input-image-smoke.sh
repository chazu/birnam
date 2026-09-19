#!/usr/bin/env bash
# Focused debug startup, compiled artifact, saved-image and baked-image input check.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: system-input-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-system-input-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'system input image smoke root: %s\n' "$scratch"
fi
python3 - "$scratch" <<'PYDATA'
from pathlib import Path
import sys
p=Path(sys.argv[1])
p.joinpath('input').write_bytes(b'first\x00\xff\r\n\nlast')
p.joinpath('save-input').write_text('retained café\n')
p.joinpath('startup-expected.out').write_bytes(b'first\x00\xff\r\n\nlast\ntrue\nsystem-input-startup-ok\n')
p.joinpath('expected.out').write_bytes('true\ntrue\nretained café\n'.encode()+b'first\x00\xff\r\n\ntrue\ntrue\nreplacement\ntrue\nsystem-input-image-ok\n')
p.joinpath('error-expected.out').write_text('true\ntrue\nretained café\n#ioError\nstdin read failed\ntrue\ntrue\nreplacement\ntrue\nsystem-input-image-ok\n')
PYDATA
cat >"$scratch/probe.birnam" <<'BIRNAM'
[System println: [System readLine]];
[System println: [System readLine]];
[System println: [System readLine]];
[System println: [[[System readLine] == nil] printString]];
[System println: "system-input-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" <"$scratch/input" >"$scratch/startup.out" 2>"$scratch/startup.err"
cmp "$scratch/startup-expected.out" "$scratch/startup.out"
grep -Fq '/libbirnam_system_input.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" <"$scratch/input" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"

cat >"$scratch/save.birnam" <<BIRNAM
let receiver := [System clone];
let alias := receiver;
let payload := [System readLine];
let payloadAlias := payload;
let answer := [Object clone];
let main := [|
  [System autoGc: false];
  [System preciseGc: true];
  [System gc];
  [System println: [[receiver == alias] printString]];
  [System println: [[payload == payloadAlias] printString]];
  [System println: payload];
  let first := [receiver readLine];
  [[first isError] ifTrue: [|
    [System println: [first kind]];
    [System println: [first message]];
    [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | answer]];
    [System println: [[[receiver readLine] == answer] printString]];
    [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | nil]];
    [System println: [[[receiver readLine] == nil] printString]]
  ] ifFalse: [|
    [System println: first];
    [System println: [receiver readLine]];
    [System println: [[[receiver readLine] = "last"] printString]];
    [System println: [[[receiver readLine] == nil] printString]]
  ]];
  [Reflect defineMethod: #readLine on: receiver as: [:self | "replacement"]];
  [System gc];
  [System println: [receiver readLine]];
  [System println: [[[System readLine] == nil] printString]];
  [System preciseGc: false];
  [System autoGc: true];
  [System println: "system-input-image-ok"]
];
[System saveImage: "$scratch/app.image"]
BIRNAM
"$bin" "$scratch/save.birnam" <"$scratch/save-input" >"$scratch/save.out" 2>"$scratch/save.err"
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
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" <"$scratch/input" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_system_input.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" <"$scratch/input" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_system_input.dylib' "$scratch/baked.err"
python3 - "$bin" "$scratch" "$prefix" <<'PYERROR'
from pathlib import Path
import os,subprocess,sys
binary,root,prefix=sys.argv[1:];p=Path(root)
for name,args in [('image-error',[binary,str(p/'app.image')]),('baked-error',[str(p/'app')])]:
    fd=os.open('/',os.O_RDONLY)
    try:
        with p.joinpath(name+'.out').open('wb') as out,p.joinpath(name+'.err').open('wb') as err:
            subprocess.run(args,stdin=fd,stdout=out,stderr=err,check=True,env=dict(os.environ,BIRNAM_LIB_DIR=str(Path(prefix)/'lib'),DYLD_PRINT_LIBRARIES='1'))
    finally:
        os.close(fd)
    assert p.joinpath(name+'.out').read_bytes()==p.joinpath('error-expected.out').read_bytes()
    assert '/libbirnam_system_input.dylib' in p.joinpath(name+'.err').read_text()
PYERROR
printf 'system-input-image-smoke: dynamic startup, source-free compiled module, native Code image and baked executable preserve binary lines, EOF, errors, retained input, aliases, precise GC and live method/factory replacement\n'
