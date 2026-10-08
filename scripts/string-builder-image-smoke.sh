#!/usr/bin/env bash
# Source-free StringBuilder clients, copied Strings and native-buffer rejection.
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
grep -Fq '/libbirnam_string.dylib' "$scratch/startup.err"
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
  [System println: [[Reflect defineMethod: #size on: StringBuilderVt as: [:self | 99]] kind]];
  [System gc];
  [System println: [[builder size] printString]];
  [System println: "string-builder-image-ok"]
];
[System saveImage: "$scratch/app.image"];
[System println: [[System imageSaveReport] status]]
BIRNAM
"$bin" "$scratch/save.birnam" >"$scratch/save.out" 2>"$scratch/save.err"
test ! -s "$scratch/save.err"
grep -Fxq '#saved' "$scratch/save.out"
test -s "$scratch/app.image"
rm "$scratch/save.birnam"
printf 'seed\ntrue\n135\nseedA\n0\nfalse\nok\n\nab3\nBIRNAM\nbirnam\n#generationNotQuiescent\n135\nstring-builder-image-ok\n' >"$scratch/expected.out"
for mode in mapped rebuilt; do
  arguments=("$scratch/app.image")
  if [[ $mode == rebuilt ]]; then arguments=(--rebuild "${arguments[@]}"); fi
  "$bin" "${arguments[@]}" >"$scratch/$mode.out" 2>"$scratch/$mode.err"
  cmp "$scratch/expected.out" "$scratch/$mode.out"
  test ! -s "$scratch/$mode.err"
done
cp -R "$prefix" "$scratch/bundle"
"$bin" --bake "$scratch/app.image" "$scratch/bundle/bin/app"
"$scratch/bundle/bin/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
test ! -s "$scratch/baked.err"

# A live builder owns a process-local buffer. Never serialize its native address.
cat >"$scratch/raw-capture.birnam" <<BIRNAM
let captured := [[StringBuilder new] add: "raw"];
let main := [| [System println: [[captured size] printString]]];
[System writeFile: "$scratch/raw-capture.image" contents: "preserved"];
[System saveImage: "$scratch/raw-capture.image"];
[System println: [[System imageSaveReport] status]]
BIRNAM
"$bin" "$scratch/raw-capture.birnam" >"$scratch/raw-capture.out" 2>"$scratch/raw-capture.err"
test ! -s "$scratch/raw-capture.err"
grep -Fxq '#rejected' "$scratch/raw-capture.out"
test "$(cat "$scratch/raw-capture.image")" = preserved
printf 'StringBuilder: copied String identity, source-free artifact, mapped/rebuilt Image, baked entry and native-buffer rejection passed\n'
