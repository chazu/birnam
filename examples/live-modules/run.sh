#!/usr/bin/env bash
set -euo pipefail

bin="${BIRNAM_BIN:-zig-out/bin/birnam}"
export BIRNAM_LIB_DIR="${BIRNAM_LIB_DIR:-zig-out/lib}"
image=/tmp/birnam-live-example.image
trap 'rm -f "$image" /tmp/birnam-live-example-export.birnam /tmp/birnam-live-example-v2.birnam' EXIT

"$bin" examples/live-modules/build-baseline.birnam
test -f "$image"

printf '\n== mapped baseline boot ==\n'
"$bin" "$image"

printf '\n== rebuilt baseline boot ==\n'
"$bin" --rebuild "$image"

printf '\n== isolate from the baseline ==\n'
"$bin" examples/live-modules/request-worker.birnam
