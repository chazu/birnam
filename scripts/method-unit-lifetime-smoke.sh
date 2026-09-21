#!/usr/bin/env bash
# Retained Method and alias ownership of real anonymous TCC materializations.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: method-unit-lifetime-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-method-unit.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then trap 'rm -rf "$scratch"' EXIT
else printf 'method Unit smoke root: %s\n' "$scratch"; fi
for disposition in Synchronous Resumable Source; do
  expression="[Codegen materialize$disposition: [CodeObject fromProgram: [CodeProgram parameters: #[\"argument\"] captures: #[] statements: #[[CodeStatement expression: [CodeNode lexicalReadAt: 0]]]]] captures: #[]]"
  if [[ $disposition == Source ]]; then expression='[:argument | argument]'; fi
  # A fresh Float supplies the one-word payload expected by a capture-free
  # resumable Block, without retaining any generated Unit of its own.
  "$bin" --repl >"$scratch/$disposition.out" 2>"$scratch/$disposition.err" <<EOF
[System preciseGc: true]
[Globals at: #MethodUnitReceiver put: [0.0 clone]]
[Codegen evictMaterializations]
[System gc]
let methodUnitBaseline := [Tcc liveUnitCount]
[Globals at: #MethodUnitHeld put: [$expression methodFor: #value:]]
[Codegen evictMaterializations]
[System gc]
[[Globals at: #MethodUnitHeld] invokeOn: [Globals at: #MethodUnitReceiver] arguments: #[73]]
[[Vtable ensurePrivateVtableFor: [Globals at: #MethodUnitReceiver]] addMethod: [Globals at: #MethodUnitHeld] as: #methodUnitAlias:]
[Globals at: #MethodUnitHeld put: nil]
[System gc]
[[Globals at: #MethodUnitReceiver] methodUnitAlias: 89]
[[Reflect vtableOf: [Globals at: #MethodUnitReceiver]] removeMethod: #methodUnitAlias:]
[System gc]
[[Tcc liveUnitCount] = [methodUnitBaseline + 1]]
:quit
EOF
  test ! -s "$scratch/$disposition.err"
  python3 - "$scratch/$disposition.out" <<'PY'
import re,sys
answers=re.findall(r'^birnam> (.*)$',open(sys.argv[1]).read(),re.M)
answers=[a for a in answers if a]
assert len(answers)==16, answers
assert answers[8]=='73' and answers[12]=='89' and answers[15]=='true',answers
assert not any('Error' in answer for answer in answers),answers
PY
done
printf 'method-unit-lifetime-smoke: synchronous/resumable/source Method and alias survive eviction/GC, then release their native Unit across outer turns\n'
