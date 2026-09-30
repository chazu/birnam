#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
# An isolated configuration cache avoids rebuilding the workspace or confusing
# a diagnostic floor with the normal ABI. Cargo revalidates source/toolchain inputs.
target_dir="$repo/.cargo-target/dylib-audit"
expected_exports=$(python3 - "$repo/rust/birnam-id/exports.json" <<'PYEXPORTS'
import json, sys
proof = json.load(open(sys.argv[1]))
print("\n".join("_" + name for name in sorted(proof["exported_symbols"])))
PYEXPORTS
)

audit_profile() {
  local profile=$1
  local cargo_args=(build --manifest-path "$repo/Cargo.toml" -p birnam-namespace -p birnam-id --locked --offline)
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
  exports=$(nm -gjU "$dylib" | sort)
  if [[ $exports != "$expected_exports" ]]; then
    printf 'rust-dylib-audit: %s export surface differs\n--- expected ---\n%s\n--- actual ---\n%s\n' \
      "$profile" "$expected_exports" "$exports" >&2
    exit 1
  fi

  local path_audit="$dylib"
  if [[ $profile == debug ]]; then
    # DWARF deliberately names source locations. Audit executable/runtime data
    # in a stripped copy, while preserving the original development artifact.
    path_audit="$target_dir/$profile/libbirnam_id.path-audit.dylib"
    strip -S -o "$path_audit" "$dylib"
  fi
  if python3 - "$path_audit" "$repo" <<'PYPATH'
from pathlib import Path
import sys
raise SystemExit(0 if sys.argv[2].encode() in Path(sys.argv[1]).read_bytes() else 1)
PYPATH
  then
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
