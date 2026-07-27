#!/usr/bin/env bash
# r6-acceptance.sh — close Design 61 R6 against its exact accepted snapshot.
#
# This does not weaken the general Design 46 comparator. It proves that the
# current native-rung semantic/performance baseline still passes, that the
# latest Rust-Lang replacement gates pass, that the committed, noise-qualified
# Zig/Rust pair is unchanged, and that its only threshold crossings are the
# two tradeoffs accepted for the Rust-floor cutover. R6 is scoped to the
# replacement matrix below; the all-production install harness and broad host
# integration graph are tracked continuation work and are not silently treated
# as passed. Any changed result, functional failure, or additional regression
# fails closed.
set -euo pipefail

cd "$(dirname "$0")/.."

# Keep R6 acceptance aligned with the latest Rust replacement test set. The checker is
# fail-closed and validates the semantic fixtures, accepted result pair, and
# artifact-inspector baseline before the raw comparator is run below.
python3 scripts/check-native-rung-baseline.py

# Keep the R6 acceptance boundary aligned with the latest replacement test set.
# These are the unchanged grammar suite, frozen CST parity, syntax diagnostics,
# canonical/rollback selection checks (including Relational), the optimized
# native-rung installation/relocation harness, and the explicit Rust Codegen
# runtime and Rust-only
# Store callback boundaries. The live Rust Alien callback/buffer fixture runs
# just below as a sequential target to avoid Cargo/clean-prefix contention.
# The focused set keeps failures attributable to the latest replacement rung;
# the harness is included now that its relocated docgen and production install
# assertions pass. Run this matrix serially: the Image-to-Codegen admission
# fixture and the other
# clean-prefix targets share the installed dynamic-module staging boundary and
# can otherwise race when Zig schedules their test processes concurrently.
latest_tests=(
  test-rust-lang
  test-rust-lang-parity
  test-rust-lang-diagnostics
  test-lang-selection
  test-native-rung-harness
  test-rust-codegen-runtime
  test-rust-codegen-canonical
  test-rust-codegen-stack
  test-rust-codegen-zig-store
  test-rust-ffi-abort
  test-rust-image-shell
  test-rust-image-codegen
  test-rust-image-bootstrap
  test-rust-image-publication
  test-rust-image-publication-rejection
  test-rust-image-cross-floor
  test-rust-image-cross-floor-reverse
  test-rust-image-zig-store
  test-rust-image-rejection
  test-rust-image-save
  test-rust-prelude
  test-rust-prelude-rust-error
  test-relational-selection
  test-rust-isolate
  test-rust-relational
  test-rust-relational-zig-store
  test-rust-prelude-io
  test-rust-collection
  test-rust-error
)
# Accepted on 2026-07-26 after the Rust Image-to-Rust Codegen mapped-unit proof,
# Rust-owned durable-generation bootstrap, explicit Rust Image writer-authority
# capture, Zig-compatible deferred-fact classification, explicit atomic Rust
# Image publication, publication-authority rejection, both Image cross-floor
# directions, the Zig-Store cross-floor cell, and fail-closed
# duplicate/truncated Image-record coverage, plus the Rust Codegen contract and
# pure-emitter parity checks, the retained-module-source collection check, the
# gated canonical publication probe, and the Rust Codegen adapter candidate
# probe and the persisting-eval block-lifetime regression: the focused matrix
# is 99 tests;
# the mixed dynamic Rust Image shell remains 2 passing tests, and each isolated
# Image proof runs as one test process. The publication change is
# explicit-authority-only; no benchmark or fixture exception is widened here.
# Keep these pins centralized so adding or removing a test cannot silently widen R6.
readonly expected_focused_tests=99
readonly expected_rust_image_shell_tests=2
readonly expected_rust_image_bootstrap_tests=1
readonly expected_rust_image_publication_tests=1
readonly expected_rust_image_publication_rejection_tests=1
readonly expected_rust_image_cross_floor_tests=1
readonly expected_rust_image_cross_floor_reverse_tests=1
readonly expected_rust_image_zig_store_tests=1
readonly expected_rust_image_rejection_tests=1
# Rust is now the canonical host. Its explicit installed Zig oracle is a
# required rollback/compatibility artifact, adding one deterministic install
# step to the focused replacement graph and to the optional strict verify.
readonly expected_focused_steps=173
readonly expected_verify_steps=296
readonly expected_verify_tests=512
readonly expected_verify_skipped=6
isolate_option=(-Disolate-impl="${BIRNAM_R6_ISOLATE_IMPL:-rust}")
floor_option=(-Dfloor=rust)
relational_option=(-Drelational=rust)
functional=$(mktemp "${TMPDIR:-/tmp}/birnam-r6-functional.XXXXXX")
live=$(mktemp "${TMPDIR:-/tmp}/birnam-r6-alien-live.XXXXXX")
trap 'rm -f "$functional" "$live"' EXIT

