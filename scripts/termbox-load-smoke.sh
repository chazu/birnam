#!/usr/bin/env bash
# Verify the reusable Termbox adapter without opening a terminal.
set -euo pipefail
cd "$(dirname "$0")/.."

bin="${BIRNAM_BIN:-${BIRNAM_PREFIX:-$HOME/.local/opt/birnam}/bin/birnam}"
root="$(pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cc -O2 -dynamiclib -o "$tmp/libtermbox2.dylib" wrapped/termbox2/termbox2.c

runner="$tmp/load-termbox.birnam"
cat >"$runner" <<EOF
[Alien libraryDir: "$tmp"];
[System load: "$root/wrapped/termbox2/termbox2.birnam"];
[System println: ["TERMBOX-LOAD:" , [[Globals includes: #Termbox] printString]]]
EOF

"$bin" "$runner" | grep -q '^TERMBOX-LOAD:true$'
echo "termbox-load-smoke: Alien adapter loaded"
