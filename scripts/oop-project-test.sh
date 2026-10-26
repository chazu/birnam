#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export MISE_TRUSTED_CONFIG_PATHS="$PWD"
export CARGO_TARGET_DIR="${BIRNAM_OOP_TARGET:-$PWD/.cargo-target/oop-slice}"
bash scripts/oop-host-test.sh
mise exec -- cargo test --offline --locked -p birnam-host-project -p birnam-project-build --lib > "$CARGO_TARGET_DIR/project-unit.log" 2>&1
python3 scripts/oop-project-test.py "$CARGO_TARGET_DIR" "$(cat "$CARGO_TARGET_DIR/bootstrap-qualification/stage.txt")"
