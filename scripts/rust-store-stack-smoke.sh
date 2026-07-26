#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
# The verify graph runs this smoke beside the Rust ABI fixture.  Keep Cargo's
# build outputs separate so concurrent debug/release builds cannot replace a
# dylib while the other script is loading it.
target_dir=${BIRNAM_RUST_STORE_TARGET_DIR:-$repo/.cargo-target/rust-store-stack}
bin=${BIRNAM_BIN:-$repo/zig-out/bin/birnam}
libdir=${BIRNAM_LIB_DIR:-$repo/zig-out/lib}

probe_profile() {
  local profile=$1
  local cargo_args=(build --workspace --locked --offline)
  if [[ $profile == release ]]; then
    cargo_args+=(--release)
  fi

  CARGO_TARGET_DIR="$target_dir" cargo "${cargo_args[@]}"
  local dylib="$target_dir/$profile/libbirnam_store.dylib"
  test -f "$dylib"

  BIRNAM_BIN="$bin" \
    BIRNAM_LIB_DIR="$libdir" \
    BIRNAM_STORE_DYLIB="$dylib" \
    bash "$repo/scripts/image-save-smoke.sh"
  BIRNAM_BIN="$bin" \
    BIRNAM_LIB_DIR="$libdir" \
    BIRNAM_STORE_DYLIB="$dylib" \
    bash "$repo/scripts/repl-golden.sh"
  echo "rust-store-stack: $profile passed image-save and REPL integration"
}

probe_profile debug
probe_profile release

oracle="$libdir/libbirnam_store_zig.dylib"
test -f "$oracle"
BIRNAM_BIN="$bin" \
  BIRNAM_LIB_DIR="$libdir" \
  BIRNAM_STORE_DYLIB="$oracle" \
  bash "$repo/scripts/image-save-smoke.sh"
BIRNAM_BIN="$bin" \
  BIRNAM_LIB_DIR="$libdir" \
  BIRNAM_STORE_DYLIB="$oracle" \
  bash "$repo/scripts/repl-golden.sh"
BIRNAM_BIN="$bin" \
  BIRNAM_LIB_DIR="$libdir" \
  BIRNAM_STORE_DYLIB="$oracle" \
  bash "$repo/scripts/project-check-smoke.sh"
BIRNAM_BIN="$bin" \
  BIRNAM_LIB_DIR="$libdir" \
  BIRNAM_STORE_DYLIB="$oracle" \
  bash "$repo/scripts/lsp-project-smoke.sh"
echo "rust-store-stack: Zig Store oracle passed image, REPL, project, and LSP integration"
