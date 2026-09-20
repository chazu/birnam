#!/usr/bin/env bash
# Source-free callers deliberately load separate source fixtures at runtime.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: source-loader-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-source-loader-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'source loader image smoke root: %s\n' "$scratch"
fi
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -Iabi/fixtures/idapi-v16 \
  -DEXPECT_SOURCE_LOADER_LIBRARY='"libbirnam_source_loader.dylib"' \
  scripts/fixtures/source-loader-native.c -o "$scratch/native-guard.dylib"
write_target() {
  cat >"$scratch/input.birnam" <<BIRNAM
[Module named: #sourceLoaderProbe requires: #[]];
[Globals at: #SourceLoaderSeen put: $1];
$1
BIRNAM
}
write_target 41
printf '[Error kind: #sourceProbe message: "loaded fault"]\n' >"$scratch/fault.birnam"
cat >"$scratch/probe.birnam" <<BIRNAM
[System loadModule: "$scratch/native-guard.dylib"];
[System println: [[System load: "$scratch/input.birnam"] printString]];
[System println: [[Module isRegistered: #sourceLoaderProbe] printString]];
[System println: [[Module isLoaded: #sourceLoaderProbe] printString]];
[System println: [[Globals at: #SourceLoaderSeen] printString]];
[System println: [[System load: "$scratch/missing.birnam"] kind]];
[System println: [[System load: "$scratch/fault.birnam"] kind]];
[System println: "source-loader-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
printf '41\ntrue\nfalse\n41\n#ioError\n#sourceProbe\nsource-loader-startup-ok\n' >"$scratch/startup-expected.out"
cmp "$scratch/startup-expected.out" "$scratch/startup.out"
grep -Fq '/libbirnam_source_loader.dylib' "$scratch/startup.err"
grep -Fq "birnam: load $scratch/fault.birnam:" "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"
grep -Fq "birnam: load $scratch/fault.birnam:" "$scratch/compiled.err"
cat >"$scratch/save.birnam" <<BIRNAM
[System loadModule: "$scratch/native-guard.dylib"];
let receiver := [System clone];
let alias := receiver;
let path := "$scratch/input.birnam";
let pathAlias := path;
let retained := [receiver load: path];
let retainedAlias := retained;
let answer := [Object clone];
let main := [|
  [System loadModule: "$scratch/native-guard.dylib"];
  [System autoGc: false];
  [System preciseGc: true];
  [System gc];
  [System println: [[receiver == alias] printString]];
  [System println: [[path == pathAlias] printString]];
  [System println: [[retained == retainedAlias] printString]];
  [System println: [retained printString]];
  [System println: [[receiver load: path] printString]];
  [System println: [[Module isRegistered: #sourceLoaderProbe] printString]];
[System println: [[Module isLoaded: #sourceLoaderProbe] printString]];
  [System println: [[Globals at: #SourceLoaderSeen] printString]];
  [System println: [[receiver load: "$scratch/missing.birnam"] kind]];
  [System println: [[receiver load: "$scratch/fault.birnam"] kind]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | answer]];
  [System println: [[[receiver load: "$scratch/missing.birnam"] == answer] printString]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | nil]];
  [System println: [[[receiver load: "$scratch/missing.birnam"] == nil] printString]];
  [Reflect defineMethod: #load: on: receiver as: [:self :name | answer]];
  [System gc];
  [System println: [[[receiver load: path] == answer] printString]];
  [System println: [[System load: path] printString]];
  [System preciseGc: false];
  [System autoGc: true];
  [System println: "source-loader-image-ok"]
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
write_target 42
printf 'true\ntrue\ntrue\n41\n42\ntrue\nfalse\n42\n#ioError\n#sourceProbe\ntrue\ntrue\ntrue\n42\nsource-loader-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_source_loader.dylib' "$scratch/image.err"
grep -Fq "birnam: load $scratch/fault.birnam:" "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
write_target 43
printf 'true\ntrue\ntrue\n41\n43\ntrue\nfalse\n43\n#ioError\n#sourceProbe\ntrue\ntrue\ntrue\n43\nsource-loader-image-ok\n' >"$scratch/baked-expected.out"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/baked-expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_source_loader.dylib' "$scratch/baked.err"
grep -Fq "birnam: load $scratch/fault.birnam:" "$scratch/baked.err"
printf 'source-loader-image-smoke: native owner, live source reading, declared module staging, evaluation, errors, retained values, aliases, GC and method/factory replacement pass in startup, source-free compiled caller, v8 native Code image and baked executable\n'
