#!/usr/bin/env bash
# Prove two independent source snapshots build the same Rust-only prefix
# without consuming checkout build products.
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-rust-cold-archive.XXXXXX")
trap 'rm -rf "$scratch"' EXIT

snapshot() {
  local destination=$1
  mkdir -p "$destination"
  tar -C "$repo" \
    --exclude='*.dylib' \
    --exclude='*.image' \
    -cf - VERSION Cargo.toml Cargo.lock rust src/id modules docs/guide scripts/rust-install.sh examples/fizzbuzz.birnam vendor/libtcc-src | tar -C "$destination" -xf -
  if find "$destination" -type f \( -name '*.dylib' -o -name '*.image' -o -path '*/target/*' -o -path '*/.cargo-target/*' \) -print -quit | grep -q .; then
    echo "rust-cold-archive: source snapshot contains a prebuilt artifact" >&2
    exit 1
  fi
}

snapshot "$scratch/source-a"
snapshot "$scratch/source-b"

"$scratch/source-a/scripts/rust-install.sh" install \
  --root "$scratch/source-a" \
  --prefix "$scratch/prefix-a" \
  --profile release
"$scratch/source-b/scripts/rust-install.sh" install \
  --root "$scratch/source-b" \
  --prefix "$scratch/prefix-b" \
  --profile release

for prefix in "$scratch/prefix-a" "$scratch/prefix-b"; do
  test -x "$prefix/bin/birnam"
done

python3 - "$scratch/prefix-a/share/birnam/rust-install.json" \
  "$scratch/prefix-b/share/birnam/rust-install.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    first = json.load(handle)
with open(sys.argv[2], encoding="utf-8") as handle:
    second = json.load(handle)

for receipt in (first, second):
    assert receipt["schema"] == 4
    assert receipt["authority"] == "rust"
    assert receipt["build_mode"] == "cold"
    assert receipt["profile"] == "release"
    assert "--features" not in receipt["cargo_invocation"]
    assert receipt["source_manifest_files"] > 0
    assert receipt["staged_manifest_files"] > 0
    assert receipt["publication"]["method"] == "atomic-sibling-rename"
    assert receipt["publication"]["durability"] == "fsync-staged-files-and-directories-before-and-after-rename"

assert first["source_manifest_sha256"] == second["source_manifest_sha256"]
assert first["staged_manifest_files"] == second["staged_manifest_files"]
assert set(first["staged_manifest"]) == set(second["staged_manifest"])
assert first["dynamic_libraries"] == second["dynamic_libraries"]
assert first["stage1_modules"] == second["stage1_modules"]
PY

for prefix in "$scratch/prefix-a" "$scratch/prefix-b"; do
  cp "$repo/examples/fizzbuzz.birnam" "$scratch/fizzbuzz.birnam"
  (cd /private/tmp && "$prefix/bin/birnam" "$scratch/fizzbuzz.birnam" </dev/null >"$scratch/fizzbuzz.out")
  grep -Fxq 'FizzBuzz' "$scratch/fizzbuzz.out"
done

echo "rust-cold-archive: two independent fixture-free Rust snapshots produced identical source/output closures and relocated runtimes"
