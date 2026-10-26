#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export MISE_TRUSTED_CONFIG_PATHS="$PWD"
export CARGO_TARGET_DIR="${BIRNAM_OOP_TARGET:-$PWD/.cargo-target/oop-slice}"
bash scripts/oop-generation-test.sh
mise exec -- cargo build --offline --locked --release -p birnam-siphon --example oop_image > "$CARGO_TARGET_DIR/optional-build.log" 2>&1
mise exec -- cargo build --offline --locked --release --lib -p birnam-store -p birnam-relational >> "$CARGO_TARGET_DIR/optional-build.log" 2>&1
mise exec -- cargo test --offline --locked --lib -p birnam-store -p birnam-relational > "$CARGO_TARGET_DIR/optional-unit.log" 2>&1
python3 scripts/oop-optional-test.py "$CARGO_TARGET_DIR" "$(cat "$CARGO_TARGET_DIR/bootstrap-qualification/stage.txt")"
