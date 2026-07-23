#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
target_dir="$repo/.cargo-target"
scratch=$(mktemp -d /tmp/birnam-rust-floor-shell.XXXXXX)
trap 'rm -rf "$scratch"' EXIT

probe_rust_profile() {
  local profile=$1
  local dylib="$target_dir/$profile/libbirnam_id.dylib"
  local cargo_args=(build --workspace --locked --offline)
  if [[ $profile == release ]]; then
    cargo_args+=(--release)
  fi

  CARGO_TARGET_DIR="$target_dir" cargo "${cargo_args[@]}"
  test -f "$dylib"
  zig run "$repo/scripts/rust-floor-shell-probe.zig" -- "$dylib"
  if nm -gU "$dylib" | grep -q ' _birnam_id_test_'; then
    echo "rust-floor-shell: test hook leaked into ordinary $profile artifact" >&2
    exit 1
  fi
}

probe_rust_profile debug
probe_rust_profile release

# The Rust dylib remains outside the Zig oracle's build path. Probe the oracle
# through its explicit installed name without mutating the ordinary prefix.
zig_floor=${BIRNAM_ZIG_ID_DYLIB:-}
if [[ -z "$zig_floor" ]]; then
  mkdir -p "$scratch/zig-cache" "$scratch/zig-global-cache"
  zig build bootstrap-install \
    -Dfloor=zig \
    --prefix "$scratch/zig-prefix" \
    --cache-dir "$scratch/zig-cache" \
    --global-cache-dir "$scratch/zig-global-cache"
  zig_floor="$scratch/zig-prefix/lib/libbirnam_id_zig.dylib"
fi
test -f "$zig_floor"
if ! nm -gU "$zig_floor" | grep -q ' _id_init$'; then
  echo "rust-floor-shell: installed Zig oracle does not export id_init" >&2
  exit 1
fi
if nm -gU "$zig_floor" | grep -q ' _birnam_id_rust_crate_identity$'; then
  echo "rust-floor-shell: Rust shell replaced the installed Zig oracle" >&2
  exit 1
fi

echo "rust-floor-shell: Zig oracle remains independently loadable"
