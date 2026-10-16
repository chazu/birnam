#!/usr/bin/env bash
# Run the birnam-level suite (modules/run-tests.birnam) against a built prefix.
#
# This is the gate for everything written IN birnam. `mise run verify` covers
# the Rust workspace and the build driver; it never loads a .birnam test file,
# so the two gates are complements, not substitutes.
#
#   BIRNAM_TEST_PREFIX  reuse an existing prefix instead of building one
#   BIRNAM_TEST_PROFILE build profile when building (default: release)
set -euo pipefail

cd "$(dirname "$0")/.."

if [ -n "${BIRNAM_TEST_PREFIX:-}" ]; then
  prefix="$BIRNAM_TEST_PREFIX"
else
  prefix_parent=$(mktemp -d "${TMPDIR:-/tmp}/birnam-suite.XXXXXX")
  trap 'rm -rf "$prefix_parent"' EXIT
  prefix="$prefix_parent/prefix"
  scripts/rust-install.sh install --root "$PWD" \
    --prefix "$prefix" --profile "${BIRNAM_TEST_PROFILE:-release}"
fi

test -x "$prefix/bin/birnam"

# Suites resolve modules/ and docs/guide relative to the working
# directory, so the runner has to see the repo root. run-tests.birnam gates its
# own exit code, and </dev/null keeps the runner off an interactive stdin.
"$prefix/bin/birnam" modules/run-tests.birnam </dev/null

# Private scratch must stay collectible while published Store records survive.
python3 scripts/scratch-state-test.py "$prefix"

"$prefix/bin/birnam" tests/fixtures/reflect-owner.birnam </dev/null
python3 scripts/code-inspector.py "$prefix" --samples 1
python3 scripts/local-cell-test.py "$prefix" --skip-images
python3 scripts/reflect-slots-test.py "$prefix" --skip-images
python3 scripts/module-registry-test.py "$prefix" --skip-images
"$prefix/bin/birnam" tests/fixtures/cell-owner.birnam </dev/null

python3 scripts/lsp-owner-test.py "$prefix" --skip-images

"$prefix/bin/birnam" tests/fixtures/globals-owner.birnam </dev/null

python3 scripts/owned-globals-test.py "$prefix" --skip-images

python3 scripts/ordinary-records-test.py "$prefix" --skip-images

echo "birnam-test: module suites passed"
