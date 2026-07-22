#!/usr/bin/env bash
# Opt-in compatibility proof against committed pre-rewrite artifacts.
set -euo pipefail
cd "$(dirname "$0")"

floor=${BIRNAM_ID_DYLIB:?BIRNAM_ID_DYLIB must name exactly one floor dylib}
bin=${BIRNAM_BIN:?BIRNAM_BIN must name the birnam executable}

shasum -a 256 -c SHA256SUMS

client_output=$(./idapi-v1-client.arm64-macos "$floor" ./idapi-v1-module.arm64-macos.dylib)
grep -q '^idapi-v1-client: ok fields=54 module_calls=2 ' <<<"$client_output"

probe='[Globals at: #IdApiV1Scalar]
[IdApiV1Receiver fixtureProbe]
[21 fixtureDouble]
[IdApiV1Deferred value: 2]
[[Globals at: #IdApiV1Vector] size]
[[Globals at: #IdApiV1Map] at: #key]
:quit
'

assert_probe() {
  local output=$1
  local actual
  local expected
  actual=$(sed -n 's/.*birnam> \([0-9][0-9]*\)$/\1/p' <<<"$output")
  expected=$'41\n73\n42\n42\n3\n9'
  if [[ "$actual" != "$expected" ]]; then
    printf 'unexpected frozen-image probe:\n%s\n' "$actual" >&2
    return 1
  fi
}

mapped=$(printf '%b' "$probe" | "$bin" pre-rewrite.image)
assert_probe "$mapped"

rebuilt=$(printf '%b' "$probe" | "$bin" --rebuild pre-rewrite.image)
assert_probe "$rebuilt"

echo "idapi-v1-fixtures: frozen native client and image passed"
