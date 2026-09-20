#!/usr/bin/env bash
# Prove a source-less portable resumable unit survives repository eviction
# while its Block is live, then retires after the outer Execution turn
# ends. Separate REPL turns are load-bearing: retirement is forbidden from
# physically unmapping native code beneath the send that requested it.
set -euo pipefail

cd "$(dirname "$0")/.."

prefix=${1:?usage: portable-resumable-lifecycle-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
lib="$prefix/lib"
test -x "$bin"

smoke_root=$(mktemp -d "${TMPDIR:-/tmp}/birnam-portable-resumable-lifecycle.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$smoke_root"' EXIT
else
  printf 'portable resumable lifecycle smoke root: %s\n' "$smoke_root" >&2
fi

"$bin" --repl >"$smoke_root/repl.out" 2>"$smoke_root/repl.err" <<'EOF'
let portableLifecycleBaseline := [Tcc liveUnitCount]
[Globals at: #PortableLifecycleBlock put: [Codegen materializeResumable: [CodeObject fromProgram: [CodeProgram parameters: #["argument"] captures: #[] statements: #[[CodeStatement expression: [CodeNode lexicalReadAt: 0]]]]] captures: #[]]]
[Tcc liveUnitCount]
[[Globals at: #PortableLifecycleBlock] value: 73]
[Codegen evictMaterializations]
[[Globals at: #PortableLifecycleBlock] value: 89]
[Globals at: #PortableLifecycleBlock put: nil]
[System gc]
[Tcc liveUnitCount]
:quit
EOF

python3 - "$smoke_root/repl.out" <<'PY'
import re
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    answers = re.findall(r"^birnam> (.*)$", handle.read(), re.MULTILINE)

answers = [answer for answer in answers if answer]
assert len(answers) == 9, answers
baseline = int(answers[0])
assert answers[1] == "anObject", answers
# The count expression, the portable unit, and possibly the just-completed
# disposable evaluator are live before the explicit collection below.
assert int(answers[2]) > baseline + 1, answers
assert answers[3] == "73", answers
assert answers[4] == "1", answers
assert answers[5] == "89", answers
assert answers[6] == "nil", answers
assert re.fullmatch(r"[0-9]+", answers[7]), answers
# Only the count expression's evaluator remains above the recorded baseline.
assert int(answers[8]) == baseline + 1, answers
PY

test ! -s "$smoke_root/repl.err"
printf 'portable-resumable-lifecycle-smoke: eviction survival and quiescent retirement passed\n'
