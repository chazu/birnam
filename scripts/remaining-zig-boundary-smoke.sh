#!/usr/bin/env bash
# Design 66 Z0.2: prove canonical Rust resolution, explicit Zig rollback, and
# the native-Birnam policy/MOP fixture from isolated installed prefixes.
set -euo pipefail

cd "$(dirname "$0")/.."
repo=$(pwd -P)
export LC_ALL=C

scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-remaining-zig-boundary.XXXXXX")
scratch=$(cd "$scratch" && pwd -P)
trap 'rm -rf "$scratch"' EXIT

# These names are the frozen runtime order from src/id/main.zig. The Rust crate
# artifact names intentionally differ for a few rungs; the map below keeps the
# receipt tied to the installed dynamic name rather than a crate/package name.
modules=(
  libbirnam_id_artifact
  libbirnam_store
  libbirnam_id_peg
  libbirnam_id_lang
  libbirnam_id_tcc
  libbirnam_id_codegen
  libbirnam_id_prelude
  libbirnam_id_collection
  libbirnam_id_error
  libbirnam_id_isolate
  libbirnam_id_relational
  libbirnam_id_alien
  libbirnam_id_image
)
runtime_names=(libbirnam_id "${modules[@]}")

rust_source_stem() {
  case "$1" in
    libbirnam_id) echo libbirnam_id ;;
    libbirnam_id_artifact) echo libbirnam_artifact ;;
    libbirnam_store) echo libbirnam_store ;;
    libbirnam_id_peg) echo libbirnam_peg ;;
    libbirnam_id_lang) echo libbirnam_lang ;;
    libbirnam_id_tcc) echo libbirnam_tcc ;;
    libbirnam_id_codegen) echo libbirnam_codegen_runtime ;;
    libbirnam_id_prelude) echo libbirnam_prelude ;;
    libbirnam_id_collection) echo libbirnam_collection ;;
    libbirnam_id_error) echo libbirnam_error ;;
    libbirnam_id_isolate) echo libbirnam_isolate ;;
    libbirnam_id_relational) echo libbirnam_relational ;;
    libbirnam_id_alien) echo libbirnam_alien ;;
    libbirnam_id_image) echo libbirnam_image ;;
    *) echo "unknown Rust source stem: $1" >&2; exit 1 ;;
  esac
}

build_prefix() {
  local implementation=$1
  local prefix="$scratch/$implementation/prefix"
  local cache="$scratch/$implementation/cache"
  local global_cache="$scratch/$implementation/global-cache"
  mkdir -p "$scratch/$implementation"

  local options=(
    -Doptimize=ReleaseSafe
    -Dfloor="$implementation"
    -Dhost-impl="$implementation"
    -Dartifact-inspector-impl="$implementation"
    -Dpeg="$implementation"
    -Dtcc="$implementation"
    -Dlang="$implementation"
    -Dcodegen-impl="$implementation"
    -Dprelude-impl="$implementation"
    -Dcollection-impl="$implementation"
    -Derror-impl="$implementation"
    -Disolate-impl="$implementation"
    -Drelational="$implementation"
    -Dimage-impl="$implementation"
  )
  zig build install "${options[@]}" \
    --prefix "$prefix" \
    --cache-dir "$cache" \
    --global-cache-dir "$global_cache" \
    >"$scratch/$implementation/build.out" 2>&1
  printf '%s\n' "$prefix"
}

rust_prefix=$(build_prefix rust)
zig_prefix=$(build_prefix zig)

sha256() { shasum -a 256 "$1" | awk '{print $1}'; }

rust_receipts=0
zig_receipts=0
for stem in "${runtime_names[@]}"; do
  canonical="$rust_prefix/lib/${stem}.dylib"
  rust_fixture="$rust_prefix/lib/${stem}_rust.dylib"
  zig_fixture="$rust_prefix/lib/${stem}_zig.dylib"
  if [[ "$stem" == libbirnam_id ]]; then
    rust_fixture="$rust_prefix/lib/${stem}.dylib"
  fi
  for path in "$canonical" "$zig_fixture"; do test -f "$path"; done
  if [[ "$stem" == libbirnam_id || "$stem" == libbirnam_store ]]; then
    rust_fixture=".cargo-target/release/$(rust_source_stem "$stem").dylib"
  fi
  test -f "$rust_fixture"
  canonical_digest=$(sha256 "$canonical")
  rust_digest=$(sha256 "$rust_fixture")
  zig_digest=$(sha256 "$zig_fixture")
  test "$canonical_digest" = "$rust_digest"
  test "$canonical_digest" != "$zig_digest"
  printf '{"schema":1,"prefix":"rust","library":"%s.dylib","canonicalSha256":"%s","rustFixtureSha256":"%s","zigFixtureSha256":"%s"}\n' \
    "$stem" "$canonical_digest" "$rust_digest" "$zig_digest"
  rust_receipts=$((rust_receipts + 1))

  canonical="$zig_prefix/lib/${stem}.dylib"
  rust_fixture="$zig_prefix/lib/${stem}_rust.dylib"
  zig_fixture="$zig_prefix/lib/${stem}_zig.dylib"
  if [[ "$stem" == libbirnam_id || "$stem" == libbirnam_store ]]; then
    zig_fixture="$zig_prefix/lib/${stem}_zig.dylib"
    rust_fixture=".cargo-target/release/$(rust_source_stem "$stem").dylib"
  elif [[ "$stem" == libbirnam_id_artifact ]]; then
    # The Zig-selected artifact branch intentionally omits the Rust artifact
    # install alias; use the independently-built Rust cdylib as its rollback
    # identity source instead of treating a missing alias as a mixed bundle.
    rust_fixture=".cargo-target/release/$(rust_source_stem "$stem").dylib"
  fi
  for path in "$canonical" "$rust_fixture" "$zig_fixture"; do test -f "$path"; done
  canonical_digest=$(sha256 "$canonical")
  rust_digest=$(sha256 "$rust_fixture")
  zig_digest=$(sha256 "$zig_fixture")
  if [[ "$stem" == libbirnam_store || "$stem" == libbirnam_id_alien ]]; then
    # Store and Alien are intentionally not selectable yet. The Zig host oracle
    # therefore records their Rust implementations as shared canonical
    # boundaries, while retaining the Zig fixtures for the later explicit
    # rollback-vector change.
    test "$canonical_digest" = "$rust_digest"
    test "$canonical_digest" != "$zig_digest"
    selection=rust-shared
  else
    test "$canonical_digest" = "$zig_digest"
    test "$canonical_digest" != "$rust_digest"
    selection=zig-oracle
  fi
  printf '{"schema":1,"prefix":"zig-oracle","selection":"%s","library":"%s.dylib","canonicalSha256":"%s","rustFixtureSha256":"%s","zigFixtureSha256":"%s"}\n' \
    "$selection" "$stem" "$canonical_digest" "$rust_digest" "$zig_digest"
  zig_receipts=$((zig_receipts + 1))
