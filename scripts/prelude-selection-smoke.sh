#!/usr/bin/env bash
# Prove canonical Rust-Prelude selection, explicit Zig rollback, and
# fail-closed option parsing without using a checkout-relative runtime tree.
set -euo pipefail

cd "$(dirname "$0")/.."

tmp=$(mktemp -d "${TMPDIR:-/tmp}/birnam-prelude-selection.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

for implementation in rust zig; do
  mkdir -p "$tmp/$implementation"
  prefix="$tmp/$implementation/prefix"
  cache="$tmp/$implementation/cache"
  global_cache="$tmp/$implementation/global-cache"
  zig build install \
    "-Dprelude-impl=$implementation" \
    --prefix "$prefix" \
    --cache-dir "$cache" \
    --global-cache-dir "$global_cache" \
    >"$tmp/$implementation/build.out" 2>&1

  canonical="$prefix/lib/libbirnam_id_prelude.dylib"
  explicit="$prefix/lib/libbirnam_id_prelude_${implementation}.dylib"
  receipt=$(./scripts/native-rung-selection.sh prelude "$implementation" "$canonical" "$explicit")
  case "$receipt" in
    '{"schema":1,"rung":"prelude","implementation":"'"$implementation"'","sha256":"'*) ;;
    *)
      echo "prelude-selection-smoke: malformed $implementation receipt: $receipt" >&2
      exit 1
      ;;
  esac
  test -f "$prefix/lib/libbirnam_id_prelude_rust.dylib"
  test -f "$prefix/lib/libbirnam_id_prelude_zig.dylib"
done

mkdir -p "$tmp/unknown"
if zig build install -Dprelude-impl=other \
  --prefix "$tmp/unknown/prefix" \
  --cache-dir "$tmp/unknown/cache" \
  --global-cache-dir "$tmp/unknown/global-cache" \
  >"$tmp/unknown.out" 2>&1
then
  echo "prelude-selection-smoke: unknown implementation was accepted" >&2
  exit 1
fi
grep -q 'prelude-impl' "$tmp/unknown.out"

echo "prelude-selection-smoke: Rust canonical, Zig rollback, and fail-closed choice passed"
