#!/bin/sh
# logue launcher — the interactive roguelike.
#
# Needs a REAL terminal: termbox2 opens /dev/tty, so this cannot run headless
# (under a pipe / no controlling tty, tb_init fails and there is nothing to play).
# The full game LOGIC is exercised headlessly by logue-test.birnam (no termbox):
#
#   birnam logue-test.birnam        # 28 assertions, green
#
# Uses the Rust birnam on PATH; from this repo, `mise run build` installs the
# development prefix under ~/.local by default (run as
# BIRNAM="$HOME/.local/bin/birnam" ./play.sh from here).
set -e
cd "$(dirname "$0")"
BIRNAM="${BIRNAM:-birnam}"
# Build the termbox2 dylib on first run (the wrapper dlopens it).
[ -f ../../wrapped/termbox2/libtermbox2.dylib ] || sh ../../wrapped/termbox2/build.sh
exec "$BIRNAM" logue.birnam
