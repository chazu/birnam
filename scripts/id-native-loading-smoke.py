#!/usr/bin/env python3
"""Exercise Siphon, Id and upper adoption using an actual debug installation."""
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parent.parent
prefix = Path(sys.argv[1]).resolve()


def run(*args, **kwargs):
    return subprocess.run(args, cwd=root, check=True, **kwargs)


# The retired loader no longer exists; a counter on that path would be vacuous.
for path in ("rust/birnam-host/src/startup.rs", "rust/birnam-native-loader/src/lib.rs"):
    source = (root / path).read_text()
    assert "ModuleInitFn" not in source and 'c"module_init"' not in source, path

# No installed native or stage-1 image may retain the aggregate initializer or
# dependency setters. Check binaries, including modules outside the exact-export
# inventories below, so obsolete entry points cannot return unnoticed.
for library in sorted((prefix / "lib").glob("libbirnam_*.dylib")):
    exports = run("nm", "-gjU", str(library), capture_output=True, text=True).stdout.splitlines()
    forbidden = [symbol for symbol in exports
                 if symbol == "_module_init" or "_dependency_v" in symbol
                 or re.search(r"_(?:prepare|provider|factory)_v[0-9]+$", symbol)]
    assert not forbidden, (library.name, forbidden)
print("all installed libraries reject retired initialization and provider exports")

# Check the installed images, not just source-level module visibility.
for stem in ("id", "bootstrap", "runtime", "method-observation", "storage", "dispatch-cache",
             "block-loop", "collection-value", "error-value", "map", "printing", "range",
             "string-builder", "string-collection", "vector", "codegen", "text-view", "system-gc",
             "system-output", "system-input", "environment", "file-io", "native-loader", "source-loader"):
    library = prefix / ("lib/libbirnam_" + ("id_codegen" if stem == "codegen" else stem.replace("-", "_")) + ".dylib")
    manifest = json.loads((root / f"abi/rust-{stem}-dylib-v1.json").read_text())
    exports = set(run("nm", "-gjU", str(library), capture_output=True, text=True).stdout.splitlines())
    assert exports == {"_" + symbol for symbol in manifest["exported_symbols"]}, (stem, exports)
    dependencies = run("otool", "-L", str(library), capture_output=True, text=True).stdout.splitlines()[1:]
    if stem == "id":
        assert {line.strip().split()[0] for line in dependencies} <= {
            "@rpath/libbirnam_id.dylib", "/usr/lib/libSystem.B.dylib"}, dependencies
print("installed Id has only its native entry and identity exports; Runtime exports are separate")

metadata = json.loads(run("mise", "exec", "--", "cargo", "metadata", "--locked", "--offline",
                          "--no-deps", "--format-version", "1", capture_output=True).stdout)
packages = {package["name"]: package for package in metadata["packages"]}
def normal_dependencies(name):
    return {dependency["name"] for dependency in packages[name]["dependencies"]
            if dependency["kind"] is None}
