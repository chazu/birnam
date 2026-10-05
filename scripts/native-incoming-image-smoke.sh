#!/usr/bin/env bash
# Typed source methods preserve their ABI through compiled and saved artifacts.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: native-incoming-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-native-incoming.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cat >"$scratch/save.birnam" <<BIRNAM
let NativeProbe := [Object clone];
let wordBody := [Execution block: [:self :bits | bits] withNativeSignature: "ww"];
[Reflect defineMethod: #word: on: NativeProbe as: wordBody];
let byteBody := [Execution block: [:self :bytes | [System gc]; bytes] withNativeSignature: "bb"];
[Reflect defineMethod: #bytes: on: NativeProbe as: byteBody];
let noneBody := [Execution block: [:self | nil] withNativeSignature: "W"];
[Reflect defineMethod: #none on: NativeProbe as: noneBody];
let NativeAlias := [Object clone];
[[Reflect vtableOf: NativeAlias] addMethod: [NativeProbe methodFor: #word:] as: #word:];
let main := [|
  let bits := [Execution wordResultOf: 42 selector: #word];
  let result := [Execution sendNativeTo: NativeProbe selector: #word: arguments: #[bits] signature: "ww" resultFactory: Word];
  [System println: [result asInteger]];
  let aliasResult := [Execution sendNativeTo: NativeAlias selector: #word: arguments: #[bits] signature: "ww" resultFactory: Word];
  [System println: [aliasResult asInteger]];
  let bytes := [Execution sendNativeTo: NativeProbe selector: #bytes: arguments: #[["hello" asBytes]] signature: "bb" resultFactory: Bytes];
  [System println: [bytes asString]];
  let none := [Execution sendNativeTo: NativeProbe selector: #none arguments: #[] signature: "u" resultFactory: Word];
  [System println: [none == nil]];
  [System println: "incoming-image-ok"]
];
[System saveImage: "$scratch/typed.image"];
[System println: [[System imageSaveReport] status]];
BIRNAM
"$bin" "$scratch/save.birnam" >"$scratch/save.out" 2>"$scratch/save.err"
test ! -s "$scratch/save.err"
grep -Fxq '#saved' "$scratch/save.out"
test -s "$scratch/typed.image"
rm "$scratch/save.birnam"
printf '42\n42\nhello\ntrue\nincoming-image-ok\n' >"$scratch/expected.out"
"$bin" "$scratch/typed.image" >"$scratch/mapped.out" 2>"$scratch/mapped.err"
"$bin" --rebuild "$scratch/typed.image" >"$scratch/rebuilt.out" 2>"$scratch/rebuilt.err"
for mode in mapped rebuilt; do
  cmp "$scratch/expected.out" "$scratch/$mode.out"
  test ! -s "$scratch/$mode.err"
done
printf 'incoming ABI: source-free mapped/rebuilt images and captured method alias passed\n'
