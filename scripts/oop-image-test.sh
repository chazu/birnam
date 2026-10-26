#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export MISE_TRUSTED_CONFIG_PATHS="$PWD"
export CARGO_TARGET_DIR="${BIRNAM_OOP_TARGET:-$PWD/.cargo-target/oop-slice}"
bash scripts/oop-generation-test.sh
mise exec -- cargo build --offline --locked --release -p birnam-siphon --example oop_image > "$CARGO_TARGET_DIR/image-reader-build.log" 2>&1
mise exec -- cargo test --offline --locked -p birnam-image-codec -p birnam-codegen --lib > "$CARGO_TARGET_DIR/image-unit.log" 2>&1
python3 scripts/oop-image-test.py "$CARGO_TARGET_DIR" "$(cat "$CARGO_TARGET_DIR/bootstrap-qualification/stage.txt")"
