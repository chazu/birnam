#!/usr/bin/env python3
"""Fail-closed Design 66 Z0.1 residue and runtime-boundary checker."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path


REPO = Path(__file__).resolve().parent.parent
MANIFEST = REPO / "abi/remaining-zig-v1.json"
KINDS = {
    "canonical-production",
    "zig-oracle",
    "build-only",
    "shared-contract",
    "test-evidence",
    "native-policy-bridge",
}
DYNAMIC_NAME = re.compile(r"^libbirnam_[a-z0-9_]+\.dylib$")
STEP_NAME = re.compile(r'\.step\(\s*"([^"]+)"')


def fail(message: str) -> None:
    print(f"remaining-zig-inventory: {message}", file=sys.stderr)
    raise SystemExit(1)


def listed(pattern: str, *extra: str) -> list[str]:
    command = ["rg", "--files", *extra, "-g", pattern]
    result = subprocess.run(command, cwd=REPO, check=True, text=True, capture_output=True)
    return sorted(line for line in result.stdout.splitlines() if line)


def main() -> None:
    try:
        manifest = json.loads(MANIFEST.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot read manifest: {exc}")
    if manifest.get("schema_version") != 1:
        fail("unsupported schema version")

    files = manifest.get("files")
    if not isinstance(files, list):
        fail("files must be an array")
    paths = [item.get("path") for item in files if isinstance(item, dict)]
    if len(paths) != len(files) or any(not isinstance(path, str) for path in paths):
        fail("every file entry must be an object with a string path")
    if len(paths) != len(set(paths)):
        fail("a Zig file is classified more than once")
    for item in files:
        if item.get("kind") not in KINDS:
            fail(f"unsupported or missing kind for {item.get('path')!r}")
        for field in ("owner", "disposition"):
            if not isinstance(item.get(field), str) or not item[field].strip():
                fail(f"{item.get('path')!r} has no {field}")
        path = item["path"]
        if Path(path).is_absolute() or ".." in Path(path).parts:
            fail(f"unsafe manifest path: {path}")
        if not (REPO / path).is_file():
            fail(f"classified Zig file does not exist: {path}")

    actual_zig = listed("*.zig")
    if set(paths) != set(actual_zig):
        fail(
            "repo-wide Zig graph differs; "
            f"missing={sorted(set(actual_zig) - set(paths))}, "
            f"stale={sorted(set(paths) - set(actual_zig))}"
        )

    graph = manifest.get("graph")
    if not isinstance(graph, dict):
        fail("graph is missing")
    for key, pattern, args in (
        ("birnam_sources", "*.birnam", ()),
        ("rust_crates", "Cargo.toml", ()),
        ("test_scripts", "*.sh", ("scripts",)),
    ):
        expected = listed(pattern, *args)
        if key == "test_scripts":
            expected = sorted(set(expected) | set(listed("*.py", "scripts")))
        recorded = graph.get(key)
        if recorded != expected:
            fail(f"{key} graph drifted; expected={expected}, recorded={recorded}")
    if not graph.get("build_targets") or not isinstance(graph.get("build_targets"), list):
        fail("build_targets must be a non-empty array")
    build_text = (REPO / "build.zig").read_text()
    actual_targets = sorted(set(STEP_NAME.findall(build_text)))
    if graph["build_targets"] != actual_targets:
        fail(
            "build target graph drifted; "
            f"missing={sorted(set(actual_targets) - set(graph['build_targets']))}, "
            f"stale={sorted(set(graph['build_targets']) - set(actual_targets))}"
        )
    destinations = graph.get("install_destinations")
    if not isinstance(destinations, list) or "bin/birnam" not in destinations:
        fail("install_destinations must name the canonical executable")
    if not any(isinstance(item, str) and item.startswith("lib/libbirnam_") for item in destinations):
        fail("install_destinations must name the dynamic library family")

    dependencies = manifest.get("dynamic_dependencies")
    if not isinstance(dependencies, list) or not dependencies:
        fail("dynamic_dependencies must be a non-empty array")
    names = []
    for dependency in dependencies:
        if not isinstance(dependency, dict):
            fail("dynamic dependency entries must be objects")
        name = dependency.get("name")
        if not isinstance(name, str) or not DYNAMIC_NAME.fullmatch(name):
            fail(f"not a declared dynamic library name: {name!r}")
        if name in names:
            fail(f"dynamic dependency appears more than once: {name}")
        if not dependency.get("owner") or not dependency.get("role"):
            fail(f"dynamic dependency has no owner/role: {name}")
        names.append(name)

    main_text = (REPO / "src/id/main.zig").read_text()
    match = re.search(r"const script_modules = \[_\]\[\]const u8\{(.*?)\n\};", main_text, re.DOTALL)
    if not match:
        fail("could not locate canonical script_modules load order")
    expected_dynamic = {"libbirnam_id.dylib"}
    expected_dynamic.update(f"{module}.dylib" for module in re.findall(r'"(libbirnam_[^"]+)"', match.group(1)))
    if set(names) != expected_dynamic:
        fail(f"declared dynamic names differ from runtime load order; expected={sorted(expected_dynamic)}, declared={sorted(names)}")

    print(
        "remaining-zig-inventory: ok "
        f"zig_files={len(paths)} birnam_sources={len(graph['birnam_sources'])} "
        f"rust_crates={len(graph['rust_crates'])} test_scripts={len(graph['test_scripts'])} "
        f"build_targets={len(graph['build_targets'])} dynamic_dependencies={len(names)}"
    )


if __name__ == "__main__":
    main()
