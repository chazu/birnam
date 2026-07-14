#!/usr/bin/env bash
set -euo pipefail

bin="${BIRNAM_BIN:-zig-out/bin/birnam}"
export BIRNAM_LIB_DIR="${BIRNAM_LIB_DIR:-zig-out/lib}"
image=/tmp/birnam-isolate-example.image
trap 'rm -f "$image"' EXIT

"$bin" examples/isolates/build-image.birnam
if [[ ! -f "$image" ]]; then
  printf 'worker image was not created: %s\n' "$image" >&2
  exit 1
fi

for example in copied-values two-workers failures; do
  printf '\n== %s ==\n' "$example"
  "$bin" "examples/isolates/$example.birnam"
done
