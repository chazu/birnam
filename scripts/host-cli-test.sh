#!/usr/bin/env bash
# Real installer + relocated executable acceptance for source, images and workers.
set -euo pipefail
cd "$(dirname "$0")/.."
repo=$PWD
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-host-cli.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
bash scripts/rust-install.sh install --root "$repo" --prefix "$scratch/original" \
  --build-cache "${BIRNAM_HOST_BUILD_CACHE:-$repo/.cargo-target/host-install}" \
  --profile "${BIRNAM_HOST_PROFILE:-release}"
python3 scripts/host-cli-test.py "$scratch/original"
python3 scripts/host-image-test.py "$scratch/relocated"
python3 scripts/artifact-cli-test.py "$scratch/relocated"
python3 scripts/worker-test.py "$scratch/relocated"
BIRNAM_BIN="$scratch/relocated/bin/birnam" bash scripts/rust-project-check-smoke.sh
BIRNAM_TEST_MODULE_DIRECTORY="$scratch/relocated/lib" \
  cargo test --offline --locked -p birnam-host project_policy_values_survive_collecting_messages -- --ignored
BIRNAM_TEST_MODULE_DIRECTORY="$scratch/relocated/lib" \
  cargo test --offline --locked -p birnam-host project_source_observation_uses_live_messages -- --ignored
BIRNAM_TEST_MODULE_DIRECTORY="$scratch/relocated/lib" \
  cargo test --offline --locked -p birnam-host startup::tests::host_roots_survive_collecting_sends -- --ignored
BIRNAM_TEST_MODULE_DIRECTORY="$scratch/relocated/lib" \
BIRNAM_TEST_IMAGE="$scratch/image checks/main.image" \
  cargo test --offline --locked -p birnam-host startup::tests::image_entry_uses_saved_code_and_roots -- --ignored
