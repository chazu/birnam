#!/usr/bin/env python3
"""Verify Design 62's committed native-rung migration inventory."""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path


REPO = Path(__file__).resolve().parent.parent
MANIFEST_PATH = REPO / "abi/native-rungs-v1.json"
SURFACE_MARKERS = (
    ".method(",
    ".exportObj(",
    "@export(",
    "pub export fn ",
    ".setLayout(",
    ".setTraceHook(",
    ".gcRegisterRootProvider(",
    ".setFinalizer(",
    '.import("',
)


def fail(message: str) -> None:
    print(f"native-rung-inventory: {message}", file=sys.stderr)
    raise SystemExit(1)


def exported_symbols(text: str) -> list[str]:
    symbols = set(re.findall(r"pub\s+export\s+fn\s+([A-Za-z0-9_]+)", text))
    symbols.update(
        re.findall(r'@export\(.*?\.name\s*=\s*"([^"]+)"', text, re.DOTALL)
    )
    return sorted(symbols)


def surface(text: str) -> tuple[int, str]:
    lines = [
        line.strip()
        for line in text.splitlines()
        if any(marker in line for marker in SURFACE_MARKERS)
    ]
    payload = ("\n".join(lines) + "\n").encode()
    return len(lines), hashlib.sha256(payload).hexdigest()


def main() -> None:
    manifest = json.loads(MANIFEST_PATH.read_text())
    if manifest.get("schema_version") != 1:
        fail("unsupported schema version")

    assignments = manifest["source_assignments"]
    assigned_paths = [item["path"] for item in assignments]
    if len(assigned_paths) != len(set(assigned_paths)):
        fail("source assignment appears more than once")

    expected_paths = {
        str(path.relative_to(REPO))
        for path in (REPO / "src/id").glob("*.zig")
        if not path.name.endswith("_test.zig")
    }
    expected_paths.add("build.zig")
    assigned_set = set(assigned_paths)
    missing = sorted(expected_paths - assigned_set)
    stale = sorted(assigned_set - expected_paths)
    if missing or stale:
        fail(f"source assignments differ; missing={missing}, stale={stale}")

    for path in assigned_paths:
        if not (REPO / path).is_file():
            fail(f"assigned source does not exist: {path}")

    rungs = manifest["rungs"]
    rung_ids = [rung["id"] for rung in rungs]
    if len(rung_ids) != len(set(rung_ids)):
        fail("rung id appears more than once")
    rung_by_id = {rung["id"]: rung for rung in rungs}

    main_text = (REPO / "src/id/main.zig").read_text()
    match = re.search(
        r"const script_modules = \[_\]\[\]const u8\{(.*?)\n\};",
        main_text,
        re.DOTALL,
    )
    if not match:
        fail("could not locate script_modules load order")
    actual_order = re.findall(r'"(libbirnam_[^"]+)"', match.group(1))
    if actual_order != manifest["load_order"]:
        fail(
            "runtime load order drifted; "
            f"expected={manifest['load_order']}, actual={actual_order}"
        )
    if [rung["dylib"] for rung in rungs] != manifest["load_order"]:
        fail("rung entries are not in the frozen runtime load order")

    assignment_by_path = {item["path"]: item for item in assignments}
    actual_external_owners: dict[str, str] = {}
    for rung in rungs:
        source = rung["source"]
        assignment = assignment_by_path.get(source)
        if assignment is None or assignment["kind"] != "native-rung":
            fail(f"{source} is not assigned as a native rung")
        if assignment["owner"] != rung["id"]:
            fail(f"{source} owner does not match rung id {rung['id']}")

        text = (REPO / source).read_text()
        actual_symbols = exported_symbols(text)
        if actual_symbols != rung["symbols"]:
            fail(
                f"{rung['id']} exported symbols drifted; "
                f"expected={rung['symbols']}, actual={actual_symbols}"
            )
        for symbol in actual_symbols:
            if symbol == "module_init":
                continue
            if symbol in actual_external_owners:
                fail(
                    f"{symbol} has two owners: "
                    f"{actual_external_owners[symbol]} and {rung['id']}"
                )
            actual_external_owners[symbol] = rung["id"]

        count, digest = surface(text)
        if count != rung["surface_lines"] or digest != rung["surface_sha256"]:
            fail(
                f"{rung['id']} public/native surface drifted; "
                f"expected lines={rung['surface_lines']} sha256={rung['surface_sha256']}, "
                f"actual lines={count} sha256={digest}"
            )

        for name in rung["globals"]:
            if f'exportObj("{name}",' not in text:
                fail(f"{rung['id']} no longer exports recorded global {name}")

    if actual_external_owners != manifest["external_symbol_owners"]:
        fail(
            "external symbol ownership drifted; "
            f"expected={manifest['external_symbol_owners']}, "
            f"actual={actual_external_owners}"
        )

    image_text = (REPO / "src/id/image.zig").read_text()
    if "pub const FORMAT_VERSION: u32 = 4;" not in image_text:
        fail("image implementation is no longer the frozen v4 contract")

    store_text = (REPO / "src/id/store.zig").read_text()
    codegen_text = (REPO / "src/id/codegen.zig").read_text()
    callback_needles = (
        (store_text, "const MethodMutationAuthorizer = *const fn () callconv(.c) Oop;"),
        (store_text, '"setMethodMutationObserver:"'),
        (store_text, '"setMethodMutationAuthorizer:"'),
        (codegen_text, '"setMethodMutationObserver:"'),
        (codegen_text, '"setMethodMutationAuthorizer:"'),
        (codegen_text, "@intFromPtr(&authorizeManagedMethodMutation)"),
    )
    for text, needle in callback_needles:
        if needle not in text:
            fail(f"private Store-Codegen callback contract drifted: {needle}")

    print(
        "native-rung-inventory: ok "
        f"sources={len(assignments)} rungs={len(rungs)} "
        f"external_symbols={len(actual_external_owners)}"
    )


if __name__ == "__main__":
    main()
