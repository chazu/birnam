#!/usr/bin/env python3
"""Fail closed when a rollback prefix is not the declared full Zig vector."""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path


REPO = Path(__file__).resolve().parent.parent
MANIFEST = REPO / "abi/rollback-prefix-v1.json"


def fail(message: str) -> None:
    print(f"rollback-prefix: {message}", file=sys.stderr)
    raise SystemExit(1)


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def safe_relative(path: str, roots: tuple[str, ...]) -> None:
    candidate = Path(path)
    if candidate.is_absolute() or ".." in candidate.parts:
        fail(f"unsafe path: {path}")
    if not any(
        path.startswith(root.removeprefix("<prefix>/").removesuffix("/") + "/")
        for root in roots
    ):
        fail(f"path is outside declared install roots: {path}")


def main() -> None:
    if len(sys.argv) != 2:
        fail("usage: check-rollback-prefix.py <prefix>")
    prefix = Path(sys.argv[1]).resolve()
    if not prefix.is_dir():
        fail(f"missing prefix: {prefix}")
    manifest = json.loads(MANIFEST.read_text())
    if manifest.get("schema_version") != 1 or manifest.get("distribution") != "rollback":
        fail("unsupported manifest")
    roots = tuple(manifest.get("allowed_install_roots", ()))
    if roots != ("<prefix>/bin", "<prefix>/lib"):
        fail("install roots drifted")

    host = manifest.get("canonical_host")
    if not isinstance(host, dict) or not host.get("identity"):
        fail("canonical host receipt is incomplete")
    host_path = host.get("path")
    host_fixture = host.get("fixture")
    if not isinstance(host_path, str) or not isinstance(host_fixture, str):
        fail("canonical host paths are incomplete")
    safe_relative(host_path, roots)
    safe_relative(host_fixture, roots)
    host_file = prefix / host_path
    host_oracle = prefix / host_fixture
    if not host_file.is_file() or not host_oracle.is_file():
        fail("canonical host or explicit oracle is missing")
    if digest(host_file) != digest(host_oracle):
        fail("canonical host is not the explicit Zig oracle")

    order = manifest.get("dynamic_load_order")
    vector = manifest.get("vector")
    if not isinstance(order, list) or not isinstance(vector, list):
        fail("dynamic vector is incomplete")
    if order != [entry.get("canonical") for entry in vector]:
        fail("dynamic load order does not match vector")
    if len(vector) != 14 or len(set(order)) != len(order):
        fail("rollback vector must contain fourteen unique canonical names")

    declared_fixtures: set[str] = set()
    receipts: list[dict[str, str]] = []
    for entry in vector:
        canonical = entry.get("canonical")
        fixture = entry.get("fixture")
        identity = entry.get("identity")
        if not all(isinstance(value, str) and value for value in (canonical, fixture, identity)):
            fail("vector entry is incomplete")
        safe_relative(f"lib/{canonical}", roots)
        safe_relative(f"lib/{fixture}", roots)
        declared_fixtures.add(fixture)
        canonical_path = prefix / "lib" / canonical
        fixture_path = prefix / "lib" / fixture
        if not canonical_path.is_file() or not fixture_path.is_file():
            fail(f"missing canonical or Zig fixture: {canonical}")
        canonical_sha256 = digest(canonical_path)
        fixture_sha256 = digest(fixture_path)
        if canonical_sha256 != fixture_sha256:
            fail(f"canonical provider is not its Zig fixture: {canonical}")
        receipts.append(
            {
                "canonical": canonical,
                "fixture": fixture,
                "identity": identity,
                "canonical_sha256": canonical_sha256,
                "fixture_sha256": fixture_sha256,
            }
        )

    actual_fixtures = {path.name for path in (prefix / "lib").glob("*_zig.dylib")}
    if actual_fixtures != declared_fixtures:
        fail(
            "undeclared or missing Zig fixtures: "
            f"actual={sorted(actual_fixtures)} declared={sorted(declared_fixtures)}"
        )

    for receipt in receipts:
        print(json.dumps(receipt, sort_keys=True, separators=(",", ":")))
    print(
        f"rollback-prefix: ok host={host['identity']} "
        f"dynamic={len(receipts)} vector=full-zig"
    )


if __name__ == "__main__":
    main()
