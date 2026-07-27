#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
scratch=$(mktemp -d /tmp/birnam-source-bootstrap.XXXXXX)
trap 'rm -rf "$scratch"' EXIT
host_impl=${BIRNAM_HOST_IMPL:-zig}
case "$host_impl" in
  zig|rust) ;;
  *) echo "source-bootstrap: BIRNAM_HOST_IMPL must be zig or rust" >&2; exit 2 ;;
esac
host_option=(-Dhost-impl="$host_impl")
codegen_impl=${BIRNAM_CODEGEN_IMPL:-zig}
case "$codegen_impl" in
  zig|rust) ;;
  *) echo "source-bootstrap: BIRNAM_CODEGEN_IMPL must be zig or rust" >&2; exit 2 ;;
esac
codegen_option=(-Dcodegen-impl="$codegen_impl")
# Cargo's FFI-heavy archive tests are deterministic only when the test
# harness is serialized. Keep the override available for diagnosis, but make
# the clean-bootstrap proof stable by default.
rust_test_threads=${RUST_TEST_THREADS:-1}

# A git archive is the clean-checkout boundary. Compatibility fixtures are
# deliberately removed: they are old-binary evidence, never bootstrap inputs.
git -C "$repo" archive HEAD | tar -x -C "$scratch"
fixture_dir="$scratch/abi/fixtures/idapi-v1"
case "$fixture_dir" in
  "$scratch"/*) rm -rf "$fixture_dir" ;;
  *) echo "source-bootstrap: refused unsafe fixture path" >&2; exit 1 ;;
esac

prebuilt=$(find "$scratch" -type f \
  \( -name birnam -o -name '*.dylib' -o -name '*.so' -o \
     -name '*.image' -o -name '*.bim' \) -print -quit)
if [[ -n "$prebuilt" ]]; then
  echo "source-bootstrap: prebuilt Birnam artifact present: $prebuilt" >&2
  exit 1
fi

[[ $(zig version) == 0.15.1 ]]
rustc --version | grep -q '^rustc 1\.95\.0 '

(
  cd "$scratch"
  sentinel="$scratch/artifact-preflight-sentinel.dylib"
  zig build-lib -dynamic -lc src/id/artifact_preflight_sentinel.zig -femit-bin="$sentinel"
  CARGO_TARGET_DIR="$scratch/.cargo-target" \
    cargo check --workspace --locked --offline
  BIRNAM_PREFLIGHT_SENTINEL_DYLIB="$sentinel" \
    CARGO_TARGET_DIR="$scratch/.cargo-target" \
    RUST_TEST_THREADS="$rust_test_threads" \
    cargo test --workspace --locked --offline
  zig build -Dfloor=rust "${host_option[@]}" "${codegen_option[@]}"
  # The archive boundary exercises the shared Rust/Zig preflight bytes in both
  # supported optimization modes; the ordinary build above is only the
  # fixture-free bootstrap/install proof.
  BIRNAM_PREFLIGHT_SENTINEL_DYLIB="$sentinel" \
    zig build test-artifact-preflight \
    -Dfloor=rust -Dartifact-inspector-impl=rust -Doptimize=Debug
  BIRNAM_PREFLIGHT_SENTINEL_DYLIB="$sentinel" \
    zig build test-artifact-preflight \
    -Dfloor=rust -Dartifact-inspector-impl=rust -Doptimize=ReleaseSafe
  BIRNAM_PREFLIGHT_SENTINEL_DYLIB="$sentinel" \
    BIRNAM_RUST_ARTIFACT_PROFILE=debug \
    bash scripts/rust-artifact-preflight-smoke.sh
  BIRNAM_PREFLIGHT_SENTINEL_DYLIB="$sentinel" \
    BIRNAM_RUST_ARTIFACT_PROFILE=release \
    bash scripts/rust-artifact-preflight-smoke.sh
  bash scripts/artifact-preflight-parity-smoke.sh
)

test -x "$scratch/zig-out/bin/birnam"
test -f "$scratch/zig-out/lib/libbirnam_id.dylib"
test -f "$scratch/zig-out/lib/libbirnam_id_zig.dylib"

# Exercise the archived Rust host's project and LSP workflows from an
# unrelated cwd. These scripts create their own projects and run check/build/
# run/install/test plus the protocol smoke against the relocated prefix.
(
  cd /tmp
  BIRNAM_BIN="$scratch/zig-out/bin/birnam" \
    BIRNAM_HOST_IMPL=rust \
    bash "$scratch/scripts/project-check-smoke.sh"
  BIRNAM_BIN="$scratch/zig-out/bin/birnam" \
    bash "$scratch/scripts/lsp-project-smoke.sh"
)

BIRNAM_LIB_DIR="$scratch/zig-out/lib" \
  BIRNAM_ID_FLOOR_IDENTITY=rust-id-v1 \
  "$scratch/zig-out/bin/birnam" "$scratch/bench/nil.birnam"
BIRNAM_LIB_DIR="$scratch/zig-out/lib" \
  BIRNAM_ID_DYLIB="$scratch/zig-out/lib/libbirnam_id_zig.dylib" \
  BIRNAM_ID_FLOOR_IDENTITY=zig-id-v1 \
  "$scratch/zig-out/bin/birnam" "$scratch/bench/nil.birnam"

(
  cd "$scratch"
  zig build -Dfloor=zig -Dcodegen-impl=zig "${host_option[@]}" --prefix "$scratch/zig-oracle-prefix"
)
cmp \
  "$scratch/zig-oracle-prefix/lib/libbirnam_id.dylib" \
  "$scratch/zig-oracle-prefix/lib/libbirnam_id_zig.dylib"
BIRNAM_LIB_DIR="$scratch/zig-oracle-prefix/lib" \
  BIRNAM_ID_FLOOR_IDENTITY=zig-id-v1 \
  "$scratch/zig-oracle-prefix/bin/birnam" "$scratch/bench/nil.birnam"

echo "source-bootstrap: fixture-free Rust production and Zig rollback builds passed"