assert normal_dependencies("birnam-id") == {"birnam-abi"}
assert normal_dependencies("birnam-bootstrap") == {"birnam-abi"}
assert not normal_dependencies("birnam-abi")
assert "birnam-id" not in normal_dependencies("birnam-runtime")
print("Cargo boundary: Id depends only on ABI; Runtime has no Rust dependency on Id")
target = Path(metadata["target_directory"]) / "debug"
run("mise", "exec", "--", "cargo", "build", "--locked", "--offline", "-p", "birnam-host", "-p", "birnam-abi")
with tempfile.TemporaryDirectory(prefix="birnam-id-native-") as directory:
    scratch = Path(directory)
    isolated = scratch / "isolated"
    isolated.mkdir()
    shutil.copy2(prefix / "lib/libbirnam_id.dylib", isolated)
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/runtime-construction-native.c", "-o", str(scratch / "runtime-construction.dylib"))
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/runtime-support-native.c", "-o", str(scratch / "runtime-support.dylib"))
    support_driver = scratch / "support-driver"
    run("mise", "exec", "--", "rustc", "--edition=2024", "-Cpanic=abort",
        "scripts/fixtures/runtime-support-host.rs", "--extern", f"birnam_host={target}/libbirnam_host.rlib",
        "--extern", f"birnam_abi={target}/libbirnam_abi.rlib", "-L", f"dependency={target}/deps", "-o", str(support_driver))
    support_prefix = scratch / "support-prefix"
    support_prefix.mkdir()
    for name in ["id", "bootstrap", "method_observation", "storage", "dispatch_cache"]:
        library = f"libbirnam_{name}.dylib"
        shutil.copy2(prefix / "lib" / library, support_prefix / library)
    run(str(support_driver), str(support_prefix), str(scratch / "runtime-support.dylib"))
    for mode in range(6):
        run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib", f"-DMODE={mode}",
            "scripts/fixtures/id-native-module.c", "-o", str(scratch / f"{mode}.dylib"))
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/early-root-native.c", "-o", str(scratch / "early-root.dylib"))
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/early-lifetime-native.c", "-o", str(scratch / "early-lifetime.dylib"))
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/base-values-native.c", "-o", str(scratch / "base-values.dylib"))
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/early-namespace-native.c", "-o", str(scratch / "early-namespace.dylib"))
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/early-object-model-native.c", "-o", str(scratch / "early-object-model.dylib"))
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/early-string-native.c", "-o", str(scratch / "early-string.dylib"))
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/early-error-native.c", "-o", str(scratch / "early-error.dylib"))
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/early-integer-native.c", "-o", str(scratch / "early-integer.dylib"))
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/early-float-native.c", "-o", str(scratch / "early-float.dylib"))
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/early-word-native.c", "-o", str(scratch / "early-word.dylib"))
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/early-text-native.c", "-o", str(scratch / "early-text.dylib"))
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/early-trace-native.c", "-o", str(scratch / "early-trace.dylib"))
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/early-collection-native.c", "-o", str(scratch / "early-collection.dylib"))
    for mode in range(2):
        run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib", f"-DMODE={mode}",
            "scripts/fixtures/bootstrap-result-native.c", "-o", str(scratch / f"bootstrap-result-{mode}.dylib"))
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/early-collector-native.c", "-o", str(scratch / "early-collector.dylib"))
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/early-activation-native.c", "-o", str(scratch / "early-activation.dylib"))
    driver = scratch / "driver"
    run("mise", "exec", "--", "rustc", "--edition=2024", "-Cpanic=abort",
        "scripts/fixtures/id-native-host.rs", "--extern", f"birnam_host={target}/libbirnam_host.rlib",
        "--extern", f"birnam_abi={target}/libbirnam_abi.rlib", "-L", f"dependency={target}/deps", "-o", str(driver))
    graph_driver = scratch / "graph-driver"
    run("mise", "exec", "--", "rustc", "--edition=2024", "-Cpanic=abort",
        "scripts/fixtures/trace-graph-host.rs", "--extern", f"birnam_host={target}/libbirnam_host.rlib",
        "--extern", f"birnam_abi={target}/libbirnam_abi.rlib", "-L", f"dependency={target}/deps", "-o", str(graph_driver))
    graph_prefix = scratch / "graph-prefix"
    graph_prefix.mkdir()
    for module in ["id", "bootstrap", "root_registry", "payload_layout", "trace_hooks", "finalizers",
                   "lifetime", "boolean", "nil", "namespace", "object_model", "string", "id_error",
                   "integer", "float", "word", "native_mark", "trace"]:
        file = f"libbirnam_{module}.dylib"
        shutil.copy2(prefix / "lib" / file, graph_prefix / file)
    run(str(graph_driver), str(graph_prefix), str(scratch / "early-trace.dylib"))
    collector_driver = scratch / "collector-driver"
    run("mise", "exec", "--", "rustc", "--edition=2024", "-Cpanic=abort",
        "scripts/fixtures/collector-host.rs", "--extern", f"birnam_host={target}/libbirnam_host.rlib",
        "--extern", f"birnam_abi={target}/libbirnam_abi.rlib", "-L", f"dependency={target}/deps", "-o", str(collector_driver))
    for module in ["native_stack", "gc_settings", "allocation_budget", "collect"]:
        file = f"libbirnam_{module}.dylib"
        shutil.copy2(prefix / "lib" / file, graph_prefix / file)
    run(str(collector_driver), str(graph_prefix), str(scratch / "early-collector.dylib"))
    run(str(collector_driver), str(graph_prefix), str(scratch / "early-collector.dylib"), "trace-failure")
    activation_driver = scratch / "activation-driver"
    run("mise", "exec", "--", "rustc", "--edition=2024", "-Cpanic=abort",
        "scripts/fixtures/activation-host.rs", "--extern", f"birnam_host={target}/libbirnam_host.rlib",
        "--extern", f"birnam_abi={target}/libbirnam_abi.rlib", "-L", f"dependency={target}/deps", "-o", str(activation_driver))
    run(str(activation_driver), str(graph_prefix), str(scratch / "early-activation.dylib"))
    run(str(activation_driver), str(graph_prefix), str(scratch / "early-activation.dylib"), "missing-layout")
    run(str(activation_driver), str(prefix / "lib"), str(scratch / "early-activation.dylib"), "adopt")
    values_prefix = scratch / "values-prefix"
    shutil.copytree(graph_prefix, values_prefix)
    for module in ["runtime", "method_observation", "storage", "dispatch_cache", "error_value",
                   "printing", "block_loop", "string_builder", "collection_value", "vector", "range", "map",
                   "string_collection", "id_collection", "store"]:
        file = f"libbirnam_{module}.dylib"
        shutil.copy2(prefix / "lib" / file, values_prefix / file)
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/execution-values-native.c", "-o", str(scratch / "execution-values.dylib"))
    for mode in range(5):
        run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib", f"-DMODE={mode}",
            "scripts/fixtures/system-native-module.c", "-o", str(scratch / f"system-native-{mode}.dylib"))
    values_driver = scratch / "values-driver"
    run("mise", "exec", "--", "rustc", "--edition=2024", "-Cpanic=abort",
        "scripts/fixtures/execution-values-host.rs", "--extern", f"birnam_host={target}/libbirnam_host.rlib",
        "--extern", f"birnam_abi={target}/libbirnam_abi.rlib", "-L", f"dependency={target}/deps", "-o", str(values_driver))
    run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib",
        "scripts/fixtures/block-loop-reentry-native.c", "-o", str(scratch / "block-loop-reentry.dylib"))
    for mode in [1, 2]:
        shutil.copy2(prefix / "lib/libbirnam_block_loop.dylib", scratch / f"block-loop-{mode}.dylib")
    parser_prefix = scratch / "parser-prefix"
    shutil.copytree(values_prefix, parser_prefix)
    compiler_prefix = scratch / "compiler-prefix"
    shutil.copytree(values_prefix, compiler_prefix)
    system_prefix = scratch / "system-prefix"
    shutil.copytree(values_prefix, system_prefix)
    run(str(values_driver), str(system_prefix), str(scratch / "execution-values.dylib"), str(scratch), str(prefix / "lib"), "system")
    run(str(values_driver), str(values_prefix), str(scratch / "execution-values.dylib"), str(scratch))
    run(str(values_driver), str(parser_prefix), str(scratch / "execution-values.dylib"), str(scratch), str(prefix / "lib"))
    run(str(values_driver), str(compiler_prefix), str(scratch / "execution-values.dylib"), str(scratch), str(prefix / "lib"), "compiler")
    env = dict(os.environ, BIRNAM_NATIVE_UNLOAD_LOG=str(scratch / "unloaded"))
    run(str(driver), str(prefix), str(scratch), env=env)
    # Entered images only unload at process exit; rejected ones unload immediately.
    assert sorted((scratch / "unloaded").read_text().splitlines()) == list("012345")
print("id-native-loading-smoke: passed")
