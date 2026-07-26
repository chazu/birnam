#!/usr/bin/env bash
# Clean-process driver for one selected native rung or the complete set of
# currently migrated production choices.
set -euo pipefail

cd "$(dirname "$0")/.."

mode=${1:-all}
optimize=${BIRNAM_NATIVE_RUNG_OPTIMIZE:-Debug}
case "$mode" in
  all|--all-migrated|--rung=artifact-inspector=zig) ;;
  *)
    echo "usage: $0 [all|--all-migrated|--rung=artifact-inspector=zig]" >&2
    exit 2
    ;;
esac

scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-native-rung-harness.XXXXXX")
trap 'rm -rf "$scratch"' EXIT

run_selected_artifact() {
  local root="$scratch/selected-artifact"
  mkdir -p "$root/tmp"
  TMPDIR="$root/tmp" zig build test-artifact test-native-rung-singleton \
    -Dartifact-inspector-impl=zig \
    -Doptimize="$optimize" \
    -j1 \
    --prefix "$root/prefix" \
    --cache-dir "$root/cache" \
    --global-cache-dir "$root/global-cache"
}

run_all_migrated() {
  local root="$scratch/all-migrated"
  mkdir -p "$root/tmp"
  TMPDIR="$root/tmp" zig build test-install \
    -Dartifact-inspector-impl=zig \
    -Doptimize="$optimize" \
    -j1 \
    --prefix "$root/prefix" \
    --cache-dir "$root/cache" \
    --global-cache-dir "$root/global-cache"
}

case "$mode" in
  --rung=artifact-inspector=zig) run_selected_artifact ;;
  --all-migrated) run_all_migrated ;;
  all)
    run_selected_artifact
    run_all_migrated
    ;;
esac

echo "native-rung-harness: clean selected-rung and all-production runs passed"
