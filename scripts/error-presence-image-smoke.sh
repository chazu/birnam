#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: error-presence-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-error-presence-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then trap 'rm -rf "$scratch"' EXIT; else printf 'error presence image root: %s\n' "$scratch"; fi
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -Iabi/fixtures/idapi-v11 scripts/fixtures/error-value-presence-native.c -o "$scratch/error-value-presence-native.dylib"
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -Iabi/fixtures/idapi-v11 scripts/fixtures/namespace-presence-reload-native.c -o "$scratch/namespace-presence-reload-native.dylib"
cat >"$scratch/probe.birnam" <<BIRNAM
[System loadModule: "$scratch/namespace-presence-reload-native.dylib"];
[System loadModule: "$scratch/error-value-presence-native.dylib"];
[System println: [[System errorValuePresenceProbe] printString]];
[System println: [[System errorValuePresenceProbe] printString]];
[System println: "error-presence-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
printf 'true\ntrue\nerror-presence-startup-ok\n' >"$scratch/startup-expected.out"
cmp "$scratch/startup-expected.out" "$scratch/startup.out"
grep -Fq '/libbirnam_error_value.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"
test ! -s "$scratch/compiled.err"
cat >"$scratch/save.birnam" <<BIRNAM
[System loadModule: "$scratch/namespace-presence-reload-native.dylib"];
[System loadModule: "$scratch/error-value-presence-native.dylib"];
let value := [Object clone];
let alias := value;
[System errorValuePresenceProbe];
let main := [|
  [System loadModule: "$scratch/namespace-presence-reload-native.dylib"];
[System loadModule: "$scratch/error-value-presence-native.dylib"];
  [System preciseGc: true];
  [System gc];
  [System println: [[value == alias] printString]];
  [System println: [[System errorValuePresenceProbe] printString]];
  [System loadModule: "$scratch/namespace-presence-reload-native.dylib"];
[System loadModule: "$scratch/error-value-presence-native.dylib"];
  [System println: [[System errorValuePresenceProbe] printString]];
  [System println: "error-presence-image-ok"]
];
[System saveImage: "$scratch/app.image"]
BIRNAM
"$bin" "$scratch/save.birnam" >"$scratch/save.out" 2>"$scratch/save.err"
test ! -s "$scratch/save.err"
python3 - "$scratch/app.image" <<'PY'
import struct,sys
with open(sys.argv[1],'rb') as f:data=f.read()
assert data[:8]==b'BIRNAMIM' and struct.unpack_from('<I',data,8)[0]==8
offset,records=28,[]
while offset<len(data):
 kind,length=struct.unpack_from('<IQ',data,offset);offset+=12+length
 assert offset<=len(data);records.append((kind,length))
assert offset==len(data) and records[-1][0]==4 and records[-1][1]>72,records
PY
rm "$scratch/save.birnam"
printf 'true\ntrue\ntrue\nerror-presence-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_error_value.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_error_value.dylib' "$scratch/baked.err"
printf 'error-presence-image-smoke: startup, source-free compiled caller, v8 native Code image and baked executable preserve explicit per-call presence, late callback/context changes, independent nested selection, null short circuit and saved-API reload presence\n'
