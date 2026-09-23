#!/usr/bin/env bash
# Exercise converted production modules through Siphon and Id, without old tables.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/.cargo-target/native-message-tests}"
mkdir -p "$CARGO_TARGET_DIR"
CARGO_TARGET_DIR=$(cd "$CARGO_TARGET_DIR" && pwd)
export CARGO_TARGET_DIR
cargo build --offline --locked -p birnam-id -p birnam-root-registry -p birnam-namespace -p birnam-boolean -p birnam-nil -p birnam-bootstrap -p birnam-collect -p birnam-object-model -p birnam-string -p birnam-error -p birnam-integer -p birnam-float -p birnam-word -p birnam-vector -p birnam-map -p birnam-range -p birnam-execution
export BIRNAM_TEST_MODULE_DIRECTORY="$CARGO_TARGET_DIR/debug"
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -fvisibility=hidden \
  abi/fixtures/id-protocol-v1/execution_method.c \
  -o "$BIRNAM_TEST_MODULE_DIRECTORY/libbirnam_execution_method_fixture.dylib"
# Audit the actual converted dylibs; no installed legacy construction graph.
python3 - "$BIRNAM_TEST_MODULE_DIRECTORY" <<'PY_AUDIT'
import json
from pathlib import Path
import subprocess
import sys
modules = "id root-registry namespace boolean nil bootstrap collect object-model string error integer float word vector map range execution".split()
directory = Path(sys.argv[1])
for module in modules:
    library = directory / ("libbirnam_" + module.replace("-", "_") + ".dylib")
    manifest = json.loads(Path(f"abi/rust-{module}-dylib-v1.json").read_text())
    actual = set(subprocess.check_output(["nm", "-gjU", library], text=True).splitlines())
    assert actual == {"_" + symbol for symbol in manifest["exported_symbols"]}, (module, actual)
dependencies = subprocess.check_output(["otool", "-L", directory / "libbirnam_id.dylib"], text=True).splitlines()[1:]
assert {line.strip().split()[0] for line in dependencies} <= {"@rpath/libbirnam_id.dylib", "/usr/lib/libSystem.B.dylib"}, dependencies
metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--offline", "--locked", "--no-deps", "--format-version", "1"], text=True))
packages = {p["name"]: p for p in metadata["packages"]}
for module in modules + ["siphon", "abi"]:
    dependencies = {d["name"] for d in packages["birnam-" + module]["dependencies"] if d["kind"] is None}
    assert dependencies == (set() if module == "abi" else {"birnam-abi"}), (module, dependencies)
print("native boundary audit: exports and dependencies passed")
PY_AUDIT
CARGO_PROFILE_DEV_PANIC=unwind cargo test --offline --locked -p birnam-siphon --test modules --test memory --test memory_activation --test object_model --test text_error --test numbers --test numbers_loading --test collections --test roots --test execution_methods -- --include-ignored
