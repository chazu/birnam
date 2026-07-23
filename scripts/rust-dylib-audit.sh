#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
target_dir=$(mktemp -d /tmp/birnam-rust-dylib-audit.XXXXXX)
trap 'rm -rf "$target_dir"' EXIT

expected_exports=$'_birnam_id_floor_identity\n_birnam_id_rust_abi_descriptor_v1\n_birnam_id_rust_crate_identity\n_id_init'

audit_profile() {
  local profile=$1
  local cargo_args=(build --workspace --locked --offline)
  if [[ $profile == release ]]; then cargo_args+=(--release); fi
  CARGO_TARGET_DIR="$target_dir" cargo "${cargo_args[@]}"

  local dylib="$target_dir/$profile/libbirnam_id.dylib"
  test -f "$dylib"

  local install_name
  install_name=$(otool -D "$dylib" | tail -n +2)
  if [[ $install_name != @rpath/libbirnam_id.dylib ]]; then
    echo "rust-dylib-audit: $profile install name is $install_name" >&2
    exit 1
  fi

  local unexpected_dependencies
  unexpected_dependencies=$(otool -L "$dylib" | tail -n +2 | awk '{print $1}' |
    grep -Ev '^(@rpath/libbirnam_id[.]dylib|/usr/lib/libSystem[.]B[.]dylib)$' || true)
  if [[ -n $unexpected_dependencies ]]; then
    printf 'rust-dylib-audit: %s has unexpected dependencies:\n%s\n' \
      "$profile" "$unexpected_dependencies" >&2
    exit 1
  fi

  local exports
  exports=$(nm -gU "$dylib" | awk '$2 == "T" {print $3}' | sort)
  if [[ $exports != "$expected_exports" ]]; then
    printf 'rust-dylib-audit: %s export surface differs\n--- expected ---\n%s\n--- actual ---\n%s\n' \
      "$profile" "$expected_exports" "$exports" >&2
    exit 1
  fi

  if strings "$dylib" | grep -Fq "$repo"; then
    echo "rust-dylib-audit: $profile retains the repository path" >&2
    exit 1
  fi

  local size digest
  size=$(stat -f %z "$dylib")
  digest=$(shasum -a 256 "$dylib" | awk '{print $1}')
  echo "rust-dylib-audit: $profile size=$size sha256=$digest install_name=$install_name"
}

audit_profile debug
audit_profile release
echo "rust-dylib-audit: dependencies, install names, paths, and exports passed"
