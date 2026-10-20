#!/usr/bin/env bash
# Exercise source-authored control and collection callbacks without native-stack
# recursion, including the 100k loop and precise collection during sort.
set -euo pipefail

cd "$(dirname "$0")/.."

prefix=${1:?usage: stackless-control-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
test -x "$bin"

smoke_root=$(mktemp -d "${TMPDIR:-/tmp}/birnam-stackless-control.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$smoke_root"' EXIT
else
  printf 'stackless control smoke root: %s\n' "$smoke_root" >&2
fi

run_script="$smoke_root/run.birnam"

cat >"$run_script" <<'EOF'
[System println: [true ifTrue: [| 11] ifFalse: [| 22]]];
[System println: [false ifTrue: [| 11] ifFalse: [| 22]]];
[System println: [[| false] whileTrue: [| nil]]];
let counter := [0 asCell];
let loopResult := [[| [[counter value] < 100000]] whileTrue: [| [counter set: [[counter value] + 1]]]];
[System println: [counter value]];
[System println: [loopResult isNil]];
let falseCounter := [0 asCell];
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
[System println: [[1 to: 100000] detect: [:each | [each = 100000]]]];
[System println: [[1 to: 100000] detect: [:each | false]]];
EOF

"$bin" "$run_script" >"$smoke_root/run.out"

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
100000
nil
EOF

cmp "$smoke_root/expected.out" "$smoke_root/run.out"
echo "stackless control: source callbacks, 100k loops, collection and error boundaries passed"
