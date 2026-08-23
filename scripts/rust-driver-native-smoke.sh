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
test "$(find "$prefix/lib" -maxdepth 1 -type f -name 'libbirnam_*.dylib' | wc -l | tr -d ' ')" = 40

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

python3 - "$prefix/share/birnam/rust-install.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    receipt = json.load(handle)
assert receipt["schema"] == 2
assert receipt["authority"] == "rust"
assert len(receipt["source_manifest_sha256"]) == 64
assert len(receipt["staged_manifest_sha256"]) == 64
assert receipt["publication"]["method"] == "atomic-sibling-rename"
assert receipt["publication"]["durability"] == "fsync-staged-files-and-directories-before-and-after-rename"
assert len(receipt["dynamic_libraries"]) == 14
assert len(receipt["stage1_modules"]) == 26
PY

echo "rust-driver-native-smoke: Rust-only release prefix, relocation, and stage1 closure passed"
