#!/usr/bin/env bash
set -euo pipefail

minimum_idle=${1:-75}
context=${2:-benchmark}

power=$(pmset -g batt | head -1)
if [[ $power != *"AC Power"* ]]; then
  echo "bench-host-check: $context requires AC power; got: $power" >&2
  exit 1
fi

cpu_line=$(top -l 2 -n 0 -stats pid 2>/dev/null | awk '/CPU usage/ {line=$0} END {print line}')
idle=$(awk '{value=$7; sub(/%/, "", value); print value}' <<<"$cpu_line")
if [[ -z $idle ]] || ! awk -v idle="$idle" -v minimum="$minimum_idle" 'BEGIN {exit !(idle >= minimum)}'; then
  echo "bench-host-check: $context requires ${minimum_idle}% idle CPU; got ${idle:-unknown}%" >&2
  ps -axo pid,%cpu,comm | sort -k2 -nr | head -8 >&2
  exit 1
fi

echo "bench-host-check: $context on AC power with ${idle}% idle CPU"
