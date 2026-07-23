#!/usr/bin/env bash
# Assert that one child terminates with SIGABRT and emits a required diagnostic.
set -euo pipefail

if [ "$#" -lt 3 ]; then
  echo "usage: $0 <label> <diagnostic> <command> [args...]" >&2
  exit 2
fi

label=$1
diagnostic=$2
shift 2

output=$(mktemp "${TMPDIR:-/tmp}/birnam-child-abort.XXXXXX")
trap 'rm -f "$output"' EXIT

set +e
"$@" >"$output" 2>&1
status=$?
set -e

if [ "$status" -ne 134 ]; then
  echo "$label exited $status, expected SIGABRT status 134" >&2
  cat "$output" >&2
  exit 1
fi
if ! grep -Fq "$diagnostic" "$output"; then
  echo "$label omitted its required diagnostic: $diagnostic" >&2
  cat "$output" >&2
  exit 1
fi

echo "$label terminated with SIGABRT"
