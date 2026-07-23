#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
target_dir="$repo/.cargo-target"

CARGO_TARGET_DIR="$target_dir" cargo test -p birnam-peg --locked --offline
CARGO_TARGET_DIR="$target_dir" cargo build -p birnam-peg --locked --offline

dylib="$target_dir/debug/libbirnam_peg.dylib"
test -f "$dylib"
nm -gU "$dylib" | grep -q 'birnam_peg_rust_identity'
echo "rust-peg-shell: dynamic Rust PEG shell and identity export passed"
