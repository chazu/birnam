#!/usr/bin/env bash
# bench.sh — the benchmark driver (docs/design/46).
#
#   scripts/bench.sh               # dev run     -> bench/results/dev/bench-<sha>.json
#   scripts/bench.sh --release     # tracked run -> bench/results/v<VERSION>.json
#   BENCH_QUICK=1 scripts/bench.sh # the fast dev loop (fewer reps everywhere)
#
# Builds ReleaseSafe (the only tracked optimize mode), runs the birnam suites
# (micro / parse / relational / gc / image) plus the whole-process macro cases
# (boot paths, image boot, stage1 compile, scripts, docgen), and merges
# everything into ONE stamped JSON. Release runs replicate the micro and
# relational suites across 3 fresh processes (send cost varies ±8% between
# processes — layout effects; the compare tool medians across runs).
#
# For the every-edit loop, run a single suite directly instead:
#   BENCH_QUICK=1 ./zig-out/bin/birnam bench/micro.birnam
set -euo pipefail
cd "$(dirname "$0")/.."

MODE=dev
[ "${1:-}" = "--release" ] && MODE=release
QUICK=${BENCH_QUICK:-}

# A quick run must never become the kept-forever snapshot.
if [ $MODE = release ] && [ -n "$QUICK" ]; then
  echo "refusing: BENCH_QUICK with --release would overwrite the tracked snapshot with a low-rep run" >&2
  exit 1
fi

echo "==> build (ReleaseSafe)"
zig build install stage1 -Doptimize=ReleaseSafe
BIN=./zig-out/bin/birnam

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
FAIL=0

# ---------------------------------------------------------------------------
# birnam suites — each process writes its own JSON via BENCH_OUT.
# ---------------------------------------------------------------------------
SUITE_FILES=()

run_suite() { # run_suite <script> <label> <runs>
  local script=$1 label=$2 runs=$3 i out
  for i in $(seq 1 "$runs"); do
    out="$TMP/$label-$i.json"
    echo "==> suite $label (run $i/$runs)"
    if BENCH_OUT="$out" "$BIN" "$script" </dev/null; then
      SUITE_FILES+=("$out")
    else
      echo "FAIL suite $label run $i" >&2; FAIL=1
    fi
  done
}

REP_SUITES=1
[ $MODE = release ] && REP_SUITES=3

run_suite bench/micro.birnam micro $REP_SUITES
run_suite bench/parse.birnam parse 1
run_suite bench/relational.birnam relational $REP_SUITES
run_suite bench/gc.birnam gc 1
BENCH_IMG="$TMP/bench.image" run_suite bench/image.birnam image 1

# ---------------------------------------------------------------------------
# macro cases — whole-process wall time (python3 monotonic clock around a
# subprocess; BSD date has no sub-second resolution) + peak RSS from one
# extra /usr/bin/time -l run.
# ---------------------------------------------------------------------------
MACRO_JSON="$TMP/macro.jsonl"
: > "$MACRO_JSON"

wall_us() { # wall_us cmd... -> microseconds (or "FAIL rc")
  python3 -c '
import subprocess, sys, time
t0 = time.monotonic()
r = subprocess.run(sys.argv[1:], stdin=subprocess.DEVNULL,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
t1 = time.monotonic()
if r.returncode != 0:
    print("FAIL", r.returncode); sys.exit(0)
print(int((t1 - t0) * 1000000))
' "$@"
}

macro_case() { # macro_case <name> <reps> cmd...
  local name=$1 reps=$2; shift 2
  echo "==> macro $name"
  local vals=() v i
  for i in $(seq 1 "$reps"); do
    v=$(wall_us "$@")
    case "$v" in
      FAIL*) echo "FAIL macro $name: exit ${v#FAIL }" >&2; FAIL=1; return ;;
      *) vals+=("$v") ;;
    esac
  done
  local sorted median min
  sorted=$(printf '%s\n' "${vals[@]}" | sort -n)
  median=$(printf '%s\n' "$sorted" | awk -v n="$reps" 'NR == int(n/2)+1 {print; exit}')
  min=$(printf '%s\n' "$sorted" | head -1)
  # One extra (unmeasured) run for peak RSS (bytes on macOS). Guarded: a
  # flake here must not `set -e`-abort the driver after all timed runs passed.
  local rss
  rss=$(/usr/bin/time -l "$@" </dev/null 2>&1 >/dev/null | awk '/maximum resident set size/{print $1}') || rss=0
  [ -n "$rss" ] || rss=0
  printf '{"name":"%s","wall_us_median":%s,"wall_us_min":%s,"reps":%s,"peak_rss_bytes":%s}\n' \
    "$name" "$median" "$min" "$reps" "$rss" >> "$MACRO_JSON"
  echo "  $name: $((median / 1000)) ms (min $((min / 1000)), rss $((rss / 1048576)) MB)"
}

