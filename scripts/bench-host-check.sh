#!/usr/bin/env bash
set -euo pipefail

minimum_median_idle=${BENCH_HOST_MIN_MEDIAN_IDLE:-50}
maximum_process_cpu=${BENCH_HOST_MAX_PROCESS_CPU:-80}
minimum_interval_idle=${BENCH_HOST_MIN_INTERVAL_IDLE:-30}
maximum_low_sample_percent=${BENCH_HOST_MAX_LOW_SAMPLE_PERCENT:-20}

valid_number() {
  awk -v value="$1" 'BEGIN {exit !(value ~ /^[0-9]+([.][0-9]+)?$/)}'
}

host_is_quiet() {
  local median=$1 hottest_cpu=$2

  awk \
    -v median="$median" \
    -v hottest="$hottest_cpu" \
    -v minimum_median="$minimum_median_idle" \
    -v maximum_process="$maximum_process_cpu" \
    'BEGIN {exit !(median >= minimum_median && hottest <= maximum_process)}'
}

intervals_are_quiet() {
  local count=$1 median=$2 low_count=$3
  awk \
    -v count="$count" \
    -v median="$median" \
    -v low_count="$low_count" \
    -v minimum_median="$minimum_median_idle" \
    -v maximum_low_percent="$maximum_low_sample_percent" \
    'BEGIN {exit !(median >= minimum_median && low_count * 100 <= count * maximum_low_percent)}'
}

self_test() {
  host_is_quiet 65 40
  host_is_quiet 52 40 # one transient sample does not veto a quiet majority
  if host_is_quiet 49 40; then return 1; fi
  if host_is_quiet 65 81; then return 1; fi
  intervals_are_quiet 10 55 2
  if intervals_are_quiet 10 55 3; then return 1; fi
  if intervals_are_quiet 10 49 0; then return 1; fi
  echo "bench-host-check: self-test passed"
}

if [[ ${1:-} == --samples ]]; then
  sample_file=$2
  context=${3:-benchmark}
  samples=()
  while IFS= read -r idle; do
    samples+=("$idle")
  done < <(
    awk '
      /CPU usage/ {
        value = $(NF - 1)
        sub(/%/, "", value)
        print value
      }
    ' "$sample_file"
  )
  if [[ ${#samples[@]} -eq 0 ]]; then
    echo "bench-host-check: $context monitor produced no CPU samples" >&2
    exit 1
  fi

  # The first streaming top sample is cumulative. A command that completes
  # before the next one-second interval relies on its immediate preflight.
  samples=("${samples[@]:1}")
  if [[ ${#samples[@]} -eq 0 ]]; then
    echo "bench-host-check: $context completed before one interval sample; preflight applies"
    exit
  fi
  for idle in "${samples[@]}"; do
    if ! valid_number "$idle"; then
      echo "bench-host-check: $context got an invalid interval sample: $idle" >&2
      exit 1
    fi
  done

  count=${#samples[@]}
  sorted=$(printf '%s\n' "${samples[@]}" | sort -n)
  median=$(awk -v rank=$((count / 2 + 1)) 'NR == rank {print; exit}' <<<"$sorted")
  low_count=$(awk -v minimum="$minimum_interval_idle" '$1 < minimum {count++} END {print count + 0}' <<<"$sorted")
  if ! intervals_are_quiet "$count" "$median" "$low_count"; then
    echo "bench-host-check: $context observed load during measurement" >&2
    echo "  interval samples: $count, median ${median}%, below ${minimum_interval_idle}%: $low_count" >&2
    echo "  required: median ${minimum_median_idle}%, at most ${maximum_low_sample_percent}% low intervals" >&2
    exit 1
  fi
  echo "bench-host-check: $context stayed quiet for $count intervals; median ${median}%, low intervals $low_count"
  exit
fi

if [[ ${1:-} == --wait ]]; then
  shift
  context=${1:-benchmark}
  wait_seconds=${BENCH_HOST_WAIT_SECONDS:-900}
  retry_seconds=${BENCH_HOST_RETRY_SECONDS:-10}
  deadline=$((SECONDS + wait_seconds))
  while ! "$0" "$context"; do
    if (( SECONDS >= deadline )); then
      echo "bench-host-check: $context did not become quiet within ${wait_seconds}s" >&2
      exit 1
    fi
    echo "bench-host-check: waiting ${retry_seconds}s before retrying $context" >&2
    sleep "$retry_seconds"
  done
  exit
fi

if [[ ${1:-} == --self-test ]]; then
  self_test
  exit
fi

context=${1:-benchmark}
power=$(pmset -g batt | sed -n '1p')
if [[ $power != *"AC Power"* ]]; then
  echo "bench-host-check: $context requires AC power; got: $power" >&2
  exit 1
fi

# The first macOS top sample is cumulative since boot. Discard it and use the
# following three interval samples so a transient instant cannot admit or
# reject a tracked benchmark.
samples=()
while IFS= read -r idle; do
  samples+=("$idle")
done < <(
  top -l 4 -n 0 -stats pid 2>/dev/null |
    awk '
      /CPU usage/ {
        seen++
        if (seen > 1) {
          value = $(NF - 1)
          sub(/%/, "", value)
          print value
        }
      }
    '
)

if [[ ${#samples[@]} -ne 3 ]]; then
  echo "bench-host-check: $context could not collect three CPU samples" >&2
  exit 1
fi
for idle in "${samples[@]}"; do
  if ! valid_number "$idle"; then
    echo "bench-host-check: $context got an invalid CPU sample: $idle" >&2
    exit 1
  fi
done

sorted=$(printf '%s\n' "${samples[@]}" | sort -n)
minimum=$(sed -n '1p' <<<"$sorted")
median=$(sed -n '2p' <<<"$sorted")
maximum=$(sed -n '3p' <<<"$sorted")
spread=$(awk -v minimum="$minimum" -v maximum="$maximum" \
  'BEGIN {printf "%.2f", maximum - minimum}')

hottest=$(ps -axo pid=,%cpu=,comm= | sort -k2,2nr | sed -n '1p')
hottest_cpu=$(awk '{print $2}' <<<"$hottest")
if ! valid_number "$hottest_cpu"; then
  echo "bench-host-check: $context could not inspect competing processes" >&2
  exit 1
fi

if ! host_is_quiet "$median" "$hottest_cpu"; then
  echo "bench-host-check: $context host is not stable enough for tracked timing" >&2
  echo "  idle samples: ${samples[*]}% (minimum ${minimum}%, median ${median}%, spread ${spread} points)" >&2
  echo "  required: median ${minimum_median_idle}% (two of three samples), hottest process <= ${maximum_process_cpu}% of one core" >&2
  echo "  hottest process: $hottest (limit ${maximum_process_cpu}% of one core)" >&2
  ps -axo pid,%cpu,comm | sort -k2,2nr | sed -n '1,8p' >&2
  exit 1
fi

echo "bench-host-check: $context stable on AC power; idle ${samples[*]}%, median ${median}%, spread ${spread} points, hottest ${hottest_cpu}%"
