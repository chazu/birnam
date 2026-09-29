#!/usr/bin/env bash
# Real installer + relocated executable acceptance for the migrated source host.
set -euo pipefail
cd "$(dirname "$0")/.."
repo=$PWD
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-host-cli.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
bash scripts/rust-install.sh install --root "$repo" --prefix "$scratch/original" \
  --build-cache "${BIRNAM_HOST_BUILD_CACHE:-$repo/.cargo-target/host-install}" \
  --profile "${BIRNAM_HOST_PROFILE:-debug}"
python3 scripts/host-cli-test.py "$scratch/original"
BIRNAM_TEST_MODULE_DIRECTORY="$scratch/relocated/lib" \
  cargo test --offline --locked -p birnam-host startup::tests::host_roots_survive_collecting_sends -- --ignored
