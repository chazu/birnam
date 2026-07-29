#!/usr/bin/env bash
set -euo pipefail

bin="${BIRNAM_BIN:-${BIRNAM_PREFIX:-$HOME/.local}/bin/birnam}"
image=/tmp/birnam-isolate-smoke.image
trap 'rm -f "$image"' EXIT
rm -f "$image"

"$bin" scripts/isolate-fixture.birnam >/dev/null
test -s "$image"
result=$("$bin" scripts/isolate-smoke.birnam)
test "$result" = $'42\ntwo'
failures=$("$bin" scripts/isolate-failures-smoke.birnam)
test "$failures" = $'#application\n#timeout\n#exit'
