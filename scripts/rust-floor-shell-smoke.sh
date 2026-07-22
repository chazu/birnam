#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
target_dir="$repo/.cargo-target"
dylib="$target_dir/debug/libbirnam_id.dylib"
zig_floor="$repo/zig-out/lib/libbirnam_id.dylib"

CARGO_TARGET_DIR="$target_dir" \
  cargo build --workspace --locked --offline

test -f "$dylib"
zig run "$repo/scripts/rust-floor-shell-probe.zig" -- "$dylib"

# The diagnostic dylib remains outside Zig's install prefix. Prove that an
# ordinary build still installs the usable Zig floor under the production name.
zig build
test -f "$zig_floor"
if ! nm -gU "$zig_floor" | grep -q ' _id_init$'; then
  echo "rust-floor-shell: installed Zig floor does not export id_init" >&2
  exit 1
fi
if nm -gU "$zig_floor" | grep -q ' _birnam_id_rust_crate_identity$'; then
  echo "rust-floor-shell: Rust shell replaced the installed Zig floor" >&2
  exit 1
fi

echo "rust-floor-shell: Zig remains installed under the production name"
