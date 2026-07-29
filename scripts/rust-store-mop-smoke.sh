#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
target_dir="$repo/.cargo-target"

CARGO_TARGET_DIR="$target_dir" cargo test \
  --manifest-path "$repo/Cargo.toml" \
  --locked --offline -p birnam-store
echo "rust-store-mop: Store consistency-law suite passed"
