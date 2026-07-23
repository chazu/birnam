#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
cycles=${1:-2}
if ! [[ $cycles =~ ^[1-9][0-9]*$ ]]; then
  echo "usage: $0 [positive-cycle-count]" >&2
  exit 2
fi

scratch=$(mktemp -d /tmp/birnam-repeat-floor-verify.XXXXXX)
trap 'rm -rf "$scratch"' EXIT

run_gate() {
  local cycle=$1
  local floor=$2
  local log="$scratch/cycle-$cycle-$floor.log"
  if ! bash "$repo/scripts/floor-verify.sh" "$floor" >"$log" 2>&1; then
    echo "repeat-floor-verification: cycle $cycle $floor failed" >&2
    tail -120 "$log" >&2
    return 1
  fi
  local selected
  selected=$(grep "^verify-floor-$floor:" "$log" | tail -1)
  echo "repeat-floor-verification: cycle $cycle $selected"
}

for ((cycle = 1; cycle <= cycles; cycle += 1)); do
  if (( cycle % 2 == 1 )); then
    run_gate "$cycle" zig
    run_gate "$cycle" rust
  else
    run_gate "$cycle" rust
    run_gate "$cycle" zig
  fi
done

echo "repeat-floor-verification: $cycles clean cycles passed in alternating floor order"
