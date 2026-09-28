#!/usr/bin/env bash
# Exercise converted production modules through Siphon and Id, without old tables.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/.cargo-target/native-message-tests}"
mkdir -p "$CARGO_TARGET_DIR"
CARGO_TARGET_DIR=$(cd "$CARGO_TARGET_DIR" && pwd)
export CARGO_TARGET_DIR
cargo build --offline --locked --features birnam-tcc/resource-metrics -p birnam-id -p birnam-root-registry -p birnam-namespace -p birnam-boolean -p birnam-nil -p birnam-bootstrap -p birnam-collect -p birnam-object-model -p birnam-string -p birnam-error -p birnam-integer -p birnam-float -p birnam-word -p birnam-vector -p birnam-map -p birnam-range -p birnam-execution -p birnam-peg -p birnam-lang -p birnam-tcc -p birnam-codegen-runtime -p birnam-store -p birnam-relational -p birnam-prelude -p birnam-printing -p birnam-system-output -p birnam-system-input -p birnam-environment -p birnam-file-io -p birnam-system-gc -p birnam-native-loader -p birnam-source-loader -p birnam-block-loop -p birnam-artifact -p birnam-portable-code
export BIRNAM_TEST_MODULE_DIRECTORY="$CARGO_TARGET_DIR/debug"
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -fvisibility=hidden \
  abi/fixtures/id-protocol-v1/execution_method.c \
  -o "$BIRNAM_TEST_MODULE_DIRECTORY/libbirnam_execution_method_fixture.dylib"
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -fvisibility=hidden \
  abi/fixtures/id-protocol-v1/execution_machine.c \
  -o "$BIRNAM_TEST_MODULE_DIRECTORY/libbirnam_execution_machine_fixture.dylib"
for variant in target missing incompatible; do
  flags=()
  case "$variant" in
    missing) flags=(-DLOADER_MISSING) ;;
    incompatible) flags=(-DLOADER_INCOMPATIBLE) ;;
  esac
  cc -std=c11 -Wall -Wextra -Werror -dynamiclib "${flags[@]}" \
    abi/fixtures/id-protocol-v1/native_loader.c \
    -o "$BIRNAM_TEST_MODULE_DIRECTORY/libbirnam_loader_${variant}_fixture.dylib"
done
# Audit the actual converted dylibs; no installed legacy construction graph.
python3 - "$BIRNAM_TEST_MODULE_DIRECTORY" <<'PY_AUDIT'
import json
from pathlib import Path
import subprocess
import sys
modules = "id root-registry namespace boolean nil bootstrap collect object-model string error integer float word vector map range execution peg lang tcc codegen-runtime store relational prelude printing system-output system-input environment file-io system-gc native-loader source-loader block-loop artifact portable-code".split()
directory = Path(sys.argv[1])
for module in modules:
    library = directory / ("libbirnam_" + module.replace("-", "_") + ".dylib")
    manifest = json.loads(Path(f"abi/rust-{module}-dylib-v1.json").read_text())
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
    expected = {"portable-code": {"birnam-abi", "birnam-artifact-codec", "sha2"}, "codegen": {"birnam-artifact-codec", "sha2"}, "artifact": {"birnam-abi", "sha2"}, "abi": set(), "execution": {"birnam-abi", "birnam-artifact-codec"}, "tcc": {"birnam-abi", "birnam-native-compiler"}, "codegen-runtime": {"birnam-abi", "birnam-codegen"}}
    assert dependencies == expected.get(module, {"birnam-abi"}), (module, dependencies)
print("native boundary audit: exports, install names and dependencies passed")
PY_AUDIT
CARGO_PROFILE_DEV_PANIC=unwind cargo test --offline --locked -p birnam-siphon --test modules --test memory --test memory_activation --test allocation_accounting --test object_model --test text_error --test numbers --test numbers_loading --test collections --test roots --test execution_methods --test execution_machines --test compiler --test store --test relational --test system_output --test system_services --test system_gc --test native_loader --test source_loader --test embedding --test block_loop --test artifact --test portable_code -- --include-ignored
