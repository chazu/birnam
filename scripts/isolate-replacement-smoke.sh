#!/usr/bin/env bash
set -euo pipefail

root=$(pwd)
bin="${BIRNAM_BIN:-$root/zig-out/bin/birnam}"

cleanup() {
  rm -f /tmp/birnam-isolate-replacement-v1.image \
    /tmp/birnam-isolate-replacement-v2.image \
    /tmp/birnam-isolate-replacement-corrupt.image
}
trap cleanup EXIT
cleanup
printf 'not an image\n' >/tmp/birnam-isolate-replacement-corrupt.image
"$bin" scripts/isolate-replacement-v1.birnam >/dev/null
"$bin" scripts/isolate-replacement-v2.birnam >/dev/null
result=$("$bin" scripts/isolate-replacement-smoke.birnam)
grep -q 'isolate-replacement-smoke: I1b failure matrix, distinct baselines, and A-to-B-to-C replacement passed' <<<"$result"
printf '%s\n' "$result"

cleanup_result=$(BIRNAM_TEST_ISOLATE_FAULTS=hard-stop,reap-pending,fast-escalation \
  "$bin" scripts/isolate-replacement-cleanup-smoke.birnam)
grep -q 'isolate-replacement-cleanup-smoke: TERM/KILL, sole pending owner, later reap, and selected B passed' <<<"$cleanup_result"
printf '%s\n' "$cleanup_result"
