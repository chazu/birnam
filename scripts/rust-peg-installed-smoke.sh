#!/usr/bin/env bash
# Exercise the installed, relocated compiler after swapping only the PEG dylib.
set -euo pipefail

cd "$(dirname "$0")/.."
prefix=${BIRNAM_PREFIX:-$HOME/.local/opt/birnam}
test -x "$prefix/bin/birnam"
test -f "$prefix/lib/libbirnam_id_peg_rust.dylib"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/birnam"
cp -R "$prefix/bin" "$prefix/lib" "$prefix/share" "$tmp/birnam/"
cp "$tmp/birnam/lib/libbirnam_id_peg_rust.dylib" "$tmp/birnam/lib/libbirnam_id_peg.dylib"

cat >"$tmp/probe.birnam" <<'EOF'
[System println: [[1 + 2] printString]]
EOF

bin="$tmp/birnam/bin/birnam"
work="$tmp/work"
mkdir -p "$work"
(
  cd "$work"
  unset BIRNAM_LIB_DIR BIRNAM_ID_DYLIB
  output=$("$bin" "$tmp/probe.birnam")
  test "$output" = "3"
)

echo "rust-peg-installed: relocated source compile/eval passed"
