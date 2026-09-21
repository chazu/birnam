#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
bash scripts/runtime-services-test.sh "${1:-}" native-stack
CARGO_PROFILE_DEV_PANIC=unwind CARGO_TARGET_DIR=.cargo-target/runtime-services-tests mise exec -- cargo test --offline --locked -p birnam-runtime --lib runtime::tests::native_stack_anchor
if [[ $# -gt 0 ]]; then
  CARGO_PROFILE_DEV_PANIC=unwind CARGO_TARGET_DIR=.cargo-target/runtime-services-tests BIRNAM_NATIVE_STACK_PREFIX="$1" mise exec -- cargo test --offline --locked -p birnam-host --test native_stack -- --ignored
fi
