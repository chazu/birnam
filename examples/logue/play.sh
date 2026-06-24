#!/bin/sh
# Build the wrapped termbox2 dylib (if needed) and launch logue.
# Uses the birnam on PATH; from this repo, `zig build` puts one in zig-out/bin.
set -e
cd "$(dirname "$0")"
BIRNAM="${BIRNAM:-birnam}"
"$BIRNAM" wrap termbox2
exec "$BIRNAM" wrapped/termbox2/termbox2.birnam logue-core.birnam logue.birnam
