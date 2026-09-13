#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?installed prefix required}
work=$(mktemp -d "${TMPDIR:-/tmp}/birnam-closure-smoke.XXXXXX")
trap 'rm -rf "$work"' EXIT
env -u BIRNAM_DEV_CLOSURE_ELISION -u BIRNAM_DEV_CLOSURE_REPORT -u BIRNAM_DEV_CLOSURE_FORCE_RECOVERY \
  "$prefix/bin/birnam" scripts/fixtures/closure-elision.birnam >"$work/baseline" 2>"$work/baseline.err"
BIRNAM_DEV_CLOSURE_ELISION=1 BIRNAM_DEV_CLOSURE_REPORT=1 BIRNAM_DEV_CLOSURE_FORCE_RECOVERY=0 \
  "$prefix/bin/birnam" scripts/fixtures/closure-elision.birnam >"$work/enabled" 2>"$work/enabled.err"
BIRNAM_DEV_CLOSURE_ELISION=1 BIRNAM_DEV_CLOSURE_REPORT=1 BIRNAM_DEV_CLOSURE_FORCE_RECOVERY=1 \
  "$prefix/bin/birnam" scripts/fixtures/closure-elision.birnam >"$work/recovery" 2>"$work/recovery.err"
cmp "$work/baseline" "$work/enabled"
cmp "$work/baseline" "$work/recovery"
test ! -s "$work/baseline.err"
grep -Fxq 'closure-elision: hit materialized=false' "$work/enabled.err"
grep -Fxq 'closure-elision: hit materialized=true' "$work/enabled.err"
grep -Fxq 'closure-elision: forced constructor recovery' "$work/recovery.err"
cat "$work/enabled"
echo "closure-elision-smoke: allocation elimination, escaping recovery and forced recovery executed"
