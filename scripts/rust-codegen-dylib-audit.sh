#!/usr/bin/env bash
# Audit the canonical Rust Codegen provider boundary.
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
dylib=${BIRNAM_RUST_CODEGEN_DYLIB:-"$repo/.cargo-target/release/libbirnam_codegen_runtime.dylib"}
manifest="$repo/rust/birnam-codegen-runtime/exports.json"
test -f "$dylib"
test -f "$manifest"

expected=$(mktemp "${TMPDIR:-/tmp}/birnam-rust-codegen-exports.XXXXXX")
actual=$(mktemp "${TMPDIR:-/tmp}/birnam-rust-codegen-actual.XXXXXX")
trap 'rm -f "$expected" "$actual"' EXIT

python3 - "$manifest" >"$expected" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    manifest = json.load(handle)
for symbol in sorted(manifest["exported_symbols"]):
    print(symbol)
PY

nm -gjU "$dylib" |
  sed 's/^_//' | sort -u >"$actual"
diff -u "$expected" "$actual"

install_name=$(otool -D "$dylib" | tail -n +2)
expected_install_name=$(python3 - "$manifest" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    print(json.load(handle)["install_name"])
PY
)
if [[ "$install_name" != "$expected_install_name" ]]; then
  echo "rust-codegen-dylib-audit: install name is $install_name" >&2
  exit 1
fi

unexpected_dependencies=$(otool -L "$dylib" | tail -n +2 | awk '{print $1}' |
  grep -Ev "^(${expected_install_name}|/usr/lib/libSystem[.]B[.]dylib)$" || true)
if [[ -n "$unexpected_dependencies" ]]; then
  printf 'rust-codegen-dylib-audit: unexpected dependencies:\n%s\n' "$unexpected_dependencies" >&2
  exit 1
fi

# The exact manifest above is authoritative; keep this explicit guard close to
# the boundary so a future manifest edit cannot accidentally fold another
# provider into canonical Rust Codegen without a visible review failure.
if grep -Eq 'birnam_(image|artifact)|rust_(image|artifact)' "$actual"; then
  echo "rust-codegen-dylib-audit: cross-rung Image/Artifact export leaked" >&2
  exit 1
fi

if strings "$dylib" | grep -Fq "$repo"; then
  echo "rust-codegen-dylib-audit: provider retains the repository path" >&2
  exit 1
fi

echo "rust-codegen-dylib-audit: exact canonical Rust Codegen boundary passed"
