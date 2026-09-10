#!/usr/bin/env bash
# Audit the installed Rust TCC dylib's identity, dependencies, and exports.
set -euo pipefail

dylib=${BIRNAM_RUST_TCC_DYLIB:?BIRNAM_RUST_TCC_DYLIB must point to the Rust TCC dylib}
test -f "$dylib"

expected=$(mktemp "${TMPDIR:-/tmp}/birnam-tcc-exports.XXXXXX")
actual=$(mktemp "${TMPDIR:-/tmp}/birnam-tcc-actual.XXXXXX")
deps=$(mktemp "${TMPDIR:-/tmp}/birnam-tcc-deps.XXXXXX")
trap 'rm -f "$expected" "$actual" "$deps"' EXIT

cat >"$expected" <<'EOF'
birnam_alloc
birnam_blockVt
birnam_cstr
birnam_ensurePrivateVtable
birnam_false
birnam_floatValue
birnam_foreignAddressValue
birnam_frame_slot_get_v1
birnam_frame_slot_put_v1
birnam_free_cstr
birnam_intern
birnam_isForeignAddress
birnam_layoutAllOops
birnam_method
birnam_mkFloat
birnam_mkForeignAddress
birnam_mkSmallInt
birnam_mkString
birnam_mkWord
birnam_nil
birnam_objectVt
birnam_proto
birnam_register_resumable_v1
birnam_register_resumable_v2
birnam_send
birnam_smallIntValue
birnam_symbol_borrow_v1
birnam_tcc_rust_identity
birnam_true
birnam_uintValue
birnam_wordValue
module_init
EOF
nm -gU "$dylib" | awk '{print $3}' | sed 's/^_//' | sort -u >"$actual"
diff -u "$expected" "$actual"
test "$(otool -D "$dylib" | tail -n 1)" = "@rpath/libbirnam_id_tcc_rust.dylib"

otool -L "$dylib" | tail -n +2 | sed 's/^[[:space:]]*//' | awk '{print $1}' >"$deps"
while IFS= read -r dependency; do
  case "$dependency" in
    @rpath/libbirnam_id_tcc_rust.dylib|/usr/lib/libSystem.B.dylib) ;;
    *) echo "rust-tcc-dylib-audit: forbidden dependency $dependency" >&2; exit 1 ;;
  esac
done <"$deps"

echo "rust-tcc-dylib-audit: identity, exports, install name, and dependencies passed"
