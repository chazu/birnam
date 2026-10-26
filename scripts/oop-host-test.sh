#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export MISE_TRUSTED_CONFIG_PATHS="$PWD"
export CARGO_TARGET_DIR="${BIRNAM_OOP_TARGET:-$PWD/.cargo-target/oop-slice}"
bash scripts/oop-bootstrap-test.sh
mkdir -p "$CARGO_TARGET_DIR/host-qualification"
mise exec -- cargo build --offline --locked --release -p birnam-cli -p birnam-host-entry -p birnam-host-interaction -p birnam-host-project -p birnam-project-build > "$CARGO_TARGET_DIR/host-qualification/build.log" 2>&1
mise exec -- cargo test --offline --locked -p birnam-cli --bin birnam > "$CARGO_TARGET_DIR/host-qualification/cli-tests.log" 2>&1
mise exec -- cargo test --offline --locked -p birnam-host-support -p birnam-project-support > "$CARGO_TARGET_DIR/host-qualification/support-tests.log" 2>&1
python3 scripts/oop-host-test.py "$CARGO_TARGET_DIR" "$(cat "$CARGO_TARGET_DIR/bootstrap-qualification/stage.txt")"
