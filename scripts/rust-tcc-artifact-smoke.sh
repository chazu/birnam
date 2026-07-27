#!/usr/bin/env bash
# Compile an installable Birnam module with the selected TCC provider, then
# activate it from a fresh process. The second process proves the emitted dylib
# does not rely on the authoring process's TCC state or workspace paths.
set -euo pipefail

bin=${BIRNAM_BIN:?BIRNAM_BIN must point to birnam}
lib_dir=${BIRNAM_LIB_DIR:?BIRNAM_LIB_DIR must point to the installed lib dir}
tcc=${BIRNAM_TCC_DYLIB:-}
if [[ -z "$tcc" ]]; then
  tcc=${BIRNAM_RUST_TCC_DYLIB:-}
fi
: "${tcc:?BIRNAM_TCC_DYLIB or BIRNAM_RUST_TCC_DYLIB must point to TCC}"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
src="$tmp/tcc-smoke.birnam"
artifact="$tmp/libtcc-smoke.dylib"
work="$tmp/child"
mkdir -p "$work"

cat >"$src" <<'EOF'
[Module named: #tccSmoke requires: #[]];
EOF

(
  cd "$work"
  BIRNAM_LIB_DIR="$lib_dir" \
    BIRNAM_TCC_DYLIB="$tcc" \
    "$bin" --compile-module "$src" "$artifact"
)
test -s "$artifact"

(
  cd "$work"
  BIRNAM_LIB_DIR="$lib_dir" \
    BIRNAM_TCC_DYLIB="$tcc" \
    "$bin" "$artifact" >/dev/null
)

echo "rust-tcc-artifact: separate-process AOT activation passed"
