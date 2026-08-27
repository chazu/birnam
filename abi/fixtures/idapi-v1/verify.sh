#!/usr/bin/env bash
# Opt-in native IdApi compatibility proof against committed pre-rewrite artifacts.
set -euo pipefail
cd "$(dirname "$0")"

floor=${BIRNAM_ID_DYLIB:?BIRNAM_ID_DYLIB must name exactly one floor dylib}
bin=${BIRNAM_BIN:?BIRNAM_BIN must name the birnam executable}

# The archived v4 image remains checksum evidence but is no longer admitted by
# current runtimes. This gate executes only the frozen native IdApi client and
# its companion module.
shasum -a 256 -c SHA256SUMS

client_output=$(./idapi-v1-client.arm64-macos "$floor" ./idapi-v1-module.arm64-macos.dylib)
grep -q '^idapi-v1-client: ok fields=54 module_calls=2 ' <<<"$client_output"

if mapped_output=$("$bin" pre-rewrite.image </dev/null 2>&1); then
  echo "archived v4 image unexpectedly mapped" >&2
  exit 1
fi
grep -Fq 'birnam: image: LoadRejected' <<<"$mapped_output"

if rebuilt_output=$("$bin" --rebuild pre-rewrite.image </dev/null 2>&1); then
  echo "archived v4 image unexpectedly rebuilt" >&2
  exit 1
fi
grep -Fq 'birnam: image: LoadRejected' <<<"$rebuilt_output"

echo "idapi-v1-fixtures: frozen native client/module passed; archived v4 image rejected"
