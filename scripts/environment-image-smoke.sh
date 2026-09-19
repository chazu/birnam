#!/usr/bin/env bash
# Focused debug startup, compiled artifact, saved-image and baked-image environment check.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: environment-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-environment-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'environment image smoke root: %s\n' "$scratch"
fi
# Only these isolated test variables are read or changed by this fixture.
export BIRNAM_ENV_SMOKE_VALUE="startup café"
export BIRNAM_ENV_SMOKE_EMPTY=""
export BIRNAM_ENV_SMOKE_BAD=$'\xff'
unset BIRNAM_ENV_SMOKE_MISSING
cat >"$scratch/probe.birnam" <<'BIRNAM'
[System println: [System getEnv: "BIRNAM_ENV_SMOKE_VALUE"]];
[System println: [[[System getEnv: "BIRNAM_ENV_SMOKE_EMPTY"] = ""] printString]];
[System println: [[[System getEnv: "BIRNAM_ENV_SMOKE_MISSING"] == nil] printString]];
[System println: [[System getEnv: "BIRNAM_ENV_SMOKE_BAD"] kind]];
[System println: "environment-startup-ok"]
BIRNAM
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/probe.birnam" >"$scratch/startup.out" 2>"$scratch/startup.err"
printf 'startup café\ntrue\ntrue\n#ioError\nenvironment-startup-ok\n' >"$scratch/startup-expected.out"
cmp "$scratch/startup-expected.out" "$scratch/startup.out"
grep -Fq '/libbirnam_environment.dylib' "$scratch/startup.err"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/startup.out" "$scratch/compiled.out"

export BIRNAM_ENV_SMOKE_VALUE="saved café"
cat >"$scratch/save.birnam" <<BIRNAM
let receiver := [System clone];
let alias := receiver;
let name := "BIRNAM_ENV_SMOKE_VALUE";
let nameAlias := name;
let payload := [System getEnv: name];
let payloadAlias := payload;
let answer := [Object clone];
let main := [|
  [System autoGc: false];
  [System preciseGc: true];
  [System gc];
  [System println: [[receiver == alias] printString]];
  [System println: [[name == nameAlias] printString]];
  [System println: [[payload == payloadAlias] printString]];
  [System println: payload];
  [System println: [receiver getEnv: name]];
  [System println: [[[receiver getEnv: "BIRNAM_ENV_SMOKE_EMPTY"] = ""] printString]];
  [System println: [[[receiver getEnv: "BIRNAM_ENV_SMOKE_MISSING"] == nil] printString]];
  [System println: [[receiver getEnv: "BIRNAM_ENV_SMOKE_BAD"] kind]];
  [System println: [[receiver getEnv: "BIRNAM_ENV_SMOKE_BAD"] message]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | answer]];
  [System println: [[[receiver getEnv: "BIRNAM_ENV_SMOKE_BAD"] == answer] printString]];
  [Reflect defineMethod: ["kind:message:" asSymbol] on: Error as: [:self :kind :message | nil]];
  [System println: [[[receiver getEnv: "BIRNAM_ENV_SMOKE_BAD"] == nil] printString]];
  [Reflect defineMethod: #getEnv: on: receiver as: [:self :key | "replacement"]];
  [System gc];
  [System println: [receiver getEnv: name]];
  [System println: [System getEnv: name]];
  [System preciseGc: false];
  [System autoGc: true];
  [System println: "environment-image-ok"]
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
export BIRNAM_ENV_SMOKE_VALUE="restored café"
printf 'true\ntrue\ntrue\nsaved café\nrestored café\ntrue\ntrue\n#ioError\ncould not read environment\ntrue\ntrue\nreplacement\nrestored café\nenvironment-image-ok\n' >"$scratch/expected.out"
DYLD_PRINT_LIBRARIES=1 "$bin" "$scratch/app.image" >"$scratch/image.out" 2>"$scratch/image.err"
cmp "$scratch/expected.out" "$scratch/image.out"
grep -Fq '/libbirnam_environment.dylib' "$scratch/image.err"
"$bin" --bake "$scratch/app.image" "$scratch/app"
export BIRNAM_ENV_SMOKE_VALUE="baked café"
printf 'true\ntrue\ntrue\nsaved café\nbaked café\ntrue\ntrue\n#ioError\ncould not read environment\ntrue\ntrue\nreplacement\nbaked café\nenvironment-image-ok\n' >"$scratch/baked-expected.out"
BIRNAM_LIB_DIR="$prefix/lib" DYLD_PRINT_LIBRARIES=1 "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/baked-expected.out" "$scratch/baked.out"
grep -Fq '/libbirnam_environment.dylib' "$scratch/baked.err"
printf 'environment-image-smoke: dynamic startup, source-free compiled module, native Code image and baked executable preserve Unicode values, empty/missing results, errors, retained values, aliases, live process lookup, precise GC and method/factory replacement\n'
