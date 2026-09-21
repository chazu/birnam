#!/usr/bin/env bash
# Compare observable native effects with optimization disabled and enabled.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: native-integer-admission-smoke.sh PREFIX}
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-native-integer-admission.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -Iabi/fixtures/idapi-v20 \
  scripts/fixtures/native-integer-admission.c -o "$scratch/probe.dylib"
printf '[System loadModule: "%s/probe.dylib"];\n' "$scratch" >"$scratch/probe.birnam"
cat >>"$scratch/probe.birnam" <<'BIRNAM'
[System load: "modules/os.birnam"];
let program := [CodeProgram parameters: #["a" "b"] captures: #[] statements: #[[CodeStatement letNamed: "first" expression: [CodeNode send: #+ operands: #[[CodeNode lexicalReadAt: 0] [CodeNode lexicalReadAt: 1]]]] [CodeStatement letNamed: "second" expression: [CodeNode send: #+ operands: #[[CodeNode lexicalReadAt: 2] [CodeNode lexicalReadAt: 1]]]] [CodeStatement expression: [CodeNode lexicalReadAt: 3]]]];
let numeric := [Codegen materializeResumable: [CodeObject fromProgram: program] captures: #[]];
let nested := [CodeProgram parameters: #[] captures: #["input"] statements: #[[CodeStatement expression: [CodeNode lexicalReadAt: 0]]]];
let directProgram := [CodeProgram parameters: #["input"] captures: #[] statements: #[[CodeStatement letNamed: "closure" expression: [CodeNode block: nested captureBindings: #[0]]] [CodeStatement letNamed: "result" expression: [CodeNode send: #value operands: #[[CodeNode lexicalReadAt: 1]]]] [CodeStatement expression: [CodeNode lexicalReadAt: 2]]]];
let direct := [Codegen materializeResumable: [CodeObject fromProgram: directProgram] captures: #[]];
let value := [Object clone];
BIRNAM
work='[128 timesRepeat: [| [[[numeric value: 11 value: 3] = 17] ifTrue: [| nil] ifFalse: [| [OS exit: 40]]]; [[[direct value: value] == value] ifTrue: [| nil] ifFalse: [| [OS exit: 41]]]]];'
printf '%s\n%s\n' "$work" "$work" >>"$scratch/probe.birnam"
for mode in {1..8}; do
  printf '[System nativeAdmissionMode: %s];\n%s\n[System nativeAdmissionReport];\n[System nativeAdmissionMode: 0];\n%s\n' "$mode" "$work" "$work" >>"$scratch/probe.birnam"
done
"$prefix/bin/birnam" --compile-module "$scratch/probe.birnam" "$scratch/probe-compiled.dylib"
for enabled in 0 1; do
  BIRNAM_DEV_CLOSURE_ELISION=$enabled BIRNAM_DEV_CLOSURE_REPORT=1 BIRNAM_DEV_UNBOXED_NUMERIC=$enabled \
    "$prefix/bin/birnam" "$scratch/probe-compiled.dylib" >"$scratch/$enabled.out" 2>"$scratch/$enabled.err"
done
cmp "$scratch/0.out" "$scratch/1.out"
test "$(wc -l <"$scratch/1.out" | tr -d ' ')" = 8
rg -Fq 'closure-elision: hit materialized=false' "$scratch/1.err"
python3 - "$scratch/1.err" <<'PY'
from pathlib import Path
import sys
parts=Path(sys.argv[1]).read_text().split('native-admission-mode 0\n')[1:]
assert len(parts)==8
for part in parts:
    restored=part.split('native-admission-mode ',1)[0]
    assert 'closure-elision: hit materialized=false' in restored, 'restoring methods must restore admission'
PY
cat "$scratch/1.out"
echo 'native Integer admission: all eight replacement paths preserve baseline effects'
