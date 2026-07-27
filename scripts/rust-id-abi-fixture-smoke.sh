#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
# This fixture is an ABI/image compatibility proof, not a Rust-host CLI
# contract.  During the Rust-host rehearsal the installed Zig oracle is the
# authority that can read the pre-rewrite image; callers may provide it
# explicitly while the canonical `BIRNAM_BIN` remains the Rust host.
target_dir=${BIRNAM_RUST_ID_TARGET_DIR:-$repo/.cargo-target/rust-id-abi-fixture}
bin=${BIRNAM_ID_FIXTURE_BIN:-${BIRNAM_BIN:-$repo/zig-out/bin/birnam}}
bin_root=$(cd "$(dirname "$bin")/.." && pwd)
lib_dir=${BIRNAM_LIB_DIR:-$bin_root/lib}

probe_profile() {
  local profile=$1
  local cargo_args=(build --workspace --locked --offline)
  if [[ $profile == release ]]; then
    cargo_args+=(--release)
  fi
  CARGO_TARGET_DIR="$target_dir" cargo "${cargo_args[@]}"
  local dylib="$target_dir/$profile/libbirnam_id.dylib"
  local digest
  digest=$(shasum -a 256 "$dylib" | awk '{print $1}')
  BIRNAM_BIN="$bin" \
    BIRNAM_IMAGE_DYLIB="${BIRNAM_IMAGE_DYLIB:-$lib_dir/libbirnam_id_image_zig.dylib}" \
    BIRNAM_CODEGEN_DYLIB="${BIRNAM_CODEGEN_DYLIB:-$lib_dir/libbirnam_id_codegen_zig.dylib}" \
    BIRNAM_ID_DYLIB="$dylib" \
    BIRNAM_ID_FLOOR_IDENTITY=rust-id-v1 \
    BIRNAM_ID_FLOOR_SHA256="$digest" \
    bash "$repo/abi/fixtures/idapi-v1/verify.sh"
  echo "rust-id-abi-fixture: $profile selected $digest"
}

probe_profile debug
probe_profile release
