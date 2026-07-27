#!/usr/bin/env bash
# N6.7: prove the Rust compiler floor can bootstrap the checked-in source
# corpus from two independent fixture-free archives with deterministic
# generated-C, compiled-unit metadata, and installed dylib contracts.
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd -P)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-rust-cold-compiler.XXXXXX")
trap 'rm -rf "$scratch"' EXIT

[[ $(zig version) == 0.15.1 ]] || {
  echo "rust-cold-compiler: requires Zig 0.15.1" >&2
  exit 2
}
rustc --version | grep -q '^rustc 1\.95\.0 ' || {
  echo "rust-cold-compiler: requires rustc 1.95.0" >&2
  exit 2
}

rust_test_threads=${RUST_TEST_THREADS:-1}

archive_cycle() {
  local name=$1
  local root="$scratch/$name"
  local prefix="$root/prefix"
  local cache="$root/zig-cache"
  local global_cache="$root/zig-global-cache"
  local cargo_target="$root/cargo-target"
  local output="$root/build.out"
  local sentinel="$root/artifact-preflight-sentinel.dylib"
  mkdir -p "$root"

  git -C "$repo" archive HEAD | tar -x -C "$root"
  rm -rf "$root/abi/fixtures/idapi-v1"

  local prebuilt
  prebuilt=$(find "$root" -type f \
    \( -name birnam -o -name '*.dylib' -o -name '*.so' -o \
       -name '*.image' -o -name '*.bim' \) -print -quit)
  if [[ -n "$prebuilt" ]]; then
    echo "rust-cold-compiler: prebuilt Birnam artifact present: $prebuilt" >&2
    exit 1
  fi

  (
    cd "$root"
    zig build-lib -dynamic -lc src/id/artifact_preflight_sentinel.zig \
      -femit-bin="$sentinel"
    CARGO_TARGET_DIR="$cargo_target" cargo check --workspace --locked --offline
    BIRNAM_PREFLIGHT_SENTINEL_DYLIB="$sentinel" \
      CARGO_TARGET_DIR="$cargo_target" \
      RUST_TEST_THREADS="$rust_test_threads" \
      cargo test --workspace --locked --offline

    zig build install \
      -Ddistribution=production \
      -Dfloor=rust \
      -Dstore-impl=rust \
      -Dpeg=rust \
      -Dtcc=rust \
      -Dlang=rust \
      -Dcodegen-impl=rust \
      -Dhost-impl=rust \
      -Dartifact-inspector-impl=rust \
      -Dprelude-impl=rust \
      -Dcollection-impl=rust \
      -Derror-impl=rust \
      -Disolate-impl=rust \
      -Drelational=rust \
      -Dalien-impl=rust \
      -Dimage-impl=rust \
      -Doptimize=ReleaseSafe \
      -j1 \
      --prefix "$prefix" \
      --cache-dir "$cache" \
      --global-cache-dir "$global_cache" \
      >"$output" 2>&1

    BIRNAM_PREFLIGHT_SENTINEL_DYLIB="$sentinel" \
      zig build test-rust-peg-codegen-parity \
        test-rust-peg-metadata-parity \
        test-rust-peg-source-corpus \
        -Dfloor=rust \
        -Dstore-impl=rust \
        -Dpeg=rust \
        -Dtcc=rust \
        -Dlang=rust \
        -Dcodegen-impl=rust \
        -Dhost-impl=rust \
        -Dartifact-inspector-impl=rust \
        -Dprelude-impl=rust \
        -Dcollection-impl=rust \
        -Derror-impl=rust \
        -Disolate-impl=rust \
        -Drelational=rust \
        -Dalien-impl=rust \
        -Dimage-impl=rust \
        -Doptimize=ReleaseSafe \
        -j1 \
        --prefix "$prefix" \
        --cache-dir "$cache" \
        --global-cache-dir "$global_cache" \
        --summary all >>"$output" 2>&1
  )

  local codegen_digest source_digest
  codegen_digest=$(sed -n 's/^rust-peg-codegen-parity digest=//p' "$output" | tail -1)
  source_digest=$(sed -n 's/^rust-peg-source-corpus digest=//p' "$output" | tail -1)
  [[ "$codegen_digest" == 94960557ad83060565f82ef2db310c111f565ec003b5306b5a89852d1932876c ]] || {
    echo "rust-cold-compiler: $name generated-C/descriptor receipt mismatch" >&2
    exit 1
  }
  [[ "$source_digest" == '8cd4303f04fcff97 files=76' ]] || {
    echo "rust-cold-compiler: $name source-corpus receipt mismatch" >&2
    exit 1
  }
  grep -Fqx 'test-rust-peg-metadata-parity success' "$output" || {
    echo "rust-cold-compiler: $name metadata parity target did not pass" >&2
    exit 1
  }

  local manifest="$root/installed-dylibs.manifest"
  : >"$manifest"
  while IFS= read -r path; do
    install_name=$(otool -D "$prefix/$path" | tail -1 | tr -d '\r')
    # Release dylibs embed their archive-specific prefix in the install name;
    # compare the stable library contract rather than that temporary path.
    printf '%s %s\n' "$path" "${install_name##*/}" >>"$manifest"
  done < <(cd "$prefix" && find lib -maxdepth 1 -type f -name '*.dylib' -print | sort)
  test -s "$manifest"
  printf '%s\n' "$codegen_digest" >"$root/codegen.digest"
  printf '%s\n' "$source_digest" >"$root/source.digest"
  printf '%s\n' "$prefix" >"$root/prefix.path"
  echo "rust-cold-compiler: $name archive passed"
}

archive_cycle cycle-a
archive_cycle cycle-b

cmp "$scratch/cycle-a/codegen.digest" "$scratch/cycle-b/codegen.digest"
cmp "$scratch/cycle-a/source.digest" "$scratch/cycle-b/source.digest"
cmp "$scratch/cycle-a/installed-dylibs.manifest" "$scratch/cycle-b/installed-dylibs.manifest"

echo "rust-cold-compiler: two independent fixture-free Rust archives produced identical generated-C, descriptor, source-corpus, and dylib install-name receipts"
