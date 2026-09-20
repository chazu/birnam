#!/usr/bin/env bash
# Real direct native loading through startup, compiled code and saved/baked images.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: native-loader-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-native-loader-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'native loader image smoke root: %s\n' "$scratch"
fi
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -Iabi/fixtures/idapi-v20 \
  -DEXPECT_NATIVE_LOADER_LIBRARY='"libbirnam_native_loader.dylib"' \
  scripts/fixtures/native-loader-target.c -o "$scratch/target.dylib"
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -Iabi/fixtures/idapi-v20 \
  -DNATIVE_LOADER_NO_INIT scripts/fixtures/native-loader-target.c -o "$scratch/not-module.dylib"
cat >"$scratch/probe.birnam" <<BIRNAM
[System println: [[[System loadModule: "$scratch/target.dylib"] == System] printString]];
[System println: [System nativeLoaderCount]];
[System loadModule: "$scratch/target.dylib"];
[System println: [System nativeLoaderCount]];
[System println: [[System loadModule: "$scratch/missing.dylib"] kind]];
[System println: [[System loadModule: "$scratch/not-module.dylib"] kind]];
[System println: "native-loader-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
printf 'true\n#native-load-1\n#native-load-2\n#ioError\n#notAModule\nnative-loader-startup-ok\n' >"$scratch/startup-expected.out"
cmp "$scratch/startup-expected.out" "$scratch/startup.out"
grep -Fq '/libbirnam_native_loader.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"
cat >"$scratch/save.birnam" <<BIRNAM
let receiver := [System clone];
let alias := receiver;
let path := "$scratch/target.dylib";
let pathAlias := path;
let answer := [Object clone];
let main := [|
  [System autoGc: false];
  [System preciseGc: true];
  [System gc];
  [System println: [[receiver == alias] printString]];
  [System println: [[path == pathAlias] printString]];
  [System println: [[[receiver loadModule: path] == alias] printString]];
  [System println: [System nativeLoaderCount]];
  [receiver loadModule: path];
  [System println: [System nativeLoaderCount]];
  [System println: [[receiver loadModule: "$scratch/missing.dylib"] kind]];
  [System println: [[receiver loadModule: "$scratch/missing.dylib"] message]];
  [System println: [[receiver loadModule: "$scratch/not-module.dylib"] kind]];
  [System println: [[receiver loadModule: "$scratch/not-module.dylib"] message]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | answer]];
  [System println: [[[receiver loadModule: "$scratch/missing.dylib"] == answer] printString]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | nil]];
  [System println: [[[receiver loadModule: "$scratch/not-module.dylib"] == nil] printString]];
  [Reflect defineMethod: #loadModule: on: receiver as: [:self :name | answer]];
  [System gc];
  [System println: [[[receiver loadModule: path] == answer] printString]];
  [System println: [System nativeLoaderCount]];
  [System loadModule: path];
  [System println: [System nativeLoaderCount]];
  [System preciseGc: false];
  [System autoGc: true];
  [System println: "native-loader-image-ok"]
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
printf 'true\ntrue\ntrue\n#native-load-1\n#native-load-2\n#ioError\ncould not load module dylib\n#notAModule\ndylib has no module_init\ntrue\ntrue\ntrue\n#native-load-2\n#native-load-3\nnative-loader-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_native_loader.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_native_loader.dylib' "$scratch/baked.err"
printf 'native-loader-image-smoke: native owner and direct repeated initialization verified in startup, source-free compiled module, v8 native Code image and baked executable; state, receiver, aliases, errors, GC and method/factory replacement preserved\n'
