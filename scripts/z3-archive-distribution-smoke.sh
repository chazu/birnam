#!/usr/bin/env bash
# Design 66 Z3: build independent clean-archive production prefixes and one
# explicit full-Zig rollback prefix, then exercise their dynamic load graphs.
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd -P)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-z3-archive.XXXXXX")
scratch=$(cd "$scratch" && pwd -P)
trap 'rm -rf "$scratch"' EXIT

canonical=(
  libbirnam_id.dylib
  libbirnam_store.dylib
  libbirnam_id_artifact.dylib
  libbirnam_id_peg.dylib
  libbirnam_id_lang.dylib
  libbirnam_id_tcc.dylib
  libbirnam_id_codegen.dylib
  libbirnam_id_prelude.dylib
  libbirnam_id_collection.dylib
  libbirnam_id_error.dylib
  libbirnam_id_isolate.dylib
  libbirnam_id_relational.dylib
  libbirnam_id_alien.dylib
  libbirnam_id_image.dylib
)

archive_source() {
  local label=$1
  local source="$scratch/$label/source"
  mkdir -p "$source"
  git -C "$repo" archive HEAD | tar -x -C "$source"
  fixture_dir="$source/abi/fixtures/idapi-v1"
  case "$fixture_dir" in
    "$source"/*) rm -rf "$fixture_dir" ;;
    *) echo "z3-archive: refused unsafe fixture path" >&2; exit 1 ;;
  esac
  if find "$source" -type f \( -name birnam -o -name '*.dylib' -o -name '*.so' -o -name '*.image' -o -name '*.bim' \) -print -quit | grep -q .; then
    echo "z3-archive: clean archive $label contains a prebuilt Birnam artifact" >&2
    exit 1
  fi
  printf '%s\n' "$source"
}

build_production() {
  local label=$1
  local source="$scratch/$label/source"
  archive_source "$label" >/dev/null
  local prefix="$scratch/$label/prefix"
  local cache="$scratch/$label/cache"
  local global_cache="$scratch/$label/global-cache"
  (
    cd "$source"
    zig build install -Doptimize=ReleaseSafe -Ddistribution=production \
      --prefix "$prefix" --cache-dir "$cache" --global-cache-dir "$global_cache" \
      >"$scratch/$label/build.out" 2>&1
  )
  printf '%s\n' "$prefix"
}

trace_prefix() {
  local label=$1
  local prefix=$2
  local cwd="$scratch/$label/cwd"
  local trace="$scratch/$label/load.trace"
  mkdir -p "$cwd/src/id"
  printf 'not a library; trace decoy\n' >"$cwd/libbirnam_id.dylib"
  printf 'not a source provider; trace decoy\n' >"$cwd/src/id/main.zig"
  (
    cd "$cwd"
    unset BIRNAM_LIB_DIR BIRNAM_ID_DYLIB DYLD_LIBRARY_PATH BIRNAM_FORCE_SOURCE
    printf ':quit\n' | DYLD_PRINT_LIBRARIES=1 "$prefix/bin/birnam" --repl \
      >"$scratch/$label/repl.out" 2>"$trace"
  )
  for name in "${canonical[@]}"; do
    grep -F "$prefix/lib/$name" "$trace" >/dev/null || {
      echo "z3-archive: $label did not load $name" >&2
      cat "$trace" >&2
      exit 1
    }
  done
  case "$label" in
    production-a|production-b)
      if grep -E '_zig\.dylib|birnam-zig-oracle' "$trace" >/dev/null; then
        echo "z3-archive: $label loaded a Zig provider" >&2
        cat "$trace" >&2
        exit 1
      fi
      ;;
    rollback)
      if grep -E '_rust\.dylib' "$trace" >/dev/null; then
        echo "z3-archive: rollback loaded a Rust fixture" >&2
        cat "$trace" >&2
        exit 1
      fi
      ;;
  esac
}

production_a=$(build_production production-a)
production_b=$(build_production production-b)
trace_prefix production-a "$production_a"
trace_prefix production-b "$production_b"

rollback_source="$scratch/rollback/source"
archive_source rollback >/dev/null
rollback_prefix="$scratch/rollback/prefix"
mkdir -p "$scratch/rollback"
(
  cd "$rollback_source"
  zig build install -Doptimize=ReleaseSafe -Ddistribution=rollback \
    --prefix "$rollback_prefix" --cache-dir "$scratch/rollback/cache" \
    --global-cache-dir "$scratch/rollback/global-cache" \
    >"$scratch/rollback/build.out" 2>&1
  python3 scripts/check-rollback-prefix.py "$rollback_prefix" \
    >"$scratch/rollback/receipt.jsonl"
)
trace_prefix rollback "$rollback_prefix"

echo "z3-archive: production_a=clean production_b=clean rollback=full-zig traces=ok"
