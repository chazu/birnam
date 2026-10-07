#!/usr/bin/env bash
# Guarded ABI literals and authored Word helpers survive source-free images.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: word-optimization-image-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-word-optimization.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cat >"$scratch/helper.birnam" <<'BIRNAM'
[Module named: #wordOptimizationImage requires: #[]];
let mask := [Execution wordResultOf: 15 selector: #word];
let probe := [Object clone];
[Reflect defineMethod: #compute:with: on: probe as: [:self :a :b |
  let x := [a bitXor: b]; let sum := [x wrappingAdd: a];
  [[sum bitXor: mask] wrappingSubtract: sum]
]];
[Reflect defineMethod: #convert: on: probe as: [:self :value |
  let bridge := Execution; let target := Integer;
  let signature := "Wo"; let factory := Word;
  [bridge sendNativeTo: target selector: #wordOf: arguments: #[value]
    signature: signature resultFactory: factory]
]];
[Globals at: #WordOptimizationImage put: probe]; true
BIRNAM
cat >"$scratch/save.birnam" <<BIRNAM
let source := [System readFile: "$scratch/helper.birnam"];
let artifact := [Codegen compile: source output: "$scratch/helper.dylib"
  from: "$scratch/helper.birnam" synchronousBodies: true];
[[artifact isError] ifTrue: [| [System println: artifact]; [System exit: 1]]];
let loaded := [Codegen load: artifact];
[[loaded isError] ifTrue: [| [System println: loaded]; [System exit: 1]]];
let main := [|
  let a := [Execution wordResultOf: 10 selector: #word];
  let b := [Execution wordResultOf: 6 selector: #word];
  [System println: [[WordOptimizationImage compute: a with: b] asInteger]];
  [System println: [[WordOptimizationImage convert: 123] asInteger]];
  [System println: [12345 + 37]];
  [System println: [12345 * 37]];
  [System println: "word-optimization-image-ok"]
];
[System saveImage: "$scratch/optimized.image"];
[System println: [[System imageSaveReport] status]];
BIRNAM
BIRNAM_NO_STDLIB=1 "$bin" "$scratch/save.birnam" >"$scratch/save.out" 2>"$scratch/save.err"
test ! -s "$scratch/save.err"
grep -Fxq '#saved' "$scratch/save.out"
test -s "$scratch/optimized.image"
rm "$scratch/save.birnam" "$scratch/helper.birnam" "$scratch/helper.dylib"
printf '3\n123\n12382\n456765\nword-optimization-image-ok\n' >"$scratch/expected.out"
"$bin" "$scratch/optimized.image" >"$scratch/mapped.out" 2>"$scratch/mapped.err"
"$bin" --rebuild "$scratch/optimized.image" >"$scratch/rebuilt.out" 2>"$scratch/rebuilt.err"
for mode in mapped rebuilt; do
  cmp "$scratch/expected.out" "$scratch/$mode.out"
  test ! -s "$scratch/$mode.err"
done
printf 'Word optimization: source-free mapped/rebuilt helpers, ABI literals and Integer arithmetic passed\n'
