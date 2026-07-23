#!/usr/bin/env bash
# r6-acceptance.sh — close Design 61 R6 against its exact accepted snapshot.
#
# This does not weaken the general Design 46 comparator. It proves that the
# current native-rung semantic/performance baseline still passes, that the
# committed, noise-qualified Zig/Rust pair is unchanged, and that its only
# threshold crossings are the two tradeoffs accepted for the Rust-floor
# cutover. Any changed result or additional regression fails closed.
set -euo pipefail

cd "$(dirname "$0")/.."

# Keep R6 acceptance aligned with the latest N0.5 test set. The checker is
# fail-closed and validates the semantic fixtures, accepted result pair, and
# artifact-inspector baseline before the raw comparator is run below.
python3 scripts/check-native-rung-baseline.py

zig_result="bench/results/v0.11.0-zig-2fe44d2-c8b632e8882358672a0de5a24897a735c09af2f080cb1aea55510e021fc3d6b1.json"
rust_result="bench/results/v0.11.0-rust-2fe44d2-239d348c297583789c4a4da8d5713ebcfd1b9ba1dbf12c04ecb44e56282ac0ea.json"
zig_result_sha256="237f42efcd8e555d6a3b59aa0a5f570fe0b2e3c594e6c57aff2918edbd062c0c"
rust_result_sha256="98e90eb7e5049b0e17fa73838c2b220888ab0285e1427a9fd2b43bafc5cc65ca"

verify_result() {
  local path="$1"
  local expected="$2"
  local actual

  [ -f "$path" ] || {
    echo "r6-acceptance: missing result: $path" >&2
    exit 1
  }
  actual=$(shasum -a 256 "$path" | awk '{print $1}')
  [ "$actual" = "$expected" ] || {
    echo "r6-acceptance: result digest mismatch: $path" >&2
    echo "  expected: $expected" >&2
    echo "  actual:   $actual" >&2
    exit 1
  }
}

verify_result "$zig_result" "$zig_result_sha256"
verify_result "$rust_result" "$rust_result_sha256"

comparison=$(mktemp "${TMPDIR:-/tmp}/birnam-r6-acceptance.XXXXXX")
trap 'rm -f "$comparison"' EXIT

set +e
./scripts/bench-compare.sh "$zig_result" "$rust_result" >"$comparison" 2>&1
compare_status=$?
set -e
cat "$comparison"

[ "$compare_status" -eq 1 ] || {
  echo "r6-acceptance: expected the raw comparator to report the recorded tradeoffs" >&2
  exit 1
}

regression_count=$(grep -c '^REG  ' "$comparison" || true)
[ "$regression_count" -eq 2 ] || {
  echo "r6-acceptance: expected exactly two recorded regressions, found $regression_count" >&2
  exit 1
}

grep -Fqx 'REG  query.aggregate: 1475835 -> 1562867 (+5.8%)' "$comparison" || {
  echo "r6-acceptance: query.aggregate no longer matches the accepted measurement" >&2
  exit 1
}
grep -Fqx 'REG  compile.stage1-stdlib: 39818 -> 44564 (+11.9%)' "$comparison" || {
  echo "r6-acceptance: compile.stage1-stdlib no longer matches the accepted measurement" >&2
  exit 1
}
grep -Fqx 'regressions: 2, missing: 0, unusable: 0' "$comparison" || {
  echo "r6-acceptance: comparison contains missing or unusable cases" >&2
  exit 1
}

echo "r6-acceptance: PASS"
echo "  accepted only for this snapshot: query.aggregate +5.8%, compile.stage1-stdlib +11.9%"
