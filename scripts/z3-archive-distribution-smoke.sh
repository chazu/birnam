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
  printf 'not a compiled-unit provider; trace decoy\n' >"$cwd/libbirnam_stdlib.dylib"
  printf 'not a source provider; trace decoy\n' >"$cwd/src/id/main.zig"
  (
    cd "$cwd"
    unset BIRNAM_LIB_DIR BIRNAM_ID_DYLIB DYLD_LIBRARY_PATH DYLD_FALLBACK_LIBRARY_PATH \
      DYLD_INSERT_LIBRARIES DYLD_FRAMEWORK_PATH DYLD_FALLBACK_FRAMEWORK_PATH BIRNAM_FORCE_SOURCE
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
  while IFS= read -r loaded; do
    case "$loaded" in
      "$prefix"/*) ;;
      *)
        echo "z3-archive: $label loaded Birnam dylib outside selected prefix: $loaded" >&2
        cat "$trace" >&2
        exit 1
        ;;
    esac
  done < <(awk 'match($0, /\/[^ ]*\/libbirnam[^ ]*\.dylib/) { print substr($0, RSTART, RLENGTH) }' "$trace" | sort -u)
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

exercise_prefix() {
  local label=$1
  local source=$2
  local prefix=$3
  local host_impl=$4
  local libdir="$prefix/lib"
  local tcc="$libdir/libbirnam_id_tcc_rust.dylib"
  if [[ "$host_impl" == zig ]]; then
    tcc="$libdir/libbirnam_id_tcc.dylib"
  fi

  BIRNAM_LIB_DIR="$libdir" "$prefix/bin/birnam" "$source/bench/nil.birnam" >/dev/null
  BIRNAM_BIN="$prefix/bin/birnam" \
    BIRNAM_HOST_IMPL="$host_impl" \
    bash "$source/scripts/project-check-smoke.sh" >/dev/null
  BIRNAM_BIN="$prefix/bin/birnam" \
    bash "$source/scripts/lsp-project-smoke.sh" >/dev/null
  (
    cd "$source"
    BIRNAM_BIN="$prefix/bin/birnam" bash scripts/isolate-smoke.sh >/dev/null
    BIRNAM_BIN="$prefix/bin/birnam" \
      BIRNAM_LIB_DIR="$libdir" \
      bash scripts/image-save-smoke.sh >/dev/null
  )
  BIRNAM_BIN="$prefix/bin/birnam" \
    BIRNAM_LIB_DIR="$libdir" \
    BIRNAM_TCC_DYLIB="$tcc" \
    bash "$source/scripts/rust-tcc-artifact-smoke.sh" >/dev/null
  echo "z3-archive: $label source/project/lsp/image/wire/generated-C fixtures=ok"
}

old_image_fixture() {
  local label=$1
  local prefix=$2
  local host=$3
  local floor_identity=$4
  local image_provider=$5
  local codegen_provider=$6
  local fixture="$repo/abi/fixtures/idapi-v1"
  echo "z3-archive: $label pre-rewrite image/native-client fixtures=running"
  (
    cd "$fixture"
    BIRNAM_BIN="$host" \
      BIRNAM_LIB_DIR="$prefix/lib" \
      BIRNAM_ID_DYLIB="$prefix/lib/libbirnam_id.dylib" \
      BIRNAM_ID_FLOOR_IDENTITY="$floor_identity" \
      BIRNAM_IMAGE_DYLIB="$image_provider/lib/libbirnam_id_image.dylib" \
      BIRNAM_CODEGEN_DYLIB="$codegen_provider/lib/libbirnam_id_codegen.dylib" \
      bash verify.sh >/dev/null
  )
  echo "z3-archive: $label pre-rewrite image/native-client fixtures=ok"
}

release_bundle() {
  local label=$1
  local prefix=$2
  local bundle="$scratch/$label-bundle"
  local cwd="$scratch/$label-bundle-cwd"
  local trace="$scratch/$label-bundle.trace"
  mkdir -p "$bundle" "$cwd"
  cp -R "$prefix/bin" "$prefix/lib" "$prefix/share" "$bundle/"
  if [[ "$label" == production ]]; then
    test ! -e "$bundle/bin/birnam-zig-oracle"
    if find "$bundle/lib" -type f -name '*_zig.dylib' -print -quit | grep -q .; then
      echo "z3-archive: production release bundle contains Zig fixtures" >&2
      exit 1
    fi
  else
    test -x "$bundle/bin/birnam-zig-oracle"
  fi
  (
    cd "$cwd"
    unset BIRNAM_LIB_DIR BIRNAM_ID_DYLIB DYLD_LIBRARY_PATH DYLD_FALLBACK_LIBRARY_PATH \
      DYLD_INSERT_LIBRARIES DYLD_FRAMEWORK_PATH DYLD_FALLBACK_FRAMEWORK_PATH BIRNAM_FORCE_SOURCE
    printf ':quit\n' | DYLD_PRINT_LIBRARIES=1 "$bundle/bin/birnam" --repl \
      >"$scratch/$label-bundle.out" 2>"$trace"
  )
  grep -F "$bundle/lib/libbirnam_id.dylib" "$trace" >/dev/null
  while IFS= read -r loaded; do
    case "$loaded" in
      "$bundle"/*) ;;
      *)
        echo "z3-archive: $label release bundle loaded Birnam dylib outside bundle: $loaded" >&2
        cat "$trace" >&2
        exit 1
        ;;
    esac
  done < <(awk 'match($0, /\/[^ ]*\/libbirnam[^ ]*\.dylib/) { print substr($0, RSTART, RLENGTH) }' "$trace" | sort -u)
  if [[ "$label" == production ]]; then
    ! grep -E '_zig\.dylib|birnam-zig-oracle' "$trace" >/dev/null
  else
    ! grep -E '_rust\.dylib' "$trace" >/dev/null
  fi
  echo "z3-archive: $label release bundle/no-ambient-fallback=ok"
}

production_a=$(build_production production-a)
production_b=$(build_production production-b)
trace_prefix production-a "$production_a"
trace_prefix production-b "$production_b"
exercise_prefix production-a "$scratch/production-a/source" "$production_a" rust
exercise_prefix production-b "$scratch/production-b/source" "$production_b" rust

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
python3 "$scratch/production-a/source/scripts/check-production-prefix.py" "$production_a" \
  "$scratch/production-a/source/.cargo-target/release" "$rollback_prefix/lib" \
  >"$scratch/production-a/receipt.jsonl"
python3 "$scratch/production-b/source/scripts/check-production-prefix.py" "$production_b" \
  "$scratch/production-b/source/.cargo-target/release" "$rollback_prefix/lib" \
  >"$scratch/production-b/receipt.jsonl"
trace_prefix rollback "$rollback_prefix"
exercise_prefix rollback "$rollback_source" "$rollback_prefix" zig
old_image_fixture production-a "$production_a" "$production_a/bin/birnam" rust-id-v1 "$production_a" "$production_a"
old_image_fixture production-b "$production_b" "$production_b/bin/birnam" rust-id-v1 "$production_b" "$production_b"
old_image_fixture rollback "$rollback_prefix" "$rollback_prefix/bin/birnam" zig-id-v1 "$rollback_prefix" "$rollback_prefix"
release_bundle production "$production_a"
release_bundle rollback "$rollback_prefix"

echo "z3-archive: production-a receipt"
cat "$scratch/production-a/receipt.jsonl"
echo "z3-archive: production-b receipt"
cat "$scratch/production-b/receipt.jsonl"
echo "z3-archive: rollback receipt"
cat "$scratch/rollback/receipt.jsonl"
echo "z3-archive: production_a=clean production_b=clean rollback=full-zig traces=ok"
