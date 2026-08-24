#!/usr/bin/env bash
# Prove that the ordinary source-authored Boolean and Block control unit is a
# selection-bearing Module AOT in G0 and drives a deep loop without native-stack
# recursion. The same contract must survive forced source recovery.
set -euo pipefail

cd "$(dirname "$0")/.."

prefix=${1:?usage: stackless-control-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
lib="$prefix/lib"
test -x "$bin"

smoke_root=$(mktemp -d "${TMPDIR:-/tmp}/birnam-stackless-control.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$smoke_root"' EXIT
else
  printf 'stackless control smoke root: %s\n' "$smoke_root" >&2
fi

run_script="$smoke_root/run.birnam"

cat >"$run_script" <<'EOF'
[DispatchGeneration initializeCurrent];
[System println: [true ifTrue: [| 11] ifFalse: [| 22]]];
[System println: [false ifTrue: [| 11] ifFalse: [| 22]]];
[System println: [[| false] whileTrue: [| nil]]];
let owner := [Object clone];
let counter := [db cellFor: owner attr: #counter];
[counter set: 0];
let loopResult := [[| [[counter value] < 100000]] whileTrue: [| [counter set: [[counter value] + 1]]]];
[System println: [counter value]];
[System println: [loopResult isNil]];
let falseCounter := [db cellFor: owner attr: #falseCounter];
[falseCounter set: 0];
[[| [[falseCounter value] >= 7]] whileFalse: [| [falseCounter set: [[falseCounter value] + 1]]]];
[System println: [falseCounter value]];
let bad := [[| 42] whileTrue: [| nil]];
[System println: [bad kind]];
let stopped := [[| true] whileTrue: [| [Error kind: #loopStopped]]];
[System println: [stopped kind]];
let integerCounter := [0 asCell];
let integerResult := [1 to: 100000 do: [:i | [integerCounter inc]]];
[System println: [integerCounter value]];
[System println: integerResult];
let rangeCounter := [0 asCell];
[[1 to: 10000] do: [:i | [rangeCounter inc]]];
[System println: [rangeCounter value]];
[System println: [[1 to: 100] inject: 0 into: [:sum :i | [sum + i]]]];
[System println: [#[1 2 3 4] collect: [:i | [i * 2]]]];
[System println: [#[1 2 3 4] select: [:i | [i even]]]];
[System println: ["abc" collect: [:c | [c , "!"]]]];
let sortedAcrossGc := [#[3 2 1] sortBy: [:each |
  let key := [each printString];
  [System gc];
  key]];
[System println: sortedAcrossGc];
let mapTotal := [0 asCell];
[{#a -> 3 #b -> 4} keysAndValuesDo: [:key :value | [mapTotal set: [[mapTotal value] + value]]]];
[System println: [mapTotal value]];
let cleaned := [false asCell];
let handled := [[| [Error kind: #expected]] on: #expected do: [:error | #handled]];
[[| 41] ensure: [| [cleaned set: true]]];
[System println: handled];
[System println: [cleaned value]];
EOF

BIRNAM_LIB_DIR="$lib" "$bin" "$run_script" >"$smoke_root/run.out"
BIRNAM_FORCE_SOURCE=1 BIRNAM_LIB_DIR="$lib" "$bin" "$run_script" >"$smoke_root/source.out"

cat >"$smoke_root/expected.out" <<'EOF'
11
22
nil
100000
true
7
#mustBeBoolean
#loopStopped
100000
1
10000
5050
#[2 4 6 8]
#[2 4]
#[a! b! c!]
#[1 2 3]
7
#handled
true
EOF

cmp "$smoke_root/expected.out" "$smoke_root/run.out"
cmp "$smoke_root/expected.out" "$smoke_root/source.out"
nm -j "$lib/libbirnam_stacklessControl.dylib" >"$smoke_root/module.symbols"
grep -Eq '__birnam_resumable_step_[0-9]+$' "$smoke_root/module.symbols"
grep -Eq '__birnam_resumable_thunk_[0-9]+$' "$smoke_root/module.symbols"
nm -gj "$lib/libbirnam_stacklessControl.dylib" >"$smoke_root/module.exports"
if grep -Eq '__birnam_resumable_(step|thunk)_[0-9]+$' "$smoke_root/module.exports"; then
  echo "stackless control artifact leaked artifact-local resumable helpers" >&2
  exit 1
fi

echo "stackless-control-smoke: G0 Module AOT and source recovery passed the 100k callback loop"
