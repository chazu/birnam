#!/usr/bin/env python3
"""Exercise Siphon, Id and upper adoption using an actual debug installation."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parent.parent
prefix = Path(sys.argv[1]).resolve()


def run(*args, **kwargs):
    return subprocess.run(args, cwd=root, check=True, **kwargs)


metadata = json.loads(run("mise", "exec", "--", "cargo", "metadata", "--locked", "--offline",
                          "--no-deps", "--format-version", "1", capture_output=True).stdout)
target = Path(metadata["target_directory"]) / "debug"
run("mise", "exec", "--", "cargo", "build", "--locked", "--offline", "-p", "birnam-host", "-p", "birnam-abi")
with tempfile.TemporaryDirectory(prefix="birnam-id-native-") as directory:
    scratch = Path(directory)
    isolated = scratch / "isolated"
    isolated.mkdir()
    shutil.copy2(prefix / "lib/libbirnam_id.dylib", isolated)
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
    for mode in range(2):
        run("cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-dynamiclib", f"-DMODE={mode}",
            "scripts/fixtures/bootstrap-result-native.c", "-o", str(scratch / f"bootstrap-result-{mode}.dylib"))
    driver = scratch / "driver"
    run("mise", "exec", "--", "rustc", "--edition=2024", "-Cpanic=abort",
        "scripts/fixtures/id-native-host.rs", "--extern", f"birnam_host={target}/libbirnam_host.rlib",
        "--extern", f"birnam_abi={target}/libbirnam_abi.rlib", "-L", f"dependency={target}/deps", "-o", str(driver))
    env = dict(os.environ, BIRNAM_NATIVE_UNLOAD_LOG=str(scratch / "unloaded"))
    run(str(driver), str(prefix), str(scratch), env=env)
    # Entered images only unload at process exit; rejected ones unload immediately.
    assert sorted((scratch / "unloaded").read_text().splitlines()) == list("012345")
print("id-native-loading-smoke: passed")
