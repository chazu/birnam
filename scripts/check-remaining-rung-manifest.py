#!/usr/bin/env python3
"""Fail closed on the Design 63 remaining-rung identity/install manifest."""

from __future__ import annotations

import json
import sys
from pathlib import Path


REPO = Path(__file__).resolve().parent.parent
MANIFEST = REPO / "abi/remaining-rungs-v1.json"
BASELINE = REPO / "abi/native-rungs-v1.json"
EXPECTED = ("codegen", "prelude", "collection", "error", "isolate", "relational", "alien", "image")


def fail(message: str) -> None:
    print(f"remaining-rung-manifest: {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    manifest = json.loads(MANIFEST.read_text())
    if manifest.get("schema_version") != 1:
        fail("unsupported schema version")
    if manifest.get("allowed_install_roots") != ["<prefix>/lib"]:
        fail("production install root is not isolated to <prefix>/lib")

    rungs = manifest.get("rungs")
    if not isinstance(rungs, list) or tuple(r["id"] for r in rungs) != EXPECTED:
        fail("remaining rung order or membership drifted")
    if manifest.get("load_order") != [r["canonical_dylib"] for r in rungs]:
        fail("load order does not match canonical dylib order")

    baseline = json.loads(BASELINE.read_text())
    baseline_by_id = {r["id"]: r for r in baseline["rungs"]}
    seen_paths: set[str] = set()
    for rung in rungs:
        rid = rung["id"]
        source = rung["source"]
        if source in seen_paths:
            fail(f"source appears twice: {source}")
        seen_paths.add(source)
        if not (REPO / source).is_file():
            fail(f"missing source: {source}")
        base = baseline_by_id.get(rid)
        if base is None or base["source"] != source:
            fail(f"source is not aligned with native-rungs-v1: {rid}")
        expected = f"{base['dylib']}.dylib"
        if rung["canonical_dylib"] != expected:
            fail(f"canonical dylib drifted for {rid}: {rung['canonical_dylib']} != {expected}")
        stem = expected[:-6]
        for key, suffix in (("zig_fixture", "_zig.dylib"), ("rust_fixture", "_rust.dylib")):
            name = rung[key]
            if name != stem + suffix:
                fail(f"{rid} {key} does not use an explicit implementation name")
        paths = rung.get("allowed_install_paths")
        expected_paths = [f"<prefix>/lib/{rung[k]}" for k in ("canonical_dylib", "zig_fixture", "rust_fixture")]
        if paths != expected_paths:
            fail(f"{rid} allowed install paths are not the three isolated artifacts")
        if any(".." in p or not p.startswith("<prefix>/lib/") for p in paths):
            fail(f"{rid} contains an unsafe install path")
        identities = rung.get("identity")
        if not isinstance(identities, dict) or not identities.get("zig") or not identities.get("rust"):
            fail(f"{rid} has no explicit diagnostic identities")
        if not rung.get("abi_digest_source") or not rung.get("dependency_identities"):
            fail(f"{rid} is missing digest or dependency identity metadata")

    print(f"remaining-rung-manifest: ok rungs={len(rungs)}")


if __name__ == "__main__":
    main()
