#!/bin/sh
set -eu
cd "$(dirname "$0")"
if [ ! -t 0 ] || [ ! -t 1 ]; then
  echo 'presentation-repl needs a terminal; use birnam --repl for pipes' >&2
  exit 1
fi
if [ "${1:-}" = "--build" ]; then
  sh build.sh
elif [ ! -f .birnam/build/aarch64-macos/debug/lib/application.dylib ]; then
  sh build.sh
fi
exec "${BIRNAM:-birnam}" run .
