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


def main() -> None:
    if len(sys.argv) != 2:
        fail("usage: check-production-prefix.py <prefix>")
    prefix = Path(sys.argv[1]).resolve()
    if not prefix.is_dir():
        fail(f"missing prefix: {prefix}")
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
        value = digest(path)
        print(
            json.dumps(
                {
                    "canonical": canonical,
                    "path": f"lib/{canonical}",
                    "resolved_path": confined(path, prefix),
                    "provider": "rust-canonical",
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
