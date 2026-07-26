#!/usr/bin/env bash
# C1.6: prove Rust Codegen selection installs a complete canonical artifact,
# retains both explicit fixtures, and still rejects unknown implementations.
set -euo pipefail

cd "$(dirname "$0")/.."

scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-codegen-refusal.XXXXXX")
trap 'rm -rf "$scratch"' EXIT

prefix="$scratch/prefix"
cache="$scratch/cache"
global_cache="$scratch/global-cache"
output="$scratch/build.out"

if ! zig build install \
  -Dhost-impl=rust \
  -Dcodegen-impl=rust \
  -Doptimize=ReleaseSafe \
  --prefix "$prefix" \
  --cache-dir "$cache" \
  --global-cache-dir "$global_cache" >"$output" 2>&1
then
  echo "codegen-selection: Rust Codegen selection failed" >&2
  exit 1
fi

canonical="$prefix/lib/libbirnam_id_codegen.dylib"
rust_fixture="$prefix/lib/libbirnam_id_codegen_rust.dylib"
zig_fixture="$prefix/lib/libbirnam_id_codegen_zig.dylib"
for path in "$canonical" "$rust_fixture" "$zig_fixture"; do
  if [ ! -f "$path" ]; then
    echo "codegen-selection: missing installed artifact: $path" >&2
    exit 1
  fi
done

if ! cmp -s "$canonical" "$rust_fixture"; then
  echo "codegen-selection: canonical Rust Codegen differs from its explicit fixture" >&2
  exit 1
fi

receipt=$(./scripts/native-rung-selection.sh codegen rust "$canonical" "$rust_fixture")
case "$receipt" in
  '{"schema":1,"rung":"codegen","implementation":"rust","sha256":"'*) ;;
  *)
    echo "codegen-selection: malformed Rust selection receipt: $receipt" >&2
    exit 1
    ;;
esac

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
