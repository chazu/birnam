#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export MISE_TRUSTED_CONFIG_PATHS="$PWD"
export CARGO_TARGET_DIR="${BIRNAM_OOP_TARGET:-$PWD/.cargo-target/oop-slice}"
bash scripts/oop-id-slice-test.sh
bash scripts/oop-host-test.sh
python3 scripts/oop-generation-test.py "$CARGO_TARGET_DIR" "$(cat "$CARGO_TARGET_DIR/bootstrap-qualification/stage.txt")"
