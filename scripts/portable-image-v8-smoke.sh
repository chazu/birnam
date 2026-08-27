#!/usr/bin/env bash
# Prove source-less nested portable meaning and current CodeBindings survive a
# clean v8 save, process exit, ordinary/rebuild boot, and resave.
set -euo pipefail

cd "$(dirname "$0")/.."

prefix=${1:?usage: portable-image-v8-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
test -x "$bin"

smoke_root=$(mktemp -d "${TMPDIR:-/tmp}/birnam-portable-image-v8.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$smoke_root"' EXIT
else
  printf 'portable image-v8 smoke root: %s\n' "$smoke_root" >&2
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
[db assert: #portableV8Nested attr: #closure value: [Codegen materializeResumable: outerObject captures: #[]]]
[Globals at: #PortableV8Target put: [Object clone]]
[Reflect defineMethod: #answer: on: [Globals at: #PortableV8Target] as: [:self :value | [value + 2]]]
[[Globals at: #PortableV8Target] answer: 40]
[Reflect defineMethod: #imageV8Prototype on: Integer as: [:self | #integerPrototype]]
[Reflect defineMethod: #imageV8Prototype on: Float as: [:self | #floatPrototype]]
[Reflect defineMethod: #imageV8Prototype on: Word as: [:self | #wordPrototype]]
[Reflect defineMethod: #imageV8Prototype on: String as: [:self | #stringPrototype]]
let specialBox := ["special" , ""]
[Reflect defineMethod: #imageV8Particular on: specialBox as: [:self | #particularBox]]
[db assert: #portableV8Boxes attr: #special value: specialBox]
[db assert: #portableV8Boxes attr: #shared value: specialBox]
[db assert: #portableV8Boxes attr: #equalA value: ["equal" , ""]]
[db assert: #portableV8Boxes attr: #equalB value: ["equal" , ""]]
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
assert struct.unpack("<I", header[8:])[0] == 8, header
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
let loadedOuter := [db currentValue: #portableV8Nested attr: #closure]
let loadedNested := [loadedOuter value: 52]
[loadedNested value]
[[Globals at: #PortableV8Target] answer: 41]
[Codegen currentCodeBindingCount]
[Integer imageV8Prototype]
[Float imageV8Prototype]
[Word imageV8Prototype]
[String imageV8Prototype]
[[db currentValue: #portableV8Boxes attr: #special] imageV8Particular]
[[db currentValue: #portableV8Boxes attr: #special] == [db currentValue: #portableV8Boxes attr: #shared]]
[[db currentValue: #portableV8Boxes attr: #equalA] == [db currentValue: #portableV8Boxes attr: #equalB]]
[[db currentValue: #portableV8Boxes attr: #equalA] = [db currentValue: #portableV8Boxes attr: #equalB]]
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
  grep -Eq '^birnam> [1-9][0-9]*$' "$smoke_root/$mode.out"
  grep -Fxq 'birnam> #integerPrototype' "$smoke_root/$mode.out"
  grep -Fxq 'birnam> #floatPrototype' "$smoke_root/$mode.out"
  grep -Fxq 'birnam> #wordPrototype' "$smoke_root/$mode.out"
  grep -Fxq 'birnam> #stringPrototype' "$smoke_root/$mode.out"
  grep -Fxq 'birnam> #particularBox' "$smoke_root/$mode.out"
  test "$(grep -Fxc 'birnam> true' "$smoke_root/$mode.out")" -eq 2
  grep -Fxq 'birnam> false' "$smoke_root/$mode.out"
done

printf '[System saveImage: "%s"]\n:quit\n' "$resaved" \
  | "$bin" "$image" >"$smoke_root/resave.out" 2>"$smoke_root/resave.err"
test ! -s "$smoke_root/resave.err"
test -s "$resaved"
printf '[[Globals at: #PortableV8Target] answer: 42]\n:quit\n' \
  | "$bin" "$resaved" >"$smoke_root/resaved.out" 2>"$smoke_root/resaved.err"
test ! -s "$smoke_root/resaved.err"
grep -Fxq 'birnam> 44' "$smoke_root/resaved.out"

app_image="$smoke_root/app.image"
cat >"$smoke_root/app.birnam" <<EOF
let banner := "portable image-v8 app";
let main := [|
  [System println: banner];
  [System println: "main entry ran"]
];
[System saveImage: "$app_image"]
EOF
"$bin" "$smoke_root/app.birnam" >"$smoke_root/app-save.out" 2>"$smoke_root/app-save.err"
test ! -s "$smoke_root/app-save.err"
test -s "$app_image"

for mode in ordinary rebuild; do
  arguments=()
  if [[ $mode == rebuild ]]; then
    arguments+=(--rebuild)
  fi
  "$bin" "${arguments[@]}" "$app_image" \
    >"$smoke_root/app-$mode.out" 2>"$smoke_root/app-$mode.err"
  test ! -s "$smoke_root/app-$mode.err"
  grep -Fxq 'portable image-v8 app' "$smoke_root/app-$mode.out"
  grep -Fxq 'main entry ran' "$smoke_root/app-$mode.out"
  ! grep -q 'interactive image' "$smoke_root/app-$mode.out"
done

baked_app="$smoke_root/baked-app"
"$bin" --bake "$app_image" "$baked_app"
test -x "$baked_app"
BIRNAM_LIB_DIR="$prefix/lib" "$baked_app" \
  >"$smoke_root/app-baked.out" 2>"$smoke_root/app-baked.err"
test ! -s "$smoke_root/app-baked.err"
grep -Fxq 'portable image-v8 app' "$smoke_root/app-baked.out"
grep -Fxq 'main entry ran' "$smoke_root/app-baked.out"
! grep -q 'interactive image' "$smoke_root/app-baked.out"

printf 'portable-image-v8-smoke: boxed identity, nested source-less meaning, CodeBinding recovery, ordinary/rebuild entry, baked entry, and resave passed\n'
