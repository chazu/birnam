#!/bin/sh
# logue launcher.
#
# The interactive renderer (logue.birnam) is currently BLOCKED: it needs the
# wrapped-C-library / termbox2 FFI subsystem, which has been retired from birnam
# (see the status note atop logue.birnam). Until that is revived over `Alien`,
# there is no playable terminal build.
#
# What DOES run today is the headless test suite, which exercises the full game
# logic (generation, combat, hunger, stairs, the macguffin + win condition):
#
#   ./play.sh            # runs logue-test.birnam
#
# Uses the birnam on PATH; from this repo, `zig build` puts one in zig-out/bin.
set -e
cd "$(dirname "$0")"
BIRNAM="${BIRNAM:-birnam}"
exec "$BIRNAM" logue-test.birnam
