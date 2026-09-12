#!/usr/bin/env bash
# Prove the Rust build driver can create and relocate a production prefix
# without consuming a prebuilt Birnam runtime artifact.
set -euo pipefail

cd "$(dirname "$0")/.."

prefix_parent=$(mktemp -d "${TMPDIR:-/tmp}/birnam-rust-driver-smoke.XXXXXX")
trap 'rm -rf "$prefix_parent"' EXIT
prefix="$prefix_parent/prefix"

scripts/rust-install.sh install --root "$PWD" --prefix "$prefix" --profile release

test -x "$prefix/bin/birnam"
test "$(find "$prefix/lib" -maxdepth 1 -type f -name 'libbirnam_id_*.dylib' | wc -l | tr -d ' ')" = 12
test "$(find "$prefix/lib" -maxdepth 1 -type f -name 'libbirnam_*.dylib' | wc -l | tr -d ' ')" = 42

run_root="$prefix_parent/run"
mkdir -p "$run_root"
cp examples/fizzbuzz.birnam "$run_root/fizzbuzz.birnam"
(cd /private/tmp && "$prefix/bin/birnam" "$run_root/fizzbuzz.birnam" </dev/null >"$prefix_parent/fizzbuzz.out")
grep -Fxq 'FizzBuzz' "$prefix_parent/fizzbuzz.out"
mkdir -p "$prefix_parent/home"
(
  cd /private/tmp
  /usr/bin/env -i PATH=/usr/bin:/bin HOME="$prefix_parent/home" \
    "$prefix/bin/birnam" "$run_root/fizzbuzz.birnam" </dev/null >"$prefix_parent/fizzbuzz-clean-env.out"
)
grep -Fxq 'FizzBuzz' "$prefix_parent/fizzbuzz-clean-env.out"

scripts/stackless-image-lifecycle-smoke.sh "$prefix"
scripts/portable-resumable-lifecycle-smoke.sh "$prefix"
scripts/portable-code-binding-lifecycle-smoke.sh "$prefix"
scripts/portable-image-v8-smoke.sh "$prefix"
scripts/stackless-control-smoke.sh "$prefix"
"$prefix/bin/birnam" scripts/fixtures/projection-recovery.birnam >"$prefix_parent/projection.out"
grep -Fxq "production-recovery: portable projections, captures, identity, eviction and GC passed" "$prefix_parent/projection.out"
cat "$prefix_parent/projection.out"
"$prefix/bin/birnam" scripts/fixtures/small-callee-recovery.birnam >"$prefix_parent/small-callee.out"
grep -Fxq "small-callee-recovery: aliases, captures, effects, eviction and GC passed" "$prefix_parent/small-callee.out"
cat "$prefix_parent/small-callee.out"
"$prefix/bin/birnam" scripts/fixtures/native-regions.birnam >"$prefix_parent/native-regions.out"
grep -Fxq "native-regions: copies, argument identity, replacement, eviction and GC passed" "$prefix_parent/native-regions.out"
cat "$prefix_parent/native-regions.out"
"$prefix/bin/birnam" scripts/fixtures/numeric-specialization.birnam >"$prefix_parent/numeric-specialization.out"
grep -Fxq "numeric-specialization: arithmetic, mixed values, errors, identity and retirement passed" "$prefix_parent/numeric-specialization.out"
cat "$prefix_parent/numeric-specialization.out"
cc -Iabi/fixtures/idapi-v1 scripts/fixtures/native-numeric-api-context.c -o "$prefix_parent/numeric-api-context"
"$prefix_parent/numeric-api-context" "$prefix/lib/libbirnam_id.dylib" "$prefix/lib/libbirnam_id_prelude.dylib"

python3 - "$prefix/share/birnam/rust-install.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    receipt = json.load(handle)
assert receipt["schema"] == 3
assert receipt["authority"] == "rust"
assert len(receipt["source_manifest_sha256"]) == 64
assert len(receipt["staged_manifest_sha256"]) == 64
assert receipt["admission_contract"]["image_version"] == 8
assert receipt["admission_contract"]["candidate_manifest_version"] == 4
assert len(receipt["admission_contract"]["id_api_fingerprint"]) == 64
assert receipt["publication"]["method"] == "atomic-sibling-rename"
assert receipt["publication"]["durability"] == "fsync-staged-files-and-directories-before-and-after-rename"
assert len(receipt["dynamic_libraries"]) == 15
assert len(receipt["stage1_modules"]) == 27
PY

echo "rust-driver-native-smoke: Rust-only release prefix, relocation, and stage1 closure passed"
