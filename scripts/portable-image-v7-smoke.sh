#!/usr/bin/env bash
# Prove source-less nested portable meaning and current CodeBindings survive a
# clean v7 save, process exit, ordinary/rebuild boot, and resave.
set -euo pipefail

cd "$(dirname "$0")/.."

prefix=${1:?usage: portable-image-v7-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
test -x "$bin"

smoke_root=$(mktemp -d "${TMPDIR:-/tmp}/birnam-portable-image-v7.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$smoke_root"' EXIT
else
  printf 'portable image-v7 smoke root: %s\n' "$smoke_root" >&2
fi

image="$smoke_root/portable.image"
resaved="$smoke_root/resaved.image"

"$bin" --repl >"$smoke_root/save.out" 2>"$smoke_root/save.err" <<EOF
let nestedRead := [CodeNode lexicalReadAt: 0]
let nestedStatement := [CodeStatement expression: nestedRead]
let nestedProgram := [CodeProgram parameters: #[] captures: #["held"] statements: #[nestedStatement]]
let nestedNode := [CodeNode block: nestedProgram captureBindings: #[0]]
let outerStatement := [CodeStatement expression: nestedNode]
let outerProgram := [CodeProgram parameters: #["held"] captures: #[] statements: #[outerStatement]]
let outerObject := [CodeObject fromProgram: outerProgram]
[db assert: #portableV7Nested attr: #closure value: [Codegen materializeResumable: outerObject captures: #[]]]
[Globals at: #PortableV7Target put: [Object clone]]
[Reflect defineMethod: #answer: on: [Globals at: #PortableV7Target] as: [:self :value | [value + 2]]]
[[Globals at: #PortableV7Target] answer: 40]
[System saveImage: "$image"]
[[System imageSaveReport] status]
:quit
EOF

test ! -s "$smoke_root/save.err"
grep -Fxq 'birnam> 42' "$smoke_root/save.out"
grep -Fxq 'birnam> #saved' "$smoke_root/save.out"
python3 - "$image" <<'PY'
import struct
import sys
with open(sys.argv[1], "rb") as handle:
    data = handle.read()
header = data[:12]
assert header[:8] == b"BIRNAMIM", header
assert struct.unpack("<I", header[8:])[0] == 7, header
offset = 28
records = []
while offset < len(data):
    kind, length = struct.unpack_from("<IQ", data, offset)
    offset += 12
    end = offset + length
    assert end <= len(data), (kind, length, len(data) - offset)
    records.append((kind, length))
    offset = end
assert offset == len(data), (offset, len(data))
assert records[-1][0] == 4 and records[-1][1] > 72, records
PY

cat >"$smoke_root/inspect.in" <<'EOF'
let loadedOuter := [db currentValue: #portableV7Nested attr: #closure]
let loadedNested := [loadedOuter value: 52]
[loadedNested value]
[[Globals at: #PortableV7Target] answer: 41]
[Codegen currentCodeBindingCount]
:quit
EOF

for mode in ordinary rebuild; do
  arguments=()
  if [[ $mode == rebuild ]]; then
    arguments+=(--rebuild)
  fi
  "$bin" "${arguments[@]}" "$image" <"$smoke_root/inspect.in" \
    >"$smoke_root/$mode.out" 2>"$smoke_root/$mode.err"
  test ! -s "$smoke_root/$mode.err"
  grep -Fxq 'birnam> 52' "$smoke_root/$mode.out"
  grep -Fxq 'birnam> 43' "$smoke_root/$mode.out"
  grep -Fxq 'birnam> 1' "$smoke_root/$mode.out"
done

printf '[System saveImage: "%s"]\n:quit\n' "$resaved" \
  | "$bin" "$image" >"$smoke_root/resave.out" 2>"$smoke_root/resave.err"
test ! -s "$smoke_root/resave.err"
test -s "$resaved"
printf '[[Globals at: #PortableV7Target] answer: 42]\n:quit\n' \
  | "$bin" "$resaved" >"$smoke_root/resaved.out" 2>"$smoke_root/resaved.err"
test ! -s "$smoke_root/resaved.err"
grep -Fxq 'birnam> 44' "$smoke_root/resaved.out"

printf 'portable-image-v7-smoke: nested source-less meaning, CodeBinding recovery, ordinary/rebuild boot, and resave passed\n'
