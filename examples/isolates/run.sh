#!/usr/bin/env bash
set -euo pipefail

bin="${BIRNAM_BIN:-zig-out/bin/birnam}"
export BIRNAM_LIB_DIR="${BIRNAM_LIB_DIR:-zig-out/lib}"
image=/tmp/birnam-isolate-example.image
replacement_v1=/tmp/birnam-isolate-replacement-example-v1.image
replacement_v2=/tmp/birnam-isolate-replacement-example-v2.image
trap 'rm -f "$image" "$replacement_v1" "$replacement_v2"' EXIT

"$bin" examples/isolates/build-image.birnam
if [[ ! -f "$image" ]]; then
  printf 'worker image was not created: %s\n' "$image" >&2
  exit 1
fi

for example in copied-values two-workers failures; do
  printf '\n== %s ==\n' "$example"
  "$bin" "examples/isolates/$example.birnam"
done

printf '\n== rolling replacement ==\n'
"$bin" examples/isolates/build-replacement-v1.birnam >/dev/null
"$bin" examples/isolates/build-replacement-v2.birnam >/dev/null
"$bin" examples/isolates/replacement.birnam
