#!/usr/bin/env bash
# C1.6: prove Rust Codegen selection installs a complete canonical artifact,
# retains both explicit fixtures, and still rejects unknown implementations.
set -euo pipefail

cd "$(dirname "$0")/.."

scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-codegen-refusal.XXXXXX")
trap 'rm -rf "$scratch"' EXIT

for implementation in rust zig; do
  prefix="$scratch/$implementation/prefix"
  cache="$scratch/$implementation/cache"
  global_cache="$scratch/$implementation/global-cache"
  output="$scratch/$implementation/build.out"
  mkdir -p "$scratch/$implementation"
  if ! zig build install \
    -Dhost-impl=rust \
    "-Dcodegen-impl=$implementation" \
    -Doptimize=ReleaseSafe \
    --prefix "$prefix" \
    --cache-dir "$cache" \
    --global-cache-dir "$global_cache" >"$output" 2>&1
  then
    echo "codegen-selection: $implementation Codegen selection failed" >&2
    exit 1
  fi

  canonical="$prefix/lib/libbirnam_id_codegen.dylib"
  explicit="$prefix/lib/libbirnam_id_codegen_${implementation}.dylib"
  for path in "$canonical" "$explicit" "$prefix/lib/libbirnam_id_codegen_rust.dylib" "$prefix/lib/libbirnam_id_codegen_zig.dylib"; do
    if [ ! -f "$path" ]; then
      echo "codegen-selection: missing installed artifact: $path" >&2
      exit 1
    fi
  done

  receipt=$(./scripts/native-rung-selection.sh codegen "$implementation" "$canonical" "$explicit")
  case "$receipt" in
    '{"schema":1,"rung":"codegen","implementation":"'"$implementation"'","sha256":"'*) ;;
    *)
      echo "codegen-selection: malformed $implementation selection receipt: $receipt" >&2
      exit 1
      ;;
  esac
done

if zig build install -Dcodegen-impl=other \
  --prefix "$scratch/unknown-prefix" \
  --cache-dir "$scratch/unknown-cache" \
  --global-cache-dir "$scratch/unknown-global" >"$scratch/unknown.out" 2>&1
then
  echo "codegen-selection: unknown implementation was accepted" >&2
  exit 1
fi
grep -Eq 'codegen-impl|invalid choice|expected one of' "$scratch/unknown.out"

echo "codegen-selection: Rust canonical selection, retained fixtures, and unknown-choice refusal passed"
