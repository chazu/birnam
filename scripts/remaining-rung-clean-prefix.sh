#!/usr/bin/env bash
# R0 clean-prefix proof for the remaining-rung manifest.
# Build/install into an isolated prefix, relocate it, add decoy dylibs to the
# working directory, and prove runtime resolution follows only the executable's
# declared resource root.
set -euo pipefail

cd "$(dirname "$0")/.."

scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-remaining-rungs.XXXXXX")
trap 'rm -rf "$scratch"' EXIT

prefix="$scratch/prefix"
cache="$scratch/cache"
global_cache="$scratch/global-cache"
zig build install \
  --prefix "$prefix" \
  --cache-dir "$cache" \
  --global-cache-dir "$global_cache" \
  -Doptimize=Debug \
  --summary all >/dev/null

test -x "$prefix/bin/birnam"
test -f "$prefix/lib/libbirnam_id_prelude.dylib"

relocated="$scratch/relocated"
mkdir -p "$relocated"
cp -R "$prefix/bin" "$prefix/lib" "$prefix/share" "$relocated/"

work="$scratch/decoy-workdir"
mkdir -p "$work"
for dylib in \
  libbirnam_id_prelude.dylib \
  libbirnam_id_collection.dylib \
  libbirnam_id_error.dylib \
  libbirnam_id_isolate.dylib \
  libbirnam_id_relational.dylib \
  libbirnam_id_alien.dylib \
  libbirnam_id_image.dylib
do
  printf 'cwd decoy; runtime must not open this file\n' >"$work/$dylib"
done

cd "$work"
unset BIRNAM_LIB_DIR BIRNAM_ID_DYLIB DYLD_LIBRARY_PATH
"$relocated/bin/birnam" --version >/dev/null
"$relocated/bin/birnam" doc image saveImage: | grep -q 'saveImage:'
"$relocated/bin/birnam" --lsp </dev/null >/dev/null

echo "remaining-rung-clean-prefix: relocated runtime ignored cwd decoys"
