#!/usr/bin/env bash
# Installed Host acceptance. Each group owns a disposable prefix and fixtures.
set -euo pipefail
cd "$(dirname "$0")/.."
repo=$PWD
prefix=
group=all
while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix) prefix=$2; shift 2 ;;
    --group) group=$2; shift 2 ;;
    *) echo "usage: $0 [--prefix installed-prefix] [--group all|cli|image|interaction|generation|tasks]" >&2; exit 2 ;;
  esac
done
case "$group" in all|cli|image|interaction|generation|tasks) ;; *) echo "unknown Host test group: $group" >&2; exit 2 ;; esac
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-host-cli.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
if [[ -n "$prefix" ]]; then
  test -x "$prefix/bin/birnam"
  cp -R "$prefix" "$scratch/original"
else
  bash scripts/rust-install.sh install --root "$repo" --prefix "$scratch/original" \
    --build-cache "${BIRNAM_HOST_BUILD_CACHE:-$repo/.cargo-target/host-install}" \
    --profile "${BIRNAM_HOST_PROFILE:-release}"
fi
selected() { [[ $group == all || $group == "$1" ]]; }
if selected cli; then
  # This check performs the relocation and verifies every installed manifest byte.
  python3 scripts/host-cli-test.py "$scratch/original"
else
  mv "$scratch/original" "$scratch/relocated"
fi
export BIRNAM_TEST_MODULE_DIRECTORY="$scratch/relocated/lib"
export BIRNAM_BIN="$scratch/relocated/bin/birnam"
host_test() { cargo test --offline --locked -p birnam-host "$1" -- --ignored; }
if selected cli; then
  cargo test --offline --locked -p birnam-host --test native_owners -- --ignored
  cargo test --offline --locked -p birnam-host --test native_backend -- --ignored
  cargo test --offline --locked -p birnam-host --test native_compilers -- --ignored
  python3 scripts/artifact-cli-test.py "$scratch/relocated"
  python3 scripts/lang-migration-test.py "$scratch/relocated"
  python3 scripts/alien-compiler-test.py "$scratch/relocated"
  python3 scripts/image-format-test.py "$scratch/relocated"
  python3 scripts/doc-render-test.py "$scratch/relocated"
  python3 scripts/portable-execution-test.py "$scratch/relocated"
  python3 scripts/worker-test.py "$scratch/relocated"
  bash scripts/rust-project-check-smoke.sh
  cargo test --offline --locked -p birnam-project-build project_policy_values_survive_collecting_messages -- --ignored
  cargo test --offline --locked -p birnam-host-project project_source_observation_uses_live_messages -- --ignored
  host_test startup::tests::host_roots_survive_collecting_sends
  python3 scripts/project-query-test.py --prefix "$scratch/relocated" --output "$scratch/project-query.json"
fi
if selected image; then
  python3 scripts/host-image-test.py "$scratch/relocated"
  bash scripts/native-loader-image-smoke.sh "$scratch/relocated"
  python3 scripts/trace-image-smoke.py "$scratch/relocated"
  python3 scripts/frame-call-image-smoke.py "$scratch/relocated"
  python3 scripts/collection-search-image-test.py "$scratch/relocated" "$scratch/collection search images"
  bash scripts/string-builder-image-smoke.sh "$scratch/relocated"
  python3 scripts/system-image-test.py "$scratch/relocated"
  python3 scripts/alien-image-test.py "$scratch/relocated"
  BIRNAM_TEST_IMAGE="$scratch/image checks/main.image" \
    host_test startup::tests::image_entry_uses_saved_code_and_roots
  # Native image lifetime/resave tests run in native-message-test.sh with the
  # resource-metrics TCC build they require; the ordinary installed host keeps
  # those diagnostic counters disabled. Do not duplicate that gate here.
fi
if selected interaction; then
  host_test startup::tests::installed_bootstrap_and_console_do_not_compile_or_parse
  python3 scripts/interaction-cli-test.py "$scratch/relocated"
  bash scripts/repl-golden.sh
  bash scripts/lsp-project-smoke.sh
fi
if selected generation; then
  host_test host_fulfills_authored_generation_intents_at_idle_boundary
  host_test image_recovers_composed_generation_before_entry
  python3 scripts/generation-cli-test.py "$scratch/relocated"
  python3 scripts/compiled-unit-cli-test.py "$scratch/relocated"
fi

if selected tasks; then
  python3 scripts/task-image-test.py "$scratch/relocated"
  python3 scripts/concurrency-cli-test.py "$scratch/relocated"
  bash scripts/multicore-smoke.sh "$scratch/relocated"
  bash scripts/closure-observation-test.sh "$scratch/relocated"
  cargo test --offline --locked -p birnam-siphon --test task_control -- --ignored --nocapture
fi
