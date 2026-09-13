#!/usr/bin/env bash
set -euo pipefail
unset BIRNAM_DEV_CLOSURE_FORCE_AT
cd "$(dirname "$0")/.."
prefix=${1:?installed prefix required}
work=$(mktemp -d "${TMPDIR:-/tmp}/birnam-closure-smoke.XXXXXX")
trap 'rm -rf "$work"' EXIT
for scenario in closure-elision:0 closure-observation:0 closure-observation:1; do
fixture=${scenario%:*}
export BIRNAM_CLOSURE_PRECISE=${scenario##*:}
env -u BIRNAM_DEV_CLOSURE_ELISION -u BIRNAM_DEV_CLOSURE_REPORT -u BIRNAM_DEV_CLOSURE_FORCE_RECOVERY \
  "$prefix/bin/birnam" "scripts/fixtures/$fixture.birnam" >"$work/baseline" 2>"$work/baseline.err"
BIRNAM_DEV_CLOSURE_ELISION=1 BIRNAM_DEV_CLOSURE_REPORT=1 BIRNAM_DEV_CLOSURE_FORCE_RECOVERY=0 \
  "$prefix/bin/birnam" "scripts/fixtures/$fixture.birnam" >"$work/enabled.combined" 2>&1
python3 - "$work" "$fixture" <<'PY'
from pathlib import Path
import sys
work = Path(sys.argv[1])
lines = (work / 'enabled.combined').read_text().splitlines(keepends=True)
(work / 'enabled').write_text(''.join(line for line in lines if not line.startswith(('closure-elision: hit ', 'closure-elision: completed calls='))))
(work / 'enabled.err').write_text(''.join(line for line in lines if line.startswith(('closure-elision: hit ', 'closure-elision: completed calls='))))
if sys.argv[2] == 'closure-observation':
    begin = lines.index('closure-observation: begin collecting caller\n')
    end = lines.index('closure-observation: end collecting caller\n')
    observed = lines[begin + 1:end]
    assert 'closure-elision: hit materialized=true\n' in observed, observed
    assert 'closure-elision: hit materialized=false\n' in observed, observed
    assert observed.count('closure-elision: completed calls=3\n') == 2, observed
PY
BIRNAM_DEV_CLOSURE_ELISION=1 BIRNAM_DEV_CLOSURE_REPORT=1 BIRNAM_DEV_CLOSURE_FORCE_RECOVERY=1 \
  "$prefix/bin/birnam" "scripts/fixtures/$fixture.birnam" >"$work/recovery" 2>"$work/recovery.err"
cmp "$work/baseline" "$work/enabled"
cmp "$work/baseline" "$work/recovery"
test ! -s "$work/baseline.err"
grep -Fxq 'closure-elision: hit materialized=false' "$work/enabled.err"
grep -Fxq 'closure-elision: hit materialized=true' "$work/enabled.err"
grep -Fxq 'closure-elision: forced constructor recovery' "$work/recovery.err"
cat "$work/enabled"
done
echo "closure-elision-smoke: allocation elimination, escaping recovery and forced recovery executed"
