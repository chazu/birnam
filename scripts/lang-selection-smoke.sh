#!/usr/bin/env bash
# Prove canonical Rust-Lang selection, explicit Zig rollback, and fail-closed
# option parsing without using a checkout-relative runtime tree.
set -euo pipefail

cd "$(dirname "$0")/.."

tmp=$(mktemp -d "${TMPDIR:-/tmp}/birnam-lang-selection.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

for implementation in rust zig; do
  mkdir -p "$tmp/$implementation"
  prefix="$tmp/$implementation/prefix"
  cache="$tmp/$implementation/cache"
  global_cache="$tmp/$implementation/global-cache"
  zig build install \
    "-Dlang=$implementation" \
    --prefix "$prefix" \
    --cache-dir "$cache" \
    --global-cache-dir "$global_cache" \
    >"$tmp/$implementation/build.out" 2>&1

  canonical="$prefix/lib/libbirnam_id_lang.dylib"
  explicit="$prefix/lib/libbirnam_id_lang_${implementation}.dylib"
  receipt=$(./scripts/native-rung-selection.sh lang "$implementation" "$canonical" "$explicit")
  case "$receipt" in
    '{"schema":1,"rung":"lang","implementation":"'"$implementation"'","sha256":"'*) ;;
    *)
      echo "lang-selection-smoke: malformed $implementation receipt: $receipt" >&2
      exit 1
      ;;
  esac
  test -f "$prefix/lib/libbirnam_id_lang_rust.dylib"
  test -f "$prefix/lib/libbirnam_id_lang_zig.dylib"
  printf 'let n := 1\n[n + 2]\n:quit\n' |
    BIRNAM_LIB_DIR="$prefix/lib" "$prefix/bin/birnam" >"$tmp/$implementation/repl.out"
  grep -q '3' "$tmp/$implementation/repl.out"
done

mkdir -p "$tmp/unknown"
if zig build install -Dlang=other \
  --prefix "$tmp/unknown/prefix" \
  --cache-dir "$tmp/unknown/cache" \
  --global-cache-dir "$tmp/unknown/global-cache" \
  >"$tmp/unknown.out" 2>&1
then
  echo "lang-selection-smoke: unknown implementation was accepted" >&2
  exit 1
fi
grep -q 'lang' "$tmp/unknown.out"

echo "lang-selection-smoke: Rust canonical, Zig rollback, and fail-closed choice passed"
