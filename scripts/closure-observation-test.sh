#!/usr/bin/env bash
# Exercise live closure aliases and nested collection through the source Host.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?installed prefix required}
work=$(mktemp -d "${TMPDIR:-/tmp}/birnam-closure-observation.XXXXXX")
trap 'rm -rf "$work"' EXIT
for precise in 0 1; do
  BIRNAM_CLOSURE_PRECISE=$precise "$prefix/bin/birnam" scripts/fixtures/closure-observation.birnam >"$work/output" 2>"$work/error"
  test ! -s "$work/error"
  grep -Fxq 'closure-observation: live aliases, captures, arguments, nested entry, ordered effects and collecting eviction passed' "$work/output"
done
echo 'closure-observation: conservative and precise collection passed'
