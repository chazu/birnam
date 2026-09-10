#!/usr/bin/env bash
# Guarded slot-access semantic matrix on an installed prefix (Design 74E §3.7).
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: slot-access-matrix-smoke.sh PREFIX}
python3 scripts/slot-access-matrix-smoke.py "$prefix"
