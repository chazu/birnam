#!/bin/sh
# Build libtermbox2.dylib from the int-clean adapter. termbox2 is a single-header
# library; termbox2.c defines TB_IMPL to instantiate it, plus the `lg_*` adapter
# the birnam wrapper (termbox2.birnam) binds. Uses the system C compiler.
#
# How wrapped-lib builds should work in general is still open; for now each lib
# carries its own build.sh next to its .c.
set -e
cd "$(dirname "$0")"
cc -O2 -dynamiclib -o libtermbox2.dylib termbox2.c
echo "built $(pwd)/libtermbox2.dylib"
