#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: multicore-smoke.sh PREFIX}
work=$(mktemp -d "${TMPDIR:-/tmp}/birnam-multicore.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then trap 'rm -rf "$work"' EXIT; else echo "multicore smoke root: $work"; fi
python3 - "$work" <<'PY'
import json,sys,time
from pathlib import Path
w=Path(sys.argv[1]); image=w/'workers.image'
fixture=f'''
[Module named: #multicoreWorker requires: #[]];
[Alien wrap: "clock_gettime" signature: "i i p" as: #multicoreTimeInto: on: Integer];
[Globals at: #MulticoreClock put: [Object clone]];
[Reflect defineMethod: #read: on: MulticoreClock as: [:self :clock |
  let stamp := [Alien alloc: 16];
  [clock multicoreTimeInto: stamp];
  let value := [[[stamp peekIntAt: 0] * 1000000] + [[stamp peekIntAt: 8] / 1000]];
  [stamp free]; value]];
[Reflect defineMethod: #now on: MulticoreClock as: [:self | [self read: {time.CLOCK_MONOTONIC}]]];
[Reflect defineMethod: #cpu on: MulticoreClock as: [:self | [self read: {time.CLOCK_PROCESS_CPUTIME_ID}]]];
[Globals at: #MulticoreWorker put: [Object clone]];
[Reflect defineMethod: #echo: on: MulticoreWorker as: [:self :value | value]];
[Reflect defineMethod: #fail: on: MulticoreWorker as: [:self :value | [Error kind: #workerFailed]]];
[Reflect defineMethod: #cpu: on: MulticoreWorker as: [:self :duration |
  let start := [MulticoreClock now];
  let cpu := [MulticoreClock cpu];
  let count := [0 asCell];
  [[| [[[MulticoreClock now] - start] < duration]] whileTrue: [|
    [1000 timesRepeat: [| [count inc]]]]];
  #[start [MulticoreClock now] [[MulticoreClock cpu] - cpu] [count value]]]];
'''
(w/'worker.birnam').write_text(fixture)
(w/'save.birnam').write_text(f'[System load: {json.dumps(str(w/"worker.birnam"))}]; [DispatchGeneration initializeCurrent]; [System saveImage: {json.dumps(str(image))}]; [System println: [[System imageSaveReport] status]];')
source=Path('scripts/multicore-smoke.birnam').read_text()
(w/'run.birnam').write_text(f'''
[System load: {json.dumps(str(w/'worker.birnam'))}];
let source := {json.dumps(source)};
[Codegen eval: source];
[Codegen recordModuleSource: source named: #multicoreSmoke];
[DispatchGeneration initializeCurrent];
[MulticoreCheck run: "{image}"];
''')
PY
"$prefix/bin/birnam" "$work/save.birnam" > "$work/save.out"
grep -Fxq '#saved' "$work/save.out"
mode=source
"$prefix/bin/birnam" "$work/run.birnam" > "$work/$mode.out"
cat "$work/$mode.out"
grep -Fxq '#[#multicoreChecks 24 #failures 0]' "$work/$mode.out"
python3 - "$work/$mode.out" <<'PY'
import re,sys
s=open(sys.argv[1]).read()
m=re.search(r'#\[#multicoreTiming (\d+) (\d+) (\d+)\]',s)
assert m, s
sequential,parallel,cpu=map(int,m.groups())
print(f'CPU evidence: sequential={sequential/1e6:.3f}s parallel={parallel/1e6:.3f}s worker CPU/wall={cpu/parallel:.2f}')
# A resource-constrained host may not provide simultaneous CPU service. Keep
# the measurement explicit rather than turning machine load into a flaky gate.
PY
