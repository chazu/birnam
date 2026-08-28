#!/usr/bin/env bash
# Rust-only source-build verification required before a production release.
set -euo pipefail

cd "$(dirname "$0")/.."

python3 scripts/release-changelog-test.py
python3 scripts/v014-performance-acceptance-test.py
cargo test --workspace --locked --offline
scripts/rust-driver-native-smoke.sh
scripts/rust-driver-cold-archive-smoke.sh

echo "rust-cutover-verify: workspace, native-prefix, cold-archive, and relocated-runtime gates passed"
