#!/usr/bin/env bash
# Real installed Codegen with an explicitly selected test-only floor bridge.
# The production floor and installation remain intact and separately verified.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?installed prefix required}
work=$(mktemp -d "${TMPDIR:-/tmp}/birnam-closure-control.XXXXXX")
trap 'rm -rf "$work"' EXIT
cat >"$work/absent.birnam" <<'EOF'
[System println: [Globals includes: #ClosureControlProbe]];
EOF
env -u BIRNAM_ID_DYLIB BIRNAM_TEST_CLOSURE_CONTROL=1 \
  "$prefix/bin/birnam" "$work/absent.birnam" >"$work/absent.out"
grep -Fxq false "$work/absent.out"
for profile in debug release; do
  flags=()
  if [[ $profile == release ]]; then flags+=(--release); fi
  CARGO_TARGET_DIR="$PWD/.cargo-target/74t-closure-control-target" mise exec -- \
    cargo build --locked --offline -p birnam-id --features ffi-test-hooks "${flags[@]}"
  floor="$PWD/.cargo-target/74t-closure-control-target/$profile/libbirnam_id.dylib"
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
    expected = ([] if mode == 'baseline' else ['closure-elision: hit materialized=true']
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
  done
done
