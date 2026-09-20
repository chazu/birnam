#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: namespace-enumeration-image-smoke.sh PREFIX [BASELINE_PREFIX]}
baseline=${2:-}
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-enumeration-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then trap 'rm -rf "$scratch"' EXIT; else printf 'enumeration image root: %s\n' "$scratch"; fi
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -Iabi/fixtures/idapi-v11 scripts/fixtures/namespace-enumeration-native.c -o "$scratch/probe.dylib"
cat >"$scratch/probe.birnam" <<BIRNAM
[System loadModule: "$scratch/probe.dylib"];
[System println: [[System namespaceEnumerationProbe] printString]]
BIRNAM
"$prefix/bin/birnam" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
test ! -s "$scratch/startup.err"
grep -Fxq true "$scratch/startup.out"
if [[ -n $baseline ]]; then
  cc -std=c11 -Wall -Wextra -Werror -DENUMERATION_V1=0 -dynamiclib -Iabi/fixtures/idapi-v11 scripts/fixtures/namespace-enumeration-native.c -o "$scratch/old.dylib"
  sed "s,probe.dylib,old.dylib," "$scratch/probe.birnam" >"$scratch/old.birnam"
  "$baseline/bin/birnam" "$scratch/old.birnam" >"$scratch/baseline.out" 2>"$scratch/baseline.err"
  test ! -s "$scratch/baseline.err"
  cmp "$scratch/baseline.out" "$scratch/startup.out"
fi
"$prefix/bin/birnam" --compile-module "$scratch/probe.birnam" "$scratch/caller.dylib"
rm "$scratch/probe.birnam"
"$prefix/bin/birnam" "$scratch/caller.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"
cat >"$scratch/save.birnam" <<BIRNAM
let retained := [Reflect allGlobalNames];
let alias := retained;
let main := [|
  [System preciseGc: true];
  [System gc];
  [System println: [[retained == alias] printString]];
  [System println: [[[retained size] > 0] printString]];
  [System loadModule: "$scratch/probe.dylib"];
  [System println: [[System namespaceEnumerationProbe] printString]]
];
[System saveImage: "$scratch/application.image"]
BIRNAM
"$prefix/bin/birnam" "$scratch/save.birnam" >"$scratch/save.out" 2>"$scratch/save.err"
test ! -s "$scratch/save.err"
python3 - "$scratch/application.image" <<'PY'
from pathlib import Path
import struct,sys
b=Path(sys.argv[1]).read_bytes();assert b[:8]==b'BIRNAMIM' and struct.unpack_from('<I',b,8)[0]==8
offset=28;records=[]
while offset<len(b):
 kind,length=struct.unpack_from('<IQ',b,offset);offset+=12+length;records.append((kind,length));assert offset<=len(b)
assert offset==len(b) and records[-1][0]==4 and records[-1][1]>72
PY
rm "$scratch/save.birnam"
"$prefix/bin/birnam" "$scratch/application.image" >"$scratch/image.out" 2>"$scratch/image.err"
test ! -s "$scratch/image.err"
test "$(grep -Fxc true "$scratch/image.out")" == 3
"$prefix/bin/birnam" --bake "$scratch/application.image" "$scratch/application"
BIRNAM_LIB_DIR="$prefix/lib" "$scratch/application" >"$scratch/baked.out" 2>"$scratch/baked.err"
test ! -s "$scratch/baked.err"
cmp "$scratch/image.out" "$scratch/baked.out"
printf 'namespace-enumeration: Namespace function ownership, live selected-floor discovery, native callback parity, missing/malformed entries, retained snapshots, null filtering and reentry pass in startup, source-free compiled caller, v8 Code image and baked execution\n'