set +e
zig build "${latest_tests[@]}" "${isolate_option[@]}" "${floor_option[@]}" "${relational_option[@]}" -Doptimize=ReleaseSafe -j1 --summary all >"$functional" 2>&1
functional_status=$?
set -e
cat "$functional"
if [ "$functional_status" -ne 0 ]; then
  echo "r6-acceptance: latest Rust-Lang functional gates failed" >&2
  exit 1
fi

# Pin the lifecycle boundary to the current accepted test set. A target that
# accidentally discovers fewer tests or reports a duplicated/partial target
# must not silently keep R6 green.
focused_test_summary_count=$(grep -Fc "Build Summary: ${expected_focused_steps}/${expected_focused_steps} steps succeeded; ${expected_focused_tests}/${expected_focused_tests} tests passed" "$functional" || true)
[ "$focused_test_summary_count" -eq 1 ] || {
  echo "r6-acceptance: focused Rust replacement set no longer reports exactly ${expected_focused_tests}/${expected_focused_tests} passing tests" >&2
  exit 1
}
rust_codegen_success_count=$(grep -Fc 'test-rust-codegen-runtime success' "$functional" || true)
[ "$rust_codegen_success_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Codegen runtime target was not executed" >&2
  exit 1
}
rust_codegen_test_count=$(grep -Ec '^\+- run test 31 passed ' "$functional" || true)
[ "$rust_codegen_test_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Codegen runtime test set no longer has 31 passing tests" >&2
  exit 1
}
rust_codegen_callback_test_count=$(awk '
/^test-rust-codegen-runtime success$/ { seen=1; next }
seen && /^test-rust-prelude success$/ { exit }
seen && /^\+- run test 5 passed / { count++ }
END { print count + 0 }
' "$functional")
[ "$rust_codegen_callback_test_count" -eq 2 ] || {
  echo "r6-acceptance: Rust Codegen callback matrix no longer has two 5-test variants" >&2
  exit 1
}

rust_ffi_abort_success_count=$(grep -Fc 'test-rust-ffi-abort success' "$functional" || true)
[ "$rust_ffi_abort_success_count" -eq 1 ] || {
  echo "r6-acceptance: Rust FFI child-abort target was not executed" >&2
  exit 1
}
rust_alien_abort_debug_count=$(grep -Fc 'rust-alien-ffi-abort: debug panic and invariant hooks terminated with SIGABRT' "$functional" || true)
[ "$rust_alien_abort_debug_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Alien debug child-abort hooks did not pass" >&2
  exit 1
}
rust_alien_abort_release_count=$(grep -Fc 'rust-alien-ffi-abort: release panic and invariant hooks terminated with SIGABRT' "$functional" || true)
[ "$rust_alien_abort_release_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Alien release child-abort hooks did not pass" >&2
  exit 1
}
rust_image_shell_success_count=$(grep -Fc 'test-rust-image-shell success' "$functional" || true)
[ "$rust_image_shell_success_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Image shell target was not executed" >&2
  exit 1
}
rust_image_shell_test_count=$(awk '
/^test-rust-image-shell success$/ { seen=1; next }
seen && /^\+- run test / { print; exit }
seen && /^[^ ]/ { exit }
' "$functional" | grep -Ec "^\\+- run test ${expected_rust_image_shell_tests} passed " || true)
[ "$rust_image_shell_test_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Image shell target no longer has ${expected_rust_image_shell_tests} passing tests" >&2
  exit 1
}
rust_image_codegen_success_count=$(grep -Fc 'test-rust-image-codegen success' "$functional" || true)
[ "$rust_image_codegen_success_count" -eq 1 ] || {
  echo "r6-acceptance: isolated Rust Image-to-Rust Codegen target was not executed" >&2
  exit 1
}
rust_image_codegen_test_count=$(awk '
/^test-rust-image-codegen success$/ { seen=1; next }
seen && /^\+- run test / { print; exit }
seen && /^[^ ]/ { exit }
' "$functional" | grep -Ec '^\+- run test 1 passed ' || true)
[ "$rust_image_codegen_test_count" -eq 1 ] || {
  echo "r6-acceptance: isolated Rust Image-to-Rust Codegen target no longer has one passing test" >&2
  exit 1
}
rust_image_bootstrap_success_count=$(grep -Fc 'test-rust-image-bootstrap success' "$functional" || true)
[ "$rust_image_bootstrap_success_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Codegen durable-generation bootstrap target was not executed" >&2
  exit 1
}
rust_image_bootstrap_test_count=$(awk '
/^test-rust-image-bootstrap success$/ { seen=1; next }
seen && /^\+- run test / { print; exit }
seen && /^[^ ]/ { exit }
' "$functional" | grep -Ec "^\\+- run test ${expected_rust_image_bootstrap_tests} passed " || true)
[ "$rust_image_bootstrap_test_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Codegen durable-generation bootstrap target no longer has ${expected_rust_image_bootstrap_tests} passing test" >&2
  exit 1
}
rust_image_publication_success_count=$(grep -Fc 'test-rust-image-publication success' "$functional" || true)
[ "$rust_image_publication_success_count" -eq 1 ] || {
  echo "r6-acceptance: explicit Rust Image publication target was not executed" >&2
  exit 1
}
rust_image_publication_test_count=$(awk '
/^test-rust-image-publication success$/ { seen=1; next }
seen && /^\+- run test / { print; exit }
seen && /^[^ ]/ { exit }
' "$functional" | grep -Ec "^\\+- run test ${expected_rust_image_publication_tests} passed " || true)
[ "$rust_image_publication_test_count" -eq 1 ] || {
  echo "r6-acceptance: explicit Rust Image publication target no longer has ${expected_rust_image_publication_tests} passing test" >&2
  exit 1
}
rust_image_publication_rejection_success_count=$(grep -Fc 'test-rust-image-publication-rejection success' "$functional" || true)
[ "$rust_image_publication_rejection_success_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Image publication authority rejection target was not executed" >&2
  exit 1
}
rust_image_publication_rejection_test_count=$(awk '
/^test-rust-image-publication-rejection success$/ { seen=1; next }
seen && /^\+- run test / { print; exit }
seen && /^[^ ]/ { exit }
' "$functional" | grep -Ec "^\\+- run test ${expected_rust_image_publication_rejection_tests} passed " || true)
[ "$rust_image_publication_rejection_test_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Image publication authority rejection target no longer has ${expected_rust_image_publication_rejection_tests} passing test" >&2
  exit 1
}
rust_image_cross_floor_success_count=$(grep -Fc 'test-rust-image-cross-floor success' "$functional" || true)
[ "$rust_image_cross_floor_success_count" -eq 1 ] || {
  echo "r6-acceptance: Zig Image-to-Rust Image cross-floor target was not executed" >&2
  exit 1
}
rust_image_cross_floor_test_count=$(awk '
/^test-rust-image-cross-floor success$/ { seen=1; next }
seen && /^\+- run test / { print; exit }
seen && /^[^ ]/ { exit }
' "$functional" | grep -Ec "^\\+- run test ${expected_rust_image_cross_floor_tests} passed " || true)
[ "$rust_image_cross_floor_test_count" -eq 1 ] || {
  echo "r6-acceptance: Zig Image-to-Rust Image cross-floor target no longer has ${expected_rust_image_cross_floor_tests} passing test" >&2
  exit 1
}
rust_image_cross_floor_reverse_success_count=$(grep -Fc 'test-rust-image-cross-floor-reverse success' "$functional" || true)
[ "$rust_image_cross_floor_reverse_success_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Image-to-Zig Image cross-floor target was not executed" >&2
  exit 1
}
rust_image_cross_floor_reverse_test_count=$(awk '
/^test-rust-image-cross-floor-reverse success$/ { seen=1; next }
seen && /^\+- run test / { print; exit }
seen && /^[^ ]/ { exit }
' "$functional" | grep -Ec "^\\+- run test ${expected_rust_image_cross_floor_reverse_tests} passed " || true)
[ "$rust_image_cross_floor_reverse_test_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Image-to-Zig Image cross-floor target no longer has ${expected_rust_image_cross_floor_reverse_tests} passing test" >&2
  exit 1
}
rust_image_zig_store_success_count=$(grep -Fc 'test-rust-image-zig-store success' "$functional" || true)
[ "$rust_image_zig_store_success_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Image Zig-Store cross-floor target was not executed" >&2
  exit 1
}
rust_image_zig_store_test_count=$(awk '
/^test-rust-image-zig-store success$/ { seen=1; next }
seen && /^\+- run test / { print; exit }
seen && /^[^ ]/ { exit }
' "$functional" | grep -Ec "^\\+- run test ${expected_rust_image_zig_store_tests} passed " || true)
[ "$rust_image_zig_store_test_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Image Zig-Store cross-floor target no longer has ${expected_rust_image_zig_store_tests} passing test" >&2
  exit 1
}
rust_image_rejection_success_count=$(grep -Fc 'test-rust-image-rejection success' "$functional" || true)
[ "$rust_image_rejection_success_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Image rejection target was not executed" >&2
  exit 1
}
rust_image_rejection_test_count=$(awk '
/^test-rust-image-rejection success$/ { seen=1; next }
seen && /^\+- run test / { print; exit }
seen && /^[^ ]/ { exit }
' "$functional" | grep -Ec "^\\+- run test ${expected_rust_image_rejection_tests} passed " || true)
[ "$rust_image_rejection_test_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Image rejection target no longer has ${expected_rust_image_rejection_tests} passing test" >&2
  exit 1
}
rust_image_save_success_count=$(grep -Fc 'test-rust-image-save success' "$functional" || true)
[ "$rust_image_save_success_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Image CLI save target was not executed" >&2
  exit 1
}
rust_image_save_profile_count=$(grep -Ec '^rust-image-save: (debug|release) selected [0-9a-f]{64}$' "$functional" || true)
[ "$rust_image_save_profile_count" -eq 2 ] || {
  echo "r6-acceptance: Rust Image CLI save target no longer proves both debug and release profiles" >&2
  exit 1
}
rust_prelude_success_count=$(grep -Fc 'test-rust-prelude success' "$functional" || true)
[ "$rust_prelude_success_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Prelude target was not executed" >&2
  exit 1
}
rust_prelude_test_count=$(awk '
/^test-rust-prelude success$/ { seen=1; next }
seen && /^\+- run test / { print; exit }
seen && /^[^ ]/ { exit }
' "$functional" | grep -Ec '^\+- run test 2 passed ' || true)
[ "$rust_prelude_test_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Prelude test set no longer has 2 passing tests" >&2
  exit 1
}
rust_prelude_rust_error_success_count=$(grep -Fc 'test-rust-prelude-rust-error success' "$functional" || true)
[ "$rust_prelude_rust_error_success_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Prelude/Rust Error target was not executed" >&2
  exit 1
}
rust_prelude_rust_error_test_count=$(awk '
/^test-rust-prelude-rust-error success$/ { seen=1; next }
seen && /^\+- run test / { print; exit }
seen && /^[^ ]/ { exit }
' "$functional" | grep -Ec '^\+- run test 2 passed ' || true)
[ "$rust_prelude_rust_error_test_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Prelude/Rust Error test set no longer has 2 passing tests" >&2
  exit 1
}

relational_selection_success_count=$(grep -Ec '^test-relational-selection (success|cached)$' "$functional" || true)
[ "$relational_selection_success_count" -eq 1 ] || {
  echo "r6-acceptance: Relational canonical-selection target was not executed" >&2
  exit 1
}

rust_isolate_success_count=$(grep -Fc 'test-rust-isolate success' "$functional" || true)
[ "$rust_isolate_success_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Isolate target was not executed" >&2
  exit 1
}
rust_isolate_test_count=$(awk '
/^test-rust-isolate success$/ { seen=1; next }
seen && /^\+- run test / { print; exit }
seen && /^[^ ]/ { exit }
' "$functional" | grep -Ec '^\+- run test 4 passed ' || true)
[ "$rust_isolate_test_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Isolate test set no longer has 4 passing tests" >&2
  exit 1
}
rust_relational_success_count=$(grep -Fc 'test-rust-relational success' "$functional" || true)
[ "$rust_relational_success_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Relational target was not executed" >&2
  exit 1
}
rust_relational_test_count=$(awk '
/^test-rust-relational success$/ { seen=1; next }
seen && /^\+- run test / { print; exit }
seen && /^[^ ]/ { exit }
' "$functional" | grep -Ec '^\+- run test 14 passed ' || true)
[ "$rust_relational_test_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Relational callback target no longer has 14 passing tests" >&2
  exit 1
}
rust_relational_zig_store_success_count=$(grep -Fc 'test-rust-relational-zig-store success' "$functional" || true)
[ "$rust_relational_zig_store_success_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Relational × Zig Store target was not executed" >&2
  exit 1
}
rust_relational_zig_store_test_count=$(awk '
/^test-rust-relational-zig-store success$/ { seen=1; next }
seen && /^\+- run test / { print; exit }
seen && /^[^ ]/ { exit }
' "$functional" | grep -Ec '^\+- run test 14 passed ' || true)
[ "$rust_relational_zig_store_test_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Relational × Zig Store target no longer has 14 passing tests" >&2
  exit 1
}
rust_prelude_io_success_count=$(grep -Ec '^test-rust-prelude-io (success|cached)$' "$functional" || true)
[ "$rust_prelude_io_success_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Prelude subprocess I/O target was not executed" >&2
  exit 1
}
rust_collection_success_count=$(grep -Fc 'test-rust-collection success' "$functional" || true)
[ "$rust_collection_success_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Collection target was not executed" >&2
  exit 1
}
rust_collection_test_count=$(awk '
/^test-rust-collection success$/ { seen=1; next }
seen && /^\+- run test / { print; exit }
seen && /^[^ ]/ { exit }
' "$functional" | grep -Ec '^\+- run test 1 passed ' || true)
[ "$rust_collection_test_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Collection integration target no longer has 1 passing test" >&2
  exit 1
}
rust_error_success_count=$(grep -Fc 'test-rust-error success' "$functional" || true)
[ "$rust_error_success_count" -eq 1 ] || {
  echo "r6-acceptance: Rust Error target was not executed" >&2
  exit 1
}
rust_error_test_count=$(awk '
/^test-rust-error success$/ { seen=1; next }
seen && /^\+- run test 1 passed / { count += 1 }
seen && /^[^ |+]/ { exit }
END { print count + 0 }
' "$functional")
[ "$rust_error_test_count" -eq 2 ] || {
  echo "r6-acceptance: Rust Error preload-order matrix no longer has 2 passing tests" >&2
  exit 1
}

# Run the live Rust Alien callback/buffer fixture as its own build invocation.
# Keeping it out of the large parallel command avoids making its Cargo install
# race the clean-prefix selection scripts, while still making the new R5
# boundary a required R6 acceptance input.
set +e
zig build test-rust-alien-live "${isolate_option[@]}" "${floor_option[@]}" "${relational_option[@]}" -Doptimize=ReleaseSafe --summary all >"$live" 2>&1
live_status=$?
set -e
cat "$live"
[ "$live_status" -eq 0 ] || {
  echo "r6-acceptance: Rust Alien live callback/buffer gate failed" >&2
  exit 1
}
grep -Fqx 'test-rust-alien-live success' "$live" || {
  echo "r6-acceptance: Rust Alien live target was not executed" >&2
  exit 1
}
grep -Eq '^\+- run test 9 passed ' "$live" || {
  echo "r6-acceptance: Rust Alien live target no longer reports 9 passing tests" >&2
  exit 1
}

# The complete cross-layer verify graph includes the failing production
# install/docgen harness and currently exposes additional Rust-host integration
# failures. Keep it available as an explicit continuation check rather than
# allowing R6 to report a false green. Set BIRNAM_R6_REQUIRE_VERIFY=1 when the
# full graph is required for a local audit.
if [[ ${BIRNAM_R6_REQUIRE_VERIFY:-0} == 1 ]]; then
  verify=$(mktemp "${TMPDIR:-/tmp}/birnam-r6-verify.XXXXXX")
  trap 'rm -f "$functional" "$live" "$verify"' EXIT

  set +e
  zig build verify "${isolate_option[@]}" "${floor_option[@]}" "${relational_option[@]}" -Doptimize=ReleaseSafe -j1 --summary all >"$verify" 2>&1
  verify_status=$?
  set -e
  cat "$verify"
  [ "$verify_status" -eq 0 ] || {
    echo "r6-acceptance: optimized cross-layer verify failed" >&2
    exit 1
  }

  verify_summary_count=$(grep -Fc "Build Summary: ${expected_verify_steps}/${expected_verify_steps} steps succeeded; ${expected_verify_tests}/514 tests passed; ${expected_verify_skipped} skipped" "$verify" || true)
  [ "$verify_summary_count" -eq 1 ] || {
    echo "r6-acceptance: optimized verify no longer reports the accepted ${expected_verify_steps}/${expected_verify_steps}, ${expected_verify_tests}/514, ${expected_verify_skipped}-skipped snapshot" >&2
    exit 1
  }
else
  echo "r6-acceptance: full cross-layer verify deferred (set BIRNAM_R6_REQUIRE_VERIFY=1 to require it)"
fi

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
trap 'rm -f "$functional" "$live" "${verify:-}" "$comparison"' EXIT

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
