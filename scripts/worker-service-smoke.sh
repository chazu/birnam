#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: worker-service-smoke.sh PREFIX}
work=$(mktemp -d "${TMPDIR:-/tmp}/birnam-service-smoke.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then trap 'rm -rf "$work"' EXIT; else echo "$work"; fi
cat > "$work/cell-save.birnam" <<EOF
[Globals at: #ServiceCellImageProbe put: [41 asCell]];
[DispatchGeneration initializeCurrent];
[System saveImage: "$work/cell.image"];
[System println: [[System imageSaveReport] status]];
EOF
"$prefix/bin/birnam" "$work/cell-save.birnam" > "$work/cell-save.out"
grep -Fxq '#saved' "$work/cell-save.out"
for recovery in mapped rebuilt; do
  arguments=()
  if [[ $recovery == rebuilt ]]; then arguments+=(--rebuild); fi
  "$prefix/bin/birnam" "${arguments[@]}" "$work/cell.image" > "$work/cell-$recovery.out" <<'EOF'
[ServiceCellImageProbe value]
[ServiceCellImageProbe set: 42]
[System gc]
[ServiceCellImageProbe value]
:quit
EOF
  grep -Fxq 'birnam> 41' "$work/cell-$recovery.out"
  grep -Fxq 'birnam> 42' "$work/cell-$recovery.out"
done
cat > "$work/run.birnam" <<'EOF'
[Module load: #os];
let program := [Concurrency load: #["scripts/worker-service-smoke.birnam"]];
[[program isError] ifTrue: [| [System println: program]; [OS exit: 1]]];
let passed := [ServiceCheck run: program];
[program close];
[[passed = true] ifFalse: [| [OS exit: 1]]];
EOF
for mode in aot source; do
  if [[ $mode == source ]]; then export BIRNAM_FORCE_SOURCE=1; else unset BIRNAM_FORCE_SOURCE; fi
  "$prefix/bin/birnam" "$work/run.birnam" > "$work/$mode.out"
  cat "$work/$mode.out"
  grep -Fxq '#[#workerServiceChecks 19 #failures 0]' "$work/$mode.out"
done
