#!/usr/bin/env bash
# C0a: prove incomplete Rust Codegen selection fails closed with the exact
# diagnostic before the build graph can install or run a partial authority.
set -euo pipefail

cd "$(dirname "$0")/.."

scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-codegen-refusal.XXXXXX")
trap 'rm -rf "$scratch"' EXIT

prefix="$scratch/prefix"
cache="$scratch/cache"
global_cache="$scratch/global-cache"
output="$scratch/build.out"

if zig build install \
  -Dhost-impl=rust \
  -Dcodegen-impl=rust \
  --prefix "$prefix" \
  --cache-dir "$cache" \
  --global-cache-dir "$global_cache" >"$output" 2>&1
then
  echo "codegen-selection-refusal: incomplete Rust Codegen was accepted" >&2
  exit 1
fi

grep -Fq 'codegen-impl=rust is not yet selectable' "$output"
if [ -e "$prefix/lib/libbirnam_id_codegen.dylib" ]; then
  echo "codegen-selection-refusal: refused build left a canonical artifact" >&2
  exit 1
fi

echo "codegen-selection-refusal: exact fail-closed diagnostic passed"
