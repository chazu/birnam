#!/usr/bin/env bash
# Prove reflective definitions publish one current portable CodeBinding, replace
# and remove through ordinary dispatch, and release predecessor TCC units after
# the outer Execution turn has ended. Separate REPL turns are load-bearing.
set -euo pipefail

cd "$(dirname "$0")/.."

prefix=${1:?usage: portable-code-binding-lifecycle-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
test -x "$bin"

smoke_root=$(mktemp -d "${TMPDIR:-/tmp}/birnam-portable-code-binding-lifecycle.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$smoke_root"' EXIT
else
  printf 'portable CodeBinding lifecycle smoke root: %s\n' "$smoke_root" >&2
fi

"$bin" --repl >"$smoke_root/repl.out" 2>"$smoke_root/repl.err" <<'EOF'
let portableBindingBaseline := [Codegen currentCodeBindingCount]
let portableBindingBytesBaseline := [Codegen codeBindingGeneratedBytes]
[Globals at: #PortableBindingTarget put: [Object clone]]
[Reflect defineMethod: #answer: on: [Globals at: #PortableBindingTarget] as: [:self :value | [value + 1]]]
[Codegen currentCodeBindingCount]
[[Globals at: #PortableBindingTarget] answer: 41]
[Reflect defineMethod: #answer: on: [Globals at: #PortableBindingTarget] as: [:self :value | [value + 2]]]
[[Globals at: #PortableBindingTarget] answer: 40]
[Codegen currentCodeBindingCount]
[Codegen retiringCodeBindingCount]
[Codegen codeBindingGeneratedBytes]
[Reflect defineMethod: #removeSelf on: [Globals at: #PortableBindingTarget] as: [:self | [Reflect removeMethod: #removeSelf on: self]; [Globals at: #RetiringDuringSelfRemoval put: [Codegen retiringCodeBindingCount]]; 7]]
[[Globals at: #PortableBindingTarget] removeSelf]
[[[Globals at: #PortableBindingTarget] removeSelf] isError]
[Reflect removeMethod: #answer: on: [Globals at: #PortableBindingTarget]]
[Codegen currentCodeBindingCount]
[System gc]
[Codegen reapCodeBindings]
[System gc]
[Codegen reapCodeBindings]
[Codegen retiringCodeBindingCount]
[Codegen codeBindingGeneratedBytes]
[Globals includes: #__rust_mblk_0]
[Globals at: #RetiringDuringSelfRemoval]
:quit
EOF

python3 - "$smoke_root/repl.out" <<'PY'
import re
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    answers = re.findall(r"^birnam> (.*)$", handle.read(), re.MULTILINE)

answers = [answer for answer in answers if answer]
assert len(answers) == 24, answers
baseline = int(answers[0])
bytes_baseline = int(answers[1])
assert answers[2] == "anObject", answers
assert answers[3] == "anObject", answers
assert int(answers[4]) == baseline + 1, answers
assert answers[5] == "42", answers
assert answers[6] == "anObject", answers
assert answers[7] == "42", answers
assert int(answers[8]) == baseline + 1, answers
# Design 83 checkpoints automatically reap the predecessor on the previous
# outer return; callers no longer have to issue a separate reap request.
assert answers[9] == "0", answers
assert int(answers[10]) > bytes_baseline, answers
assert answers[11] == "anObject", answers
assert answers[12] == "7", answers
assert answers[13] == "true", answers
assert answers[14] == "anObject", answers
assert int(answers[15]) == baseline, answers
assert int(answers[16]) >= 0, answers
assert answers[17] == "0", answers
assert int(answers[18]) >= 0, answers
assert answers[19] == "0", answers
assert answers[20] == "0", answers
assert int(answers[21]) == bytes_baseline, answers
assert answers[22] == "false", answers
assert int(answers[23]) >= 1, "self-removal must retain the executing entry until its outer return"
PY

test ! -s "$smoke_root/repl.err"
printf 'portable-code-binding-lifecycle-smoke: publication, replacement, self-removal, and retirement passed\n'
