#!/usr/bin/env bash
# Prove canonical Rust-TCC selection, explicit Zig rollback, and fail-closed
# option parsing without using a checkout-relative runtime tree.
set -euo pipefail

cd "$(dirname "$0")/.."

tmp=$(mktemp -d "${TMPDIR:-/tmp}/birnam-tcc-selection.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

for implementation in rust zig; do
  mkdir -p "$tmp/$implementation"
  prefix="$tmp/$implementation/prefix"
  cache="$tmp/$implementation/cache"
  global_cache="$tmp/$implementation/global-cache"
  zig build install \
    "-Dtcc=$implementation" \
    --prefix "$prefix" \
    --cache-dir "$cache" \
    --global-cache-dir "$global_cache" \
    >"$tmp/$implementation/build.out" 2>&1

  canonical="$prefix/lib/libbirnam_id_tcc.dylib"
  explicit="$prefix/lib/libbirnam_id_tcc_${implementation}.dylib"
  receipt=$(./scripts/native-rung-selection.sh tcc "$implementation" "$canonical" "$explicit")
  case "$receipt" in
    '{"schema":1,"rung":"tcc","implementation":"'"$implementation"'","sha256":"'*) ;;
    *)
      echo "tcc-selection-smoke: malformed $implementation receipt: $receipt" >&2
      exit 1
      ;;
  esac
  test -f "$prefix/lib/libbirnam_id_tcc_rust.dylib"
  test -f "$prefix/lib/libbirnam_id_tcc_zig.dylib"
done

mkdir -p "$tmp/unknown"
if zig build install -Dtcc=other \
  --prefix "$tmp/unknown/prefix" \
  --cache-dir "$tmp/unknown/cache" \
  --global-cache-dir "$tmp/unknown/global-cache" \
  >"$tmp/unknown.out" 2>&1
then
  echo "tcc-selection-smoke: unknown implementation was accepted" >&2
  exit 1
fi
grep -q 'tcc' "$tmp/unknown.out"

echo "tcc-selection-smoke: Rust canonical, Zig rollback, and fail-closed choice passed"