done

trace_prefix() {
  local implementation=$1
  local prefix=$2
  local work="$scratch/$implementation/decoy-work"
  local trace="$scratch/$implementation/load.trace"
  mkdir -p "$work/src/id"
  # Invalid source and dylib decoys make accidental cwd/checkout fallback fail
  # immediately. Runtime resolution must stay inside the installed prefix.
  for stem in "${runtime_names[@]}"; do
    printf 'not a dynamic library; decoy must never be opened\n' >"$work/${stem}.dylib"
  done
  while IFS= read -r path; do
    mkdir -p "$work/$(dirname "$path")"
    printf 'not a source provider; decoy must never be read\n' >"$work/$path"
  done < <(rg --files src/id -g '*.zig')
  (
    cd "$work"
    unset BIRNAM_LIB_DIR BIRNAM_ID_DYLIB DYLD_LIBRARY_PATH BIRNAM_FORCE_SOURCE
    printf ':quit\n' | DYLD_PRINT_LIBRARIES=1 "$prefix/bin/birnam" --repl \
      >"$scratch/$implementation/repl.out" 2>"$trace"
  )
  for stem in "${runtime_names[@]}"; do
    grep -F "$prefix/lib/${stem}.dylib" "$trace" >/dev/null
  done
  if grep -F "$work/" "$trace" >/dev/null || grep -E '_zig\.dylib|_rust\.dylib' "$trace" >/dev/null; then
    echo "remaining-zig-boundary: $implementation loaded a fixture/decoy path" >&2
    cat "$trace" >&2
    exit 1
  fi
}

trace_prefix rust "$rust_prefix"

# The Zig process is deliberately selected as an oracle/rollback prefix. It
# must load its canonical names, never an implicit Rust fixture, and its
# identity receipts above must remain distinct from the Rust canonical set.
oracle_trace="$scratch/zig-oracle/load.trace"
mkdir -p "$scratch/zig-oracle/work"
(
  cd "$scratch/zig-oracle/work"
  unset BIRNAM_LIB_DIR BIRNAM_ID_DYLIB DYLD_LIBRARY_PATH BIRNAM_FORCE_SOURCE
  printf ':quit\n' | DYLD_PRINT_LIBRARIES=1 "$zig_prefix/bin/birnam" --repl \
    >"$scratch/zig-oracle/repl.out" 2>"$oracle_trace"
)
for stem in "${runtime_names[@]}"; do
  grep -F "$zig_prefix/lib/${stem}.dylib" "$oracle_trace" >/dev/null
done
if grep -E '_rust\.dylib' "$oracle_trace" >/dev/null; then
  echo "remaining-zig-boundary: Zig oracle loaded a Rust fixture" >&2
  exit 1
fi

# This fixture mutates a .birnam-owned method and checks its reflected method
# marker. The Rust host may observe and validate the result, but it may not
# install a competing native vtable implementation.
policy_fixture="$repo/modules/run-tests.birnam"
policy_output="$scratch/policy.out"
(cd "$repo" && BIRNAM_LIB_DIR="$rust_prefix/lib" \
  "$rust_prefix/bin/birnam" "$policy_fixture" >"$policy_output")
if ! grep -q 'ok   policy edit is not a native primitive' "$policy_output" || \
  ! grep -Eq 'TOTAL: [0-9]+ passed, 0 failed' "$policy_output"; then
  echo "remaining-zig-boundary: policy fixture output did not prove the source-owned MOP edit" >&2
  cat "$policy_output" >&2
  exit 1
fi

# Keep the Rust/Zig artifact negative corpus tied to one normalized result
# contract while this boundary exercises relocated provider selection.
bash scripts/artifact-preflight-parity-smoke.sh

printf 'remaining-zig-boundary: rust_receipts=%d zig_oracle_receipts=%d canonical_load_trace=ok oracle_load_trace=ok policy_authority=ok\n' \
  "$rust_receipts" "$zig_receipts"
