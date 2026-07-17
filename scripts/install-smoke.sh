#!/usr/bin/env bash
# Verify the installed tree, isolated from checkout-relative module fallbacks.
set -euo pipefail
cd "$(dirname "$0")/.."

prefix=${BIRNAM_PREFIX:-zig-out}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/birnam"
cp -R "$prefix/bin" "$prefix/lib" "$prefix/share" "$tmp/birnam/"

bin="$tmp/birnam/bin/birnam"
work="$tmp/work"
mkdir -p "$work"
cd "$work"
unset BIRNAM_MODULES_DIR BIRNAM_LIB_DIR BIRNAM_ID_DYLIB

"$bin" doc time nowMillis | grep -q 'nowMillis'
"$bin" doc image saveImage: | grep -q 'saveImage:'
"$bin" docgen reference.html >/dev/null
grep -q 'id="guide-01-language"' reference.html
grep -q 'id="guide-04-images"' reference.html
grep -q 'System saveImage:' reference.html
grep -q 'Blocking process isolates' reference.html
"$bin" --lsp </dev/null >/dev/null

echo "install-smoke: doc, Zig docs, guide docgen, and LSP passed"
