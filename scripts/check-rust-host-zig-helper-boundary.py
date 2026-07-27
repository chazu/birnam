#!/usr/bin/env python3
"""Fail closed if the Rust host starts depending on Zig helper modules."""

from __future__ import annotations

from pathlib import Path


REPO = Path(__file__).resolve().parent.parent
HOST = REPO / "rust/birnam-host/src"
FORBIDDEN = (
    "project.zig",
    "project_build.zig",
    "project_check.zig",
    "runtime_resources.zig",
)
REQUIRED = (
    "pub fn resolve()",
    "BIRNAM_LIB_DIR",
    "BIRNAM_ID_DYLIB",
)


def fail(message: str) -> None:
    raise SystemExit(f"rust-host-zig-helper-boundary: {message}")


def main() -> None:
    sources = sorted(HOST.rglob("*.rs"))
    if not sources:
        fail("Rust host source set is empty")

    contents = {path: path.read_text(encoding="utf-8") for path in sources}
    for path, text in contents.items():
        for token in FORBIDDEN:
            if token in text:
                fail(f"{path.relative_to(REPO)} mentions forbidden Zig helper {token}")

    lib = contents.get(HOST / "lib.rs", "")
    for token in REQUIRED:
        if token not in lib:
            fail(f"Rust host lost explicit resource boundary marker {token}")

    # The Rust host may dlopen runtime artifacts through the selected floor,
    # but it must not grow a path-based escape hatch to the Zig helper layer.
    if "dlopen" in "\n".join(contents.values()) and any(
        token in "\n".join(contents.values()) for token in FORBIDDEN
    ):
        fail("a host dlopen path is coupled to a forbidden Zig helper")

    print(
        "rust-host-zig-helper-boundary: "
        f"sources={len(sources)} static=ok resource-boundary=ok"
    )


if __name__ == "__main__":
    main()