REP_MACRO=3
[ $MODE = release ] && REP_MACRO=5
[ -n "$QUICK" ] && REP_MACRO=2
REP_HEAVY=$(( REP_MACRO < 2 ? REP_MACRO : 2 ))   # docgen/compile: cap at 2

# The source-boot decoy: every dylib EXCEPT the self-hosted four, so
# loadStdlib falls back to embedded source exactly (docs/46).
DECOY="$TMP/decoylib"
mkdir -p "$DECOY"
for f in zig-out/lib/*.dylib; do
  b=$(basename "$f")
  case "$b" in
    libbirnam_modules.dylib|libbirnam_stdlib.dylib|libbirnam_toml.dylib|libbirnam_config.dylib) ;;
    *) ln -sf "$(pwd)/$f" "$DECOY/$b" ;;
  esac
done

macro_case boot.bare          "$REP_MACRO" env BIRNAM_NO_STDLIB=1 "$BIN" bench/nil.birnam
macro_case boot.stdlib-dylib  "$REP_MACRO" "$BIN" bench/nil.birnam
macro_case boot.stdlib-source "$REP_MACRO" env BIRNAM_LIB_DIR="$DECOY" "$BIN" bench/nil.birnam
if [ -f "$TMP/bench.image" ]; then
  macro_case boot.image       "$REP_MACRO" "$BIN" "$TMP/bench.image"
else
  echo "FAIL macro boot.image: no image artifact (image suite failed?)" >&2; FAIL=1
fi
macro_case compile.stage1-stdlib "$REP_HEAVY" "$BIN" --compile-module src/id/stdlib.birnam "$TMP/libbirnam_stdlib.dylib"
macro_case script.primes         "$REP_MACRO" "$BIN" examples/primes.birnam
macro_case script.word-frequency "$REP_MACRO" "$BIN" examples/word-frequency.birnam
macro_case docgen.full           "$REP_HEAVY" "$BIN" docgen "$TMP/docsite.html"

# ---------------------------------------------------------------------------
# merge + stamp
# ---------------------------------------------------------------------------
SHA=$(git rev-parse --short HEAD)
DIRTY=$(test -z "$(git status --porcelain)" && echo false || echo true)
VERSION=$(cat VERSION)
STAMP_DATE=$(date -u +%FT%TZ)
MACHINE=$(sysctl -n hw.model 2>/dev/null || echo unknown)
QUICK_JSON=$([ -n "$QUICK" ] && echo true || echo false)

if [ $MODE = release ]; then
  OUT="bench/results/v${VERSION}.json"
else
  mkdir -p bench/results/dev
  OUT="bench/results/dev/bench-${SHA}.json"
fi

{
  printf '{"stamp":{"sha":"%s","dirty":%s,"version":"%s","date":"%s","machine":"%s","optimize":"ReleaseSafe","mode":"%s","quick":%s},\n' \
    "$SHA" "$DIRTY" "$VERSION" "$STAMP_DATE" "$MACHINE" "$MODE" "$QUICK_JSON"
  # Each suite element is tagged with its label + run number (parsed from the
  # per-run filename) so release mode's 3 micro runs are distinguishable and a
  # partial failure is visible, not a silent multiset change.
  printf '"suites":['
  first=1
  for f in ${SUITE_FILES[@]+"${SUITE_FILES[@]}"}; do   # empty-safe on bash 3.2
    base=$(basename "$f" .json)   # e.g. micro-2
    label=${base%-*}; runno=${base##*-}
    [ $first = 1 ] && first=0 || printf ','
    printf '\n{"label":"%s","run":%s,"data":' "$label" "$runno"
    cat "$f"
    printf '}'
  done
  printf '],\n"macro":['
  first=1
  while IFS= read -r line; do
    [ $first = 1 ] && first=0 || printf ','
    printf '\n%s' "$line"
  done < "$MACRO_JSON"
  printf ']}\n'
} > "$OUT"

# The result must be real JSON — a malformed merge is a failed run.
python3 -m json.tool "$OUT" >/dev/null || { echo "FAIL: merged JSON invalid" >&2; FAIL=1; }

echo "==> results: $OUT"
[ "$FAIL" = 0 ] || { echo "==> BENCH FAILED (see FAIL lines above)" >&2; exit 1; }
