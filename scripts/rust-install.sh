#!/usr/bin/env bash
# Build and publish a Rust-only Birnam prefix.
#
# This is the production source-build entrypoint. The Cargo driver gets its
# own temporary target directory so neither the checkout nor the requested
# prefix can satisfy the build from stale artifacts.
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
driver_target=$(mktemp -d "${TMPDIR:-/tmp}/birnam-rust-driver.XXXXXX")
trap 'rm -rf "$driver_target"' EXIT

exec env CARGO_TARGET_DIR="$driver_target" cargo run \
  --manifest-path "$repo/Cargo.toml" \
  --offline --locked -p birnam-host --bin birnam-build -- "$@"
