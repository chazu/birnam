#!/usr/bin/env bash
# Real source-authored lifecycle and cleanup handlers on the checked Machine.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: task-control-smoke.sh PREFIX [FLOOR]}
if [[ $# -gt 1 ]]; then export BIRNAM_ID_DYLIB="$2"; fi
bin=${BIRNAM_TASK_TEST_BIN:-"$prefix/bin/birnam"}
smoke_root=$(mktemp -d "${TMPDIR:-/tmp}/birnam-task-control.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$smoke_root"' EXIT
else
  echo "task control smoke root: $smoke_root" >&2
fi
for execution in aot source; do
work="$smoke_root/$execution"
mkdir -p "$work"
if [[ $execution == source ]]; then
  export BIRNAM_FORCE_SOURCE=1
else
  unset BIRNAM_FORCE_SOURCE
fi
# The fixture's methods and nested blocks use the normal candidate-selection
# API. Record the synchronous baseline before taking the generation snapshot,
# then select its resumable implementations before invoking the fixture.
python3 - "$work" <<'PY'
import json
from pathlib import Path
import sys
work = Path(sys.argv[1])
source = json.dumps(Path('scripts/task-control-smoke.birnam').read_text())
candidate = work / 'checks.dylib'
(work / 'run.birnam').write_text(f'''
[Globals at: #NativeTaskObserver put: [Object clone]];
[Reflect defineMethod: #observe: on: NativeTaskObserver as: [:self :target | [target outcome]]];
[Reflect defineMethod: #failTask: on: NativeTaskObserver as: [:self :target | [target fail: #mustNotRecord]]];
[Reflect defineMethod: #ensureTask:body: on: NativeTaskObserver as: [:self :target :body |
  [target ensure: body cleanup: [| nil]]]];
let source := {source};
[Codegen eval: source];
[Codegen recordModuleSource: source named: #taskControlSmoke];
[DispatchGeneration initializeCurrent];
[CompiledUnit compile: source named: #taskControlSmoke output: "{candidate}" selection: #resumableRequired];
let unit := [CompiledUnit open: "{candidate}"];
[unit map];
[[unit activate] fulfill];
[TaskCheck run];
[System saveImage: "{work / 'handle.image'}"];
[System println: [[System imageSaveReport] status]];
[Globals at: #TaskControlHandleProof put: nil];
[System saveImage: "{work / 'outcome.image'}"];
[System println: [[System imageSaveReport] status]];
''')
PY
"$bin" "$work/run.birnam" >"$work/output" || { cat "$work/output"; exit 1; }
cat "$work/output"
grep -Fxq '#[#taskControlChecks 105 #failures 0]' "$work/output"
grep -Fxq '#rejected' "$work/output"
test ! -e "$work/handle.image"
grep -Fxq '#saved' "$work/output"
for mode in mapped rebuilt; do
  arguments=()
  if [[ $mode == rebuilt ]]; then arguments+=(--rebuild); fi
  "$bin" "${arguments[@]}" "$work/outcome.image" >"$work/$mode.out" <<'EOF'
[Globals at: #TaskControlOutcomeProof]
[[TaskControl start: [:task | 42]] pollOutcome]
:quit
EOF
  grep -Fxq 'birnam> #[#failure #[#bodyFailed #cleanupFailed] #late #[]]' "$work/$mode.out"
  grep -Fxq 'birnam> #[#success 42]' "$work/$mode.out"
done
echo "task-control-smoke ($execution): handle rejected; outcome round-trips and fresh tasks work in mapped and rebuilt images"
done
