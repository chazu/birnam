#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <null-args-child>" >&2
  exit 2
fi

set +e
output=$("$1" 2>&1)
status=$?
set -e

if [ "$status" -eq 0 ]; then
  echo "null args with nonzero count unexpectedly succeeded" >&2
  exit 1
fi
grep -q 'IdApi.send args is null with nonzero count' <<<"$output"
echo "IdApi.send null/nonzero contract: correctly rejected in child"
