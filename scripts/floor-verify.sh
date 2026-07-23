#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
floor_kind=${1:?usage: floor-verify.sh zig|rust}
if [[ $floor_kind != zig && $floor_kind != rust ]]; then
  echo "usage: $0 zig|rust" >&2
  exit 2
fi

# First run the complete in-process matrix in its own isolated worker tree.
bash "$repo/scripts/rust-lower-rung-matrix.sh" "$floor_kind"

scratch=$(mktemp -d "/tmp/birnam-floor-verify-$floor_kind.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
prefix="$scratch/prefix"
cache="$scratch/zig-cache"
global_cache="$scratch/zig-global-cache"
tmp="$scratch/tmp"
mkdir -p "$prefix" "$cache" "$global_cache" "$tmp"

TMPDIR="$tmp" zig build install \
  -Dfloor="$floor_kind" \
  -Doptimize=ReleaseSafe \
  --prefix "$prefix" \
  --cache-dir "$cache" \
  --global-cache-dir "$global_cache"

floor="$prefix/lib/libbirnam_id.dylib"
identity="$floor_kind-id-v1"

test -x "$prefix/bin/birnam"
test -f "$floor"
test -f "$prefix/lib/libbirnam_id_zig.dylib"
digest=$(shasum -a 256 "$floor" | awk '{print $1}')

export BIRNAM_BIN="$prefix/bin/birnam"
export BIRNAM_LIB_DIR="$prefix/lib"
export BIRNAM_ID_DYLIB="$floor"
export BIRNAM_ID_FLOOR_IDENTITY="$identity"
export BIRNAM_ID_FLOOR_SHA256="$digest"

"$BIRNAM_BIN" "$repo/modules/run-tests.birnam"
bash "$repo/scripts/repl-golden.sh"
bash "$repo/scripts/source-fallback.sh"
bash "$repo/scripts/project-check-smoke.sh"
bash "$repo/scripts/lsp-project-smoke.sh"
bash "$repo/scripts/image-save-smoke.sh"
bash "$repo/scripts/isolate-smoke.sh"
bash "$repo/scripts/termbox-load-smoke.sh"
BIRNAM_PREFIX="$prefix" bash "$repo/scripts/install-smoke.sh"

echo "verify-floor-$floor_kind: CLI and relocated-install smokes passed with $identity $digest"
