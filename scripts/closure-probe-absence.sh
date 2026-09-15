#!/usr/bin/env bash
# The diagnostic opt-in must not expose a test bridge in a normal installation.
set -euo pipefail
prefix=${1:?prefix}
work=$(mktemp -d "${TMPDIR:-/tmp}/birnam-closure-absence.XXXXXX")
trap 'rm -rf "$work"' EXIT
cat >"$work/absent.birnam" <<'SOURCE'
[System println: [Globals includes: #ClosureControlProbe]];
SOURCE
env -u BIRNAM_ID_DYLIB BIRNAM_TEST_CLOSURE_CONTROL=1 \
  "$prefix/bin/birnam" "$work/absent.birnam" >"$work/absent.out"
grep -Fxq false "$work/absent.out"
