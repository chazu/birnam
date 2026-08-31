#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
bin=${BIRNAM_BIN:-${BIRNAM_PREFIX:-$HOME/.local/opt/birnam}/bin/birnam}
libdir=${BIRNAM_LIB_DIR:-${BIRNAM_PREFIX:-$HOME/.local/opt/birnam}/lib}

test -f "$libdir/libbirnam_store.dylib"
BIRNAM_BIN="$bin" BIRNAM_LIB_DIR="$libdir" bash "$repo/scripts/image-save-smoke.sh"
BIRNAM_BIN="$bin" BIRNAM_LIB_DIR="$libdir" bash "$repo/scripts/repl-golden.sh"
echo "rust-store-stack: canonical Rust Store passed image-save and REPL integration"
