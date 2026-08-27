#!/usr/bin/env bash
# Prove Design 60J/J5's fixed-live-set plateaus, retained-history attribution,
# and save/load/continued-churn/resave boundary.
set -euo pipefail

cd "$(dirname "$0")/.."

prefix=${1:?usage: runtime-bounded-churn-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
test -x "$bin"

method_cycles=${BIRNAM_CHURN_METHOD_CYCLES:-1667}
items=${BIRNAM_CHURN_ITEMS:-5000}
control_items=${BIRNAM_CHURN_CONTROL_ITEMS:-1000}
if ((method_cycles * 3 < 5000 || items < 5000)); then
  printf 'runtime bounded-churn overrides do not satisfy J5 minimums\n' >&2
  exit 2
fi

smoke_root=$(mktemp -d "${TMPDIR:-/tmp}/birnam-runtime-bounded-churn.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$smoke_root"' EXIT
else
  printf 'runtime bounded-churn root: %s\n' "$smoke_root" >&2
fi

image="$smoke_root/churn.image"
resaved="$smoke_root/churn-resaved.image"

emit_method_batch() {
  cat <<EOF
[methodBatch value]
[System gc];
[Codegen reapCodeBindings];
[System gc];
[Codegen reapCodeBindings];
[System gc];
"METHOD"
[Codegen liveHeapBytes]
[Codegen liveSymbolCount]
[Codegen liveSymbolNameBytes]
[Codegen liveVtableCount]
[Codegen currentCodeBindingCount]
[Codegen retiringCodeBindingCount]
[Codegen codeBindingGeneratedBytes]
[db retainedFactBytes]
EOF
}

emit_symbol_batch() {
  cat <<EOF
[symbolBatch value]
[System gc];
"SYMBOL"
[Codegen liveHeapBytes]
[Codegen liveSymbolCount]
[Codegen liveSymbolNameBytes]
[Codegen liveVtableCount]
[db retainedFactBytes]
EOF
}

emit_history_batch() {
  local policy=$1
  case $policy in
    current)
      cat <<EOF
[currentHistoryBatch value]
EOF
      ;;
    since)
      cat <<EOF
[sinceHistoryBatch value]
EOF
      ;;
    last)
      cat <<EOF
[lastHistoryBatch value]
EOF
      ;;
  esac
  cat <<EOF
[db compact];
[System gc];
"HISTORY-$policy"
[db retainedFactCount]
[db retainedFactBytes]
[db oldestRetainedBasis]
[Codegen liveHeapBytes]
EOF
}

