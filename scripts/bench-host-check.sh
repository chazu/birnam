#!/usr/bin/env bash
set -euo pipefail

minimum_median_idle=${BENCH_HOST_MIN_MEDIAN_IDLE:-60}
minimum_sample_idle=${BENCH_HOST_MIN_SAMPLE_IDLE:-50}
maximum_idle_spread=${BENCH_HOST_MAX_IDLE_SPREAD:-15}
maximum_process_cpu=${BENCH_HOST_MAX_PROCESS_CPU:-80}

valid_number() {
  awk -v value="$1" 'BEGIN {exit !(value ~ /^[0-9]+([.][0-9]+)?$/)}'
}

host_is_quiet() {
  local minimum=$1 median=$2 maximum=$3 spread=$4 hottest_cpu=$5

  awk \
    -v minimum="$minimum" \
    -v median="$median" \
    -v spread="$spread" \
    -v hottest="$hottest_cpu" \
    -v minimum_sample="$minimum_sample_idle" \
    -v minimum_median="$minimum_median_idle" \
    -v maximum_spread="$maximum_idle_spread" \
    -v maximum_process="$maximum_process_cpu" \
    'BEGIN {exit !(minimum >= minimum_sample && median >= minimum_median && spread <= maximum_spread && hottest <= maximum_process)}'
}

self_test() {
  host_is_quiet 62 65 68 6 40
  if host_is_quiet 49 65 68 19 40; then return 1; fi
  if host_is_quiet 52 59 63 11 40; then return 1; fi
  if host_is_quiet 52 65 69 17 40; then return 1; fi
  if host_is_quiet 62 65 68 6 81; then return 1; fi
  echo "bench-host-check: self-test passed"
}

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

if ! host_is_quiet "$minimum" "$median" "$maximum" "$spread" "$hottest_cpu"; then
  echo "bench-host-check: $context host is not stable enough for tracked timing" >&2
  echo "  idle samples: ${samples[*]}% (minimum ${minimum}%, median ${median}%, spread ${spread} points)" >&2
  echo "  required: minimum ${minimum_sample_idle}%, median ${minimum_median_idle}%, spread <= ${maximum_idle_spread} points" >&2
  echo "  hottest process: $hottest (limit ${maximum_process_cpu}% of one core)" >&2
  ps -axo pid,%cpu,comm | sort -k2,2nr | sed -n '1,8p' >&2
  exit 1
fi

echo "bench-host-check: $context stable on AC power; idle ${samples[*]}%, median ${median}%, spread ${spread} points, hottest ${hottest_cpu}%"
