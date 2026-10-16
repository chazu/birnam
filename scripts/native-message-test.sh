#!/usr/bin/env bash
# Exercise converted production modules through Siphon and Id, without old tables.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/.cargo-target/native-message-tests}"
mkdir -p "$CARGO_TARGET_DIR"
CARGO_TARGET_DIR=$(cd "$CARGO_TARGET_DIR" && pwd)
export CARGO_TARGET_DIR
# The native floor includes compiled Module/Lang artifacts. Build them with the
# explicit Rust grammar seed; the published test prefix excludes that seed.
if [[ -n ${BIRNAM_TEST_PREFIX:-} ]]; then
  prefix=$BIRNAM_TEST_PREFIX
else
  prefix_parent=$(mktemp -d "${TMPDIR:-/tmp}/birnam-native.XXXXXX")
  trap 'rm -rf "$prefix_parent"' EXIT
  prefix="$prefix_parent/prefix"
  BIRNAM_BUILD_CARGO_FEATURES=birnam-tcc/resource-metrics \
    bash scripts/rust-install.sh install --root "$PWD" --prefix "$prefix" \
      --profile release --build-cache "$CARGO_TARGET_DIR/install"
fi
export BIRNAM_TEST_MODULE_DIRECTORY="$prefix/lib"
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -fvisibility=hidden \
  rust/birnam-abi/tests/fixtures/id-protocol-v1/execution_method.c \
  -o "$BIRNAM_TEST_MODULE_DIRECTORY/libbirnam_execution_method_fixture.dylib"
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -fvisibility=hidden \
  rust/birnam-abi/tests/fixtures/id-protocol-v1/execution_machine.c \
  -o "$BIRNAM_TEST_MODULE_DIRECTORY/libbirnam_execution_machine_fixture.dylib"
for variant in target missing incompatible; do
  flags=(-std=c11 -Wall -Wextra -Werror -dynamiclib)
  case "$variant" in
    missing) flags+=(-DLOADER_MISSING) ;;
    incompatible) flags+=(-DLOADER_INCOMPATIBLE) ;;
  esac
  cc "${flags[@]}" \
    rust/birnam-abi/tests/fixtures/id-protocol-v1/native_loader.c \
    -o "$BIRNAM_TEST_MODULE_DIRECTORY/libbirnam_loader_${variant}_fixture.dylib"
done
cc -std=c11 -Wall -Wextra -Werror -dynamiclib rust/birnam-abi/tests/fixtures/id-protocol-v1/alien.c -o "$BIRNAM_TEST_MODULE_DIRECTORY/libbirnam_alien_fixture.dylib"
# Audit the actual converted dylibs; no installed legacy construction graph.
python3 - "$BIRNAM_TEST_MODULE_DIRECTORY" <<'PY_AUDIT'
import json
from pathlib import Path
import subprocess
import sys
modules = "id root-registry namespace boolean nil bootstrap collect object-model word bytes string integer float vector map execution peg tcc codegen-runtime native-backend generation store relational prelude system-output system-input environment file-io storage artifact portable-code image alien isolate host-entry host-project project-build host-interaction".split()
directory = Path(sys.argv[1])
for module in modules:
    library = directory / ("libbirnam_" + module.replace("-", "_") + ".dylib")
    manifest = json.loads(Path(f"rust/birnam-{module}/exports.json").read_text())
    actual = set(subprocess.check_output(["nm", "-gjU", library], text=True).splitlines())
    assert actual == {"_" + symbol for symbol in manifest["exported_symbols"]}, (module, actual)
    if "install_name" in manifest:
        install_names = subprocess.check_output(["otool", "-D", library], text=True).splitlines()[1:]
        assert install_names == [manifest["install_name"]], (module, install_names)
