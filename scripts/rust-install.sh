#!/usr/bin/env bash
# Build and publish a Rust-only Birnam prefix.
#
# Cold by default. --build-cache DIR explicitly enables Cargo reuse for local
# development; the driver records this distinction in the published receipt.
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
build_cache=
next_cache=0
for argument in "$@"; do
  if [[ $next_cache == 1 ]]; then build_cache=$argument; next_cache=0; fi
  if [[ $argument == --build-cache ]]; then next_cache=1; fi
done
if [[ $next_cache == 1 ]]; then echo '--build-cache requires a directory' >&2; exit 2; fi
if [[ -n $build_cache ]]; then
  mkdir -p "$build_cache/driver"
  driver_target=$(cd "$build_cache/driver" && pwd)
else
  driver_target=$(mktemp -d "${TMPDIR:-/tmp}/birnam-rust-driver.XXXXXX")
  trap 'rm -rf "$driver_target"' EXIT
fi

env CARGO_TARGET_DIR="$driver_target" cargo run \
  --manifest-path "$repo/Cargo.toml" \
  --offline --locked -p birnam-host --bin birnam-build -- "$@"
