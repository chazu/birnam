#!/usr/bin/env bash
# bench-compare.sh — diff two merged benchmark results (docs/design/46).
#
#   scripts/bench-compare.sh <old.json> <new.json>
#   scripts/bench-compare.sh bench/results/v0.8.0.json bench/results/dev/bench-abc1234.json
#
# Exit 0 = no regressions past threshold and no un-retired missing cases;
# 1 = regression/missing; 2 = usage or parse failure. Thresholds and the
# retired-case list live at the top of bench/compare.birnam.
#
# The compare tool is birnam (bench/compare.birnam), whose module sources
# live in THIS repo — so the repo's own build runs it (an older installed
# binary can silently mis-run newer module sources; measured: the 0.7.4
# install produced empty output on these files). If the working-tree build
# is broken, point BENCH_BIN at any known-good birnam >= the compare tool's
# vintage.
set -euo pipefail

[ $# -eq 2 ] || { echo "usage: $(basename "$0") <old.json> <new.json>" >&2; exit 2; }
# Resolve against the CALLER's cwd, before moving to the repo root — a
# relative path must mean what the user sees, not repo-root-relative.
[ -f "$1" ] || { echo "error: no such file: $1" >&2; exit 2; }
[ -f "$2" ] || { echo "error: no such file: $2" >&2; exit 2; }
A=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
B=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")

cd "$(dirname "$0")/.."

BIN="${BENCH_BIN:-$HOME/.local/bin/birnam}"
[ -x "$BIN" ] || { echo "error: no birnam binary at $BIN (set BENCH_BIN or install the Rust prefix)" >&2; exit 2; }

BENCH_A="$A" BENCH_B="$B" exec "$BIN" bench/compare.birnam </dev/null
