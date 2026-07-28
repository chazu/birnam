#!/usr/bin/env python3
"""Emit a retained, fail-closed receipt for a production prefix."""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path


CANONICAL = (
    "libbirnam_id.dylib",
    "libbirnam_store.dylib",
    "libbirnam_id_artifact.dylib",
    "libbirnam_id_peg.dylib",
    "libbirnam_id_lang.dylib",
    "libbirnam_id_tcc.dylib",
    "libbirnam_id_codegen.dylib",
    "libbirnam_id_prelude.dylib",
    "libbirnam_id_collection.dylib",
    "libbirnam_id_error.dylib",
    "libbirnam_id_isolate.dylib",
    "libbirnam_id_relational.dylib",
    "libbirnam_id_alien.dylib",
    "libbirnam_id_image.dylib",
)


def fail(message: str) -> None:
    print(f"production-prefix: {message}", file=sys.stderr)
    raise SystemExit(1)


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def confined(path: Path, root: Path) -> str:
    try:
        resolved = path.resolve()
        resolved.relative_to(root)
    except ValueError:
        fail(f"resolved path escapes prefix: {path}")
    return str(resolved)


def rust_fixture_name(canonical: str) -> str:
    stem = canonical.removesuffix(".dylib")
    if stem == "libbirnam_id":
        return "libbirnam_id.dylib"
    if stem == "libbirnam_id_codegen":
        return "libbirnam_codegen_runtime.dylib"
    if stem.startswith("libbirnam_id_"):
        return "libbirnam_" + stem.removeprefix("libbirnam_id_") + ".dylib"
    return stem + ".dylib"


def fixture_root(raw: str, label: str) -> Path:
    root = Path(raw).resolve()
    if not root.is_dir():
        fail(f"missing {label} fixture root: {root}")
    return root


def main() -> None:
    if len(sys.argv) != 4:
        fail(
            "usage: check-production-prefix.py <prefix> "
            "<rust-fixture-root> <zig-fixture-root>"
        )
    prefix = Path(sys.argv[1]).resolve()
    if not prefix.is_dir():
        fail(f"missing prefix: {prefix}")
    rust_root = fixture_root(sys.argv[2], "Rust")
    zig_root = fixture_root(sys.argv[3], "Zig")
    if (prefix / "bin/birnam-zig-oracle").exists():
        fail("production prefix contains birnam-zig-oracle")
    zig_fixtures = sorted((prefix / "lib").glob("*_zig.dylib"))
    if zig_fixtures:
        fail(f"production prefix contains Zig fixtures: {[p.name for p in zig_fixtures]}")

    host = prefix / "bin/birnam"
    if not host.is_file():
        fail("production prefix is missing bin/birnam")
    host_sha = digest(host)
    print(
        json.dumps(
            {
                "kind": "host",
                "path": "bin/birnam",
                "resolved_path": confined(host, prefix),
                "observed_identity": f"sha256:{host_sha}",
                "sha256": host_sha,
            },
            sort_keys=True,
            separators=(",", ":"),
        )
    )

    for canonical in CANONICAL:
        path = prefix / "lib" / canonical
        if not path.is_file():
            fail(f"missing canonical provider: {canonical}")
        rust_name = rust_fixture_name(canonical)
        rust_path = rust_root / rust_name
        zig_name = canonical.removesuffix(".dylib") + "_zig.dylib"
        zig_path = zig_root / zig_name
        if not rust_path.is_file():
            fail(f"missing Rust fixture for {canonical}: {rust_name}")
        if not zig_path.is_file():
            fail(f"missing Zig fixture for {canonical}: {zig_name}")
        value = digest(path)
        rust_value = digest(rust_path)
        zig_value = digest(zig_path)
        if value != rust_value:
            fail(f"canonical provider differs from Rust fixture: {canonical}")
        if value == zig_value:
            fail(f"canonical provider equals Zig fixture: {canonical}")
        print(
            json.dumps(
                {
                    "canonical": canonical,
                    "path": f"lib/{canonical}",
                    "resolved_path": confined(path, prefix),
                    "provider": "rust-canonical",
                    "rust_fixture": rust_name,
                    "rust_fixture_resolved_path": confined(rust_path, rust_root),
                    "rust_fixture_sha256": rust_value,
                    "zig_fixture": zig_name,
                    "zig_fixture_resolved_path": confined(zig_path, zig_root),
                    "zig_fixture_sha256": zig_value,
                    "observed_identity": f"sha256:{value}",
                    "sha256": value,
                },
                sort_keys=True,
                separators=(",", ":"),
            )
        )
    print(f"production-prefix: ok host=sha256:{host_sha} dynamic={len(CANONICAL)}")


if __name__ == "__main__":
    main()
