#!/usr/bin/env bash
# Focused V2 capture/suspend and native callback authority assertions.
set -euo pipefail
unset BIRNAM_DEV_CLOSURE_FORCE_AT
cd "$(dirname "$0")/.."
prefix=${1:?prefix}
floor=${2:?diagnostic floor}
profile=${3:?profile}
work=$(mktemp -d "${TMPDIR:-/tmp}/birnam-closure-api.XXXXXX")
trap 'rm -rf "$work"' EXIT
  for precise in 0 1; do
    for control in capture suspend; do
      for mode in baseline enabled forced; do
        enabled=0
        forced=0
        if [[ $mode != baseline ]]; then enabled=1; fi
        if [[ $mode == forced ]]; then forced=1; fi
        BIRNAM_ID_DYLIB="$floor" BIRNAM_TEST_CLOSURE_CONTROL=1 \
          BIRNAM_CLOSURE_PRECISE=$precise BIRNAM_CLOSURE_CONTROL=$control \
          BIRNAM_DEV_CLOSURE_ELISION=$enabled BIRNAM_DEV_CLOSURE_FORCE_RECOVERY=$forced \
          BIRNAM_DEV_CLOSURE_REPORT=1 "$prefix/bin/birnam" scripts/fixtures/closure-control.birnam \
          >"$work/$mode.combined" 2>&1
      done
      python3 - "$work" "$profile" "$precise" "$control" "$floor" <<'PY'
import hashlib
from pathlib import Path
import sys
work = Path(sys.argv[1])
outputs = {}
for mode in ('baseline', 'enabled', 'forced'):
    lines = (work / (mode + '.combined')).read_text().splitlines()
    begin = lines.index('closure-control: begin measured control')
    end = lines.index('closure-control: end measured control')
    diagnostics = [line for line in lines[begin + 1:end] if line.startswith('closure-elision: ')]
    expected = ([] if mode == 'baseline' else ['closure-elision: hit materialized=true', 'closure-elision: completed calls=3']
                if mode == 'enabled' else ['closure-elision: forced constructor recovery'])
    assert diagnostics == expected, (mode, diagnostics)
    outputs[mode] = [line for line in lines if not line.startswith('closure-elision: ')]
assert outputs['baseline'] == outputs['enabled'] == outputs['forced'], outputs
print('closure-control-smoke:', 'profile=' + sys.argv[2], 'precise=' + sys.argv[3],
      'control=' + sys.argv[4], 'floor_sha256=' + hashlib.sha256(Path(sys.argv[5]).read_bytes()).hexdigest(),
      'baseline/enabled/forced passed')
print('\n'.join(outputs['enabled']))
PY
    done
    for operation in 0 1 2 3 4 5 6 7; do
      for mode in baseline enabled; do
        enabled=0
        if [[ $mode == enabled ]]; then enabled=1; fi
        BIRNAM_ID_DYLIB="$floor" BIRNAM_TEST_CLOSURE_CONTROL=1 \
          BIRNAM_CLOSURE_PRECISE=$precise BIRNAM_CLOSURE_API_OPERATION=$operation \
          BIRNAM_DEV_CLOSURE_ELISION=$enabled BIRNAM_DEV_CLOSURE_FORCE_RECOVERY=0 \
          BIRNAM_DEV_CLOSURE_REPORT=1 "$prefix/bin/birnam" scripts/fixtures/closure-live-api.birnam \
          >"$work/$mode.api" 2>&1
      done
      python3 - "$work" "$profile" "$precise" "$operation" <<'PY'
from pathlib import Path
import sys
work = Path(sys.argv[1])
operation = int(sys.argv[4])
outputs = {}
for mode in ('baseline', 'enabled'):
    lines = (work / (mode + '.api')).read_text().splitlines()
    for phase in ('override', 'restored'):
        begin = lines.index('closure-live-api: ' + phase + ' begin')
        end = lines.index('closure-live-api: ' + phase + ' end')
        hits = [line for line in lines[begin + 1:end] if line.startswith('closure-elision: ')]
        expected = (['closure-elision: hit materialized=true']
                    if mode == 'enabled' and (phase == 'restored' or operation == 0) else [])
        assert hits == expected, (operation, mode, phase, hits, expected)
    outputs[mode] = [line for line in lines if not line.startswith('closure-elision: ')]
assert outputs['baseline'] == outputs['enabled'], outputs
print('closure-live-api-smoke:', 'profile=' + sys.argv[2], 'precise=' + sys.argv[3],
      'operation=' + sys.argv[4], 'callback trace and restored admission passed')
print(next(line for line in outputs['enabled'] if line.startswith('closure-live-api: operation=')))
PY
    done
  done