write_churn_program() {
  local output=$1
  local save_path=$2
  local phase=$3
  {
    cat <<EOF
let target := [Object clone];
let first := [:self | 1];
let second := [:self | 2];
let methodBatch := [| [$method_cycles timesRepeat: [| [Reflect defineMethod: #j5BoundedProbe on: target as: first]; [Reflect defineMethod: #j5BoundedProbe on: target as: second]; [Reflect removeMethod: #j5BoundedProbe on: target]]]];
let symbolBatch := [| [1 to: $items do: [:i | [["j5-transient-" , [i printString]] asSymbol]]]];
let currentHistoryBatch := [| [db retainCurrentOnly]; [1 to: $items do: [:i | [db assert: #j5HistoryEntity attr: #j5HistoryAttribute value: i]]]];
let sinceHistoryBatch := [| [db retainSince: [db basis]]; [1 to: $items do: [:i | [db assert: #j5HistoryEntity attr: #j5HistoryAttribute value: i]]]];
let lastHistoryBatch := [| [db retainLast: 128]; [1 to: $items do: [:i | [db assert: #j5HistoryEntity attr: #j5HistoryAttribute value: i]]]];
let controlHistoryBatch := [| [1 to: $control_items do: [:i | [db assert: #j5HistoryEntity attr: #j5HistoryAttribute value: i]]]];
[db assert: #j5HistoryAttribute attr: #history value: true];
EOF
    emit_method_batch
    if [[ $phase == full ]]; then
      emit_method_batch
      emit_method_batch
    fi
    emit_symbol_batch
    if [[ $phase == full ]]; then
      emit_symbol_batch
      emit_symbol_batch
    fi
    emit_history_batch current
    if [[ $phase == full ]]; then
      emit_history_batch current
      emit_history_batch current
    fi
    emit_history_batch since
    if [[ $phase == full ]]; then
      emit_history_batch since
      emit_history_batch since
    fi
    emit_history_batch last
    if [[ $phase == full ]]; then
      emit_history_batch last
      emit_history_batch last
      cat <<EOF
let unboundedFloor := [db basis];
[db retainSince: unboundedFloor];
EOF
      for ordinal in 1 2 3; do
        cat <<EOF
[controlHistoryBatch value]
[db compact];
"CONTROL"
[db retainedFactBytes]
EOF
      done
    fi
    cat <<EOF
[db retainCurrentOnly];
[db compact];
[System gc];
"CLEAN"
[db retainedFactBytes]
[System saveImage: "$save_path"];
:quit
EOF
  } >"$output"
}

write_churn_program "$smoke_root/initial.birnam" "$image" full
"$bin" --repl <"$smoke_root/initial.birnam" >"$smoke_root/initial.out" 2>"$smoke_root/initial.err"
test ! -s "$smoke_root/initial.err"
test -s "$image"

write_churn_program "$smoke_root/continued.in" "$resaved" continued
"$bin" "$image" <"$smoke_root/continued.in" >"$smoke_root/continued.out" 2>"$smoke_root/continued.err"
test ! -s "$smoke_root/continued.err"
test -s "$resaved"

printf '[db retainedFactBytes]\n:quit\n' \
  | "$bin" "$resaved" >"$smoke_root/resaved.out" 2>"$smoke_root/resaved.err"
test ! -s "$smoke_root/resaved.err"

python3 - "$smoke_root/initial.out" "$smoke_root/continued.out" "$smoke_root/resaved.out" <<'PY'
import re
import sys

TOLERANCE = 64 * 1024

def sections(path):
    with open(path, encoding="utf-8") as handle:
        lines = [line.removeprefix("birnam> ").strip() for line in handle]
    markers = {"METHOD": 8, "SYMBOL": 5, "HISTORY-current": 4,
               "HISTORY-since": 4, "HISTORY-last": 4,
               "CONTROL": 1, "CLEAN": 1}
    found = {key: [] for key in markers}
    index = 0
    while index < len(lines):
        marker = lines[index]
        width = markers.get(marker)
        if width is None:
            index += 1
            continue
        values = [int(value) for value in lines[index + 1:index + 1 + width]]
        assert len(values) == width, (path, marker, values)
        found[marker].append(values)
        index += width + 1
    return found

def plateau(rows, columns, tolerance=0):
    assert len(rows) == 3, rows
    for column in columns:
        values = [row[column] for row in rows]
        assert max(values) - min(values) <= tolerance, values

initial = sections(sys.argv[1])
plateau(initial["METHOD"], [1, 2, 3, 4, 5, 6, 7])
plateau(initial["METHOD"], [0], TOLERANCE)
for row in initial["METHOD"]:
    assert row[4:7] == [0, 0, 0], row
plateau(initial["SYMBOL"], [1, 2, 3, 4])
plateau(initial["SYMBOL"], [0], TOLERANCE)
for policy in ("current", "since", "last"):
    plateau(initial[f"HISTORY-{policy}"], [0, 1])
    plateau(initial[f"HISTORY-{policy}"], [3], TOLERANCE)
control = [row[0] for row in initial["CONTROL"]]
assert len(control) == 3 and control[0] < control[1] < control[2], control
assert len(initial["CLEAN"]) == 1

continued = sections(sys.argv[2])
assert len(continued["METHOD"]) == 1, continued["METHOD"]
assert continued["METHOD"][0][4:7] == [0, 0, 0], continued["METHOD"]
assert len(continued["SYMBOL"]) == 1, continued["SYMBOL"]
for policy in ("current", "since", "last"):
    assert len(continued[f"HISTORY-{policy}"]) == 1, continued[f"HISTORY-{policy}"]
assert len(continued["CLEAN"]) == 1

with open(sys.argv[3], encoding="utf-8") as handle:
    answers = re.findall(r"^birnam> ([0-9]+)$", handle.read(), re.MULTILINE)
assert answers, sys.argv[3]
PY

printf 'runtime-bounded-churn-smoke: method, Symbol, bounded-history, retained-control, and save/load/resave plateaus passed\n'
