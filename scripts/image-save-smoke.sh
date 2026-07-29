#!/usr/bin/env bash
# Exercise the script host's image-save path. This catches regressions where a
# script evaluates successfully but has not published the active generation
# needed to classify module-owned globals during save.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
bin=${BIRNAM_BIN:-${BIRNAM_PREFIX:-$HOME/.local}/bin/birnam}
libdir=${BIRNAM_LIB_DIR:-${BIRNAM_PREFIX:-$HOME/.local}/lib}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/bench/results/dev"
(
  cd "$tmp"
  BIRNAM_LIB_DIR="$libdir" \
  BIRNAM_BENCH_SOURCE="$root/modules/bench.birnam" \
  BENCH_IMG="$tmp/bench.image" \
  BENCH_OUT="$tmp/image.json" \
  "$bin" "$root/bench/image.birnam" </dev/null
)

test -s "$tmp/bench.image"
echo "image-save-smoke: script image save passed"
