#!/usr/bin/env bash
# Audit the Zig Codegen authority surface before the Rust port takes ownership.
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
dylib=${BIRNAM_CODEGEN_DYLIB:-"$repo/zig-out/lib/libbirnam_id_codegen.dylib"}
manifest="$repo/src/id/fixtures/codegen-authority-v1.json"
test -f "$dylib"
test -f "$manifest"

expected=$(mktemp "${TMPDIR:-/tmp}/birnam-codegen-exports.XXXXXX")
actual=$(mktemp "${TMPDIR:-/tmp}/birnam-codegen-actual.XXXXXX")
trap 'rm -f "$expected" "$actual"' EXIT

python3 - "$manifest" >"$expected" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    manifest = json.load(handle)
for symbol in sorted(manifest["exported_symbols"]):
    print(symbol)
PY

nm -gU "$dylib" |
  awk '$2 == "T" && $3 !~ /^_?__ubsan_handle_/ {print $3}' |
  sed 's/^_//' | sort -u >"$actual"
diff -u "$expected" "$actual"
echo "codegen-dylib-audit: exact exported Codegen authority surface passed"
