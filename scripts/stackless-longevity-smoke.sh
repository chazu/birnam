#!/usr/bin/env bash
# Run Design 60H's fresh-process, image-chained longevity gate.
set -euo pipefail

cd "$(dirname "$0")/.."

prefix=${1:?usage: stackless-longevity-smoke.sh PREFIX EVIDENCE_DIR}
evidence_dir=${2:?usage: stackless-longevity-smoke.sh PREFIX EVIDENCE_DIR}
epochs=${BIRNAM_LONGEVITY_EPOCHS:-20}
if ((epochs < 1)); then
  printf 'longevity epochs must be positive\n' >&2
  exit 2
fi
mkdir -p "$evidence_dir"
bin="$prefix/bin/birnam"
test -x "$bin"

seed="$evidence_dir/epoch-000.image"
printf '[System saveImage: "%s"];\n:quit\n' "$seed" | "$bin" --repl \
  >"$evidence_dir/seed.out" 2>"$evidence_dir/seed.err"
test ! -s "$evidence_dir/seed.err"
test -s "$seed"

previous=$seed
for ((epoch = 1; epoch <= epochs; epoch++)); do
  printf -v ordinal '%03d' "$epoch"
  next="$evidence_dir/epoch-${ordinal}.image"
  metrics="$evidence_dir/epoch-${ordinal}.json"
  log="$evidence_dir/epoch-${ordinal}.log"
  timing="$evidence_dir/epoch-${ordinal}.time"
  if ((epoch % 2 == 1)); then
    mode=mapped
  else
    mode=rebuild
  fi
  /usr/bin/time -l env \
    BIRNAM_CHURN_INPUT_IMAGE="$previous" \
    BIRNAM_CHURN_OUTPUT_IMAGE="$next" \
    BIRNAM_CHURN_RECOVERY_MODE="$mode" \
    BIRNAM_CHURN_EVIDENCE_JSON="$metrics" \
    scripts/runtime-bounded-churn-smoke.sh "$prefix" \
    >"$log" 2>"$timing"
  test -s "$next"
  test -s "$metrics"
  rss=$(awk '/maximum resident set size/ {print $1; exit}' "$timing")
  test -n "$rss"
  python3 - "$metrics" "$epoch" "$rss" "$previous" "$next" "$log" <<'PY'
import hashlib
import json
import sys

def digest(path):
    value = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()

path, epoch, rss, previous, current, log = sys.argv[1:]
with open(path, encoding="utf-8") as handle:
    evidence = json.load(handle)
evidence.update({
    "epoch": int(epoch),
    "maximum_resident_bytes": int(rss),
    "input_image_sha256": digest(previous),
    "output_image_sha256": digest(current),
    "complete_output_sha256": digest(log),
})
with open(path, "w", encoding="utf-8") as handle:
    json.dump(evidence, handle, sort_keys=True, indent=2)
    handle.write("\n")
PY
  previous=$next
done

python3 scripts/check-stackless-longevity.py "$evidence_dir" "$epochs"

printf 'stackless-longevity-smoke: %d mapped/rebuilt fresh-process epochs passed\n' "$epochs"