dependencies = subprocess.check_output(["otool", "-L", directory / "libbirnam_id.dylib"], text=True).splitlines()[1:]
assert {line.strip().split()[0] for line in dependencies} <= {"@rpath/libbirnam_id.dylib", "/usr/lib/libSystem.B.dylib"}, dependencies
metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--offline", "--locked", "--no-deps", "--format-version", "1"], text=True))
packages = {p["name"]: p for p in metadata["packages"]}
for module in modules + ["siphon", "abi", "codegen"]:
    dependencies = {d["name"] for d in packages["birnam-" + module]["dependencies"] if d["kind"] is None}
    expected = {"image": {"birnam-abi", "birnam-image-codec", "birnam-artifact-codec", "sha2"}, "portable-code": {"birnam-abi", "birnam-artifact-codec", "sha2"}, "codegen": {"birnam-artifact-codec", "sha2"}, "host-entry": {"birnam-abi", "birnam-host-support"}, "host-interaction": {"birnam-abi", "birnam-host-support"}, "host-project": {"birnam-abi", "birnam-host-support", "birnam-project-support"}, "project-build": {"birnam-abi", "birnam-host-support", "birnam-project-support", "birnam-artifact-codec", "serde", "serde_json"}, "string": {"birnam-abi"}, "bytes": {"birnam-abi", "sha2"}, "artifact": {"birnam-abi", "sha2"}, "abi": set(), "execution": {"birnam-abi", "birnam-artifact-codec"}, "tcc": {"birnam-abi", "birnam-native-compiler"}, "generation": {"birnam-abi", "birnam-codegen"}, "codegen-runtime": {"birnam-abi", "birnam-codegen"}, "native-backend": {"birnam-abi", "birnam-codegen", "birnam-artifact-codec", "sha2"}}
    assert dependencies == expected.get(module, {"birnam-abi"}), (module, dependencies)
print("native boundary audit: exports, install names and dependencies passed")
PY_AUDIT
CARGO_PROFILE_DEV_PANIC=unwind cargo test --offline --locked -p birnam-siphon --test modules --test memory --test memory_activation --test allocation_accounting --test external_allocation --test object_model --test object_model_scopes --test object_model_slots --test owned_globals --test text_error --test numbers --test numbers_loading --test native_values --test native_incoming --test boolean_values --test integer_values --test collections --test roots --test execution_methods --test shared_invocation_probe --test method_binding --test method_invocation --test artifact_templates --test execution_machines --test compiler --test store --test store_compensation --test relational --test system_output --test system_services --test system_gc --test storage_trace --test trace_graphs --test trace_heap --test method_observation --test publication_contract --test publication_failure_boundary --test publication_failure_image --test module_registry_retention --test local_cell_retention --test generation --test generation_ledger --test generation_transaction --test candidate --test candidate_retirement --test native_loader --test source_loader --test embedding --test tagged_rows --test block_loop --test artifact --test artifact_activation --test portable_code --test materialization --test image --test image_recovery_conflict --test image_recovery_owners --test image_snapshot_stamp --test alien --test isolate --test binding_proof --test byte_abi --test capture_access --test capture_dependencies --test closure_construction --test fixed_abi --test global_resolution --test guard_batch --test integer_box_observability --test integer_operation_objects --test invocation_inputs --test borrowed_read_inputs --test native_numeric_inputs --test representation --test representation_batch --test representation_fallback --test result_cleanup --test root_batches --test root_reuse --test word_expression --test word_helper --test word_representation_order -- --include-ignored

# The investigation fixture also runs its forwarding step as compiled Birnam.
BIRNAM_AUTHORED_SHARED_STEP=1 CARGO_PROFILE_DEV_PANIC=unwind cargo test --offline --locked -p birnam-siphon --test shared_invocation_probe -- --include-ignored

# Default construction uses transient source-owned templates, including after restore.
python3 scripts/method-invocation-test.py "$prefix"

# The exported production Cell programs must also work with Store absent.
python3 scripts/cell-without-store-test.py "$prefix" --registry

python3 scripts/globals-without-store-test.py "$prefix"
