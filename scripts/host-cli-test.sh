#!/usr/bin/env bash
# Installed Host acceptance. Each group owns a disposable prefix and fixtures.
set -euo pipefail
cd "$(dirname "$0")/.."
repo=$PWD
prefix=
group=all
prepared=
timings=
while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix) prefix=$2; shift 2 ;;
    --group) group=$2; shift 2 ;;
    --prepared-binaries) prepared=$2; shift 2 ;;
    --timings) timings=$2; shift 2 ;;
    *) echo "usage: $0 [--prefix installed-prefix] [--group all|cli|image|interaction|generation|tasks]" >&2; exit 2 ;;
  esac
done
case "$group" in all|cli|image|interaction|generation|tasks) ;; *) echo "unknown Host test group: $group" >&2; exit 2 ;; esac
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-host-cli.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
timings=${timings:-$scratch/timings.json}
mkdir -p "$(dirname "$timings")"
printf '{"schema":1,"checks":[]}\n' >"$timings"
run_check() {
  local label=$1; shift
  python3 scripts/validation_timed.py "$timings" "$group:$label" -- "$@"
}
if [[ -z "$prepared" ]]; then
  run_check prepare python3 scripts/host_test_binaries.py prepare "$scratch/test-binaries"
  prepared="$scratch/test-binaries/manifest.json"
fi
rust_test() {
  local key=$1; shift
  run_check "$key:${1:-all}" python3 scripts/host_test_binaries.py run "$prepared" "$key" -- "$@"
}
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
  run_check host-cli-test.py python3 scripts/host-cli-test.py "$scratch/original"
else
  mv "$scratch/original" "$scratch/relocated"
fi
export BIRNAM_TEST_MODULE_DIRECTORY="$scratch/relocated/lib"
export BIRNAM_BIN="$scratch/relocated/bin/birnam"
host_test() { rust_test host-lib "$1" --ignored; }
if selected cli; then
  rust_test native-owners --ignored
  rust_test native-backend --ignored
  rust_test native-compilers --ignored
  run_check artifact-cli-test.py python3 scripts/artifact-cli-test.py "$scratch/relocated"
  run_check lang-migration-test.py python3 scripts/lang-migration-test.py "$scratch/relocated"
  run_check alien-compiler-test.py python3 scripts/alien-compiler-test.py "$scratch/relocated"
  run_check image-format-test.py python3 scripts/image-format-test.py "$scratch/relocated"
  run_check doc-render-test.py python3 scripts/doc-render-test.py "$scratch/relocated"
  run_check portable-execution-test.py python3 scripts/portable-execution-test.py "$scratch/relocated"
  run_check worker-test.py python3 scripts/worker-test.py "$scratch/relocated"
  run_check rust-project-check-smoke.sh bash scripts/rust-project-check-smoke.sh
  rust_test project-build project_policy_values_survive_collecting_messages --ignored
  rust_test host-project project_source_observation_uses_live_messages --ignored
  host_test startup::tests::host_roots_survive_collecting_sends
  run_check project-query-test.py python3 scripts/project-query-test.py --prefix "$scratch/relocated" --output "$scratch/project-query.json" --prepared-binaries "$prepared"
fi
if selected image; then
  run_check host-image-test.py python3 scripts/host-image-test.py "$scratch/relocated"
  run_check native-loader-image-smoke.sh bash scripts/native-loader-image-smoke.sh "$scratch/relocated"
  run_check trace-image-smoke.py python3 scripts/trace-image-smoke.py "$scratch/relocated"
  run_check frame-call-image-smoke.py python3 scripts/frame-call-image-smoke.py "$scratch/relocated"
  run_check collection-search-image-test.py python3 scripts/collection-search-image-test.py "$scratch/relocated" "$scratch/collection search images"
  run_check string-builder-image-smoke.sh bash scripts/string-builder-image-smoke.sh "$scratch/relocated"
  run_check system-image-test.py python3 scripts/system-image-test.py "$scratch/relocated"
  run_check alien-image-test.py python3 scripts/alien-image-test.py "$scratch/relocated"
  BIRNAM_TEST_IMAGE="$scratch/image checks/main.image" \
    host_test startup::tests::image_entry_uses_saved_code_and_roots
  # Native image lifetime/resave tests run in native-message-test.sh with the
  # resource-metrics TCC build they require; the ordinary installed host keeps
  # those diagnostic counters disabled. Do not duplicate that gate here.
fi
if selected interaction; then
  run_check socket-owner-test.py python3 scripts/socket-owner-test.py "$scratch/relocated"
  host_test startup::tests::installed_bootstrap_and_console_do_not_compile_or_parse
  run_check interaction-cli-test.py python3 scripts/interaction-cli-test.py "$scratch/relocated"
  run_check repl-golden.sh bash scripts/repl-golden.sh
  run_check lsp-project-smoke.sh bash scripts/lsp-project-smoke.sh
fi
if selected generation; then
  host_test direct_method_observations_restore_native_abi_and_owned_identity
  host_test candidate_task_exit_retains_explicit_undo_and_releases_operation_scope
  host_test registry_validation_uses_one_metadata_read_and_current_query
  host_test source_registry_images_preserve_aliases_and_reject_changed_snapshots
  host_test source_registry_sends_retain_partial_edits_for_explicit_undo
  host_test image_application_boundary_invalidates_without_restore
  host_test certificate_release_reclaims_observations_and_preserves_candidate_undo
  host_test discard_cancels_preparation_and_keeps_bindings_and_files
  host_test independent_candidate_modules_need_no_certificate
  host_test rollback_keeps_released_predecessors_discarded
  host_test dependency_changes_use_registered_module_without_certification
  host_test activation_inside_entered_methods_uses_current_setters
  host_test lifecycle_fields_preserve_explicit_facts_and_status_reads
  run_check lifecycle-record-image-test.py python3 scripts/lifecycle-record-image-test.py "$scratch/relocated"
  host_test image_restores_descriptions_and_explicit_adoption
  run_check generation-cli-test.py python3 scripts/generation-cli-test.py "$scratch/relocated"
  run_check compiled-unit-cli-test.py python3 scripts/compiled-unit-cli-test.py "$scratch/relocated"
fi

if selected tasks; then
  run_check task-image-test.py python3 scripts/task-image-test.py "$scratch/relocated"
  run_check task-fields-test.py python3 scripts/task-fields-test.py "$scratch/relocated"
  run_check concurrency-cli-test.py python3 scripts/concurrency-cli-test.py "$scratch/relocated"
  run_check multicore-smoke.sh bash scripts/multicore-smoke.sh "$scratch/relocated"
  run_check closure-observation-test.sh bash scripts/closure-observation-test.sh "$scratch/relocated"
  rust_test task-control --ignored --nocapture
fi
