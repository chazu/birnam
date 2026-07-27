#!/usr/bin/env bash
# C3: prove packaged Rust Codegen is the dylib actually loaded by the Rust
# host, while retaining an independently selectable Zig rollback prefix.
set -euo pipefail

cd "$(dirname "$0")/.."

scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-codegen-distribution.XXXXXX")
trap 'rm -rf "$scratch"' EXIT

build_prefix() {
  local implementation=$1
  local prefix="$scratch/$implementation/prefix"
  mkdir -p "$scratch/$implementation"
  zig build install \
    -Dhost-impl=rust \
    "-Dcodegen-impl=$implementation" \
    -Doptimize=ReleaseSafe \
    --prefix "$prefix" \
    --cache-dir "$scratch/$implementation/cache" \
    --global-cache-dir "$scratch/$implementation/global-cache" \
    >"$scratch/$implementation/build.out" 2>&1
  printf '%s\n' "$prefix"
}

rust_prefix=$(build_prefix rust)
zig_prefix=$(build_prefix zig)

rust_canonical="$rust_prefix/lib/libbirnam_id_codegen.dylib"
rust_selected="$rust_prefix/lib/libbirnam_id_codegen_rust.dylib"
rust_rollback="$rust_prefix/lib/libbirnam_id_codegen_zig.dylib"
zig_canonical="$zig_prefix/lib/libbirnam_id_codegen.dylib"
zig_selected="$zig_prefix/lib/libbirnam_id_codegen_zig.dylib"
zig_rollback="$zig_prefix/lib/libbirnam_id_codegen_rust.dylib"

for artifact in "$rust_canonical" "$rust_selected" "$rust_rollback" \
  "$zig_canonical" "$zig_selected" "$zig_rollback"; do
  test -f "$artifact"
done

# Keep the normal relocation contract in the evidence. This exercises the
# packaged tree from a separate working directory before the load trace below.
BIRNAM_PREFIX="$rust_prefix" scripts/install-smoke.sh >"$scratch/install-smoke.out" 2>&1

rust_digest=$(shasum -a 256 "$rust_canonical" | awk '{print $1}')
rust_selected_digest=$(shasum -a 256 "$rust_selected" | awk '{print $1}')
rust_rollback_digest=$(shasum -a 256 "$rust_rollback" | awk '{print $1}')
zig_digest=$(shasum -a 256 "$zig_canonical" | awk '{print $1}')
zig_selected_digest=$(shasum -a 256 "$zig_selected" | awk '{print $1}')
zig_rollback_digest=$(shasum -a 256 "$zig_rollback" | awk '{print $1}')

test "$rust_digest" = "$rust_selected_digest"
test "$zig_digest" = "$zig_selected_digest"
test "$rust_digest" != "$rust_rollback_digest"
test "$zig_digest" != "$zig_rollback_digest"

rust_receipt=$(scripts/native-rung-selection.sh codegen rust "$rust_canonical" "$rust_selected")
zig_receipt=$(scripts/native-rung-selection.sh codegen zig "$zig_canonical" "$zig_selected")
case "$rust_receipt" in
  '{"schema":1,"rung":"codegen","implementation":"rust","sha256":"'*) ;;
  *) echo "codegen-distribution: malformed Rust selection receipt" >&2; exit 1 ;;
esac
case "$zig_receipt" in
  '{"schema":1,"rung":"codegen","implementation":"zig","sha256":"'*) ;;
  *) echo "codegen-distribution: malformed Zig rollback receipt" >&2; exit 1 ;;
esac

trace_prefix() {
  local implementation=$1
  local prefix=$2
  local canonical="$prefix/lib/libbirnam_id_codegen.dylib"
  local trace="$scratch/$implementation/load.trace"
  mkdir -p "$scratch/$implementation/work"
  (
    cd "$scratch/$implementation/work"
    printf ':quit\n' |
      DYLD_PRINT_LIBRARIES=1 "$prefix/bin/birnam" --repl \
        >"$scratch/$implementation/repl.out" 2>"$trace"
  )
  grep -F "$canonical" "$trace" >/dev/null
  if grep -F "$prefix/lib/libbirnam_id_codegen_zig.dylib" "$trace" >/dev/null; then
    echo "codegen-distribution: $implementation loaded the Zig fixture directly" >&2
    exit 1
  fi
  printf '%s\n' "$trace"
}

rust_trace=$(trace_prefix rust "$rust_prefix")
zig_trace=$(trace_prefix zig "$zig_prefix")

printf '{"schema":1,"rung":"codegen-distribution","implementation":"rust","canonicalSha256":"%s","selectedFixtureSha256":"%s","zigRollbackSha256":"%s","loadedCanonical":true}\n' \
  "$rust_digest" "$rust_selected_digest" "$rust_rollback_digest"
printf '{"schema":1,"rung":"codegen-distribution","implementation":"zig","canonicalSha256":"%s","selectedFixtureSha256":"%s","rustRollbackSha256":"%s","loadedCanonical":true}\n' \
  "$zig_digest" "$zig_selected_digest" "$zig_rollback_digest"
printf 'codegen-distribution: packaged Rust canonical and explicit Zig rollback loads passed\n'
