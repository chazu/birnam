#!/usr/bin/env python3
"""Fail closed if Rust artifact roles cross the host/policy boundaries."""

from __future__ import annotations

import re
from pathlib import Path


REPO = Path(__file__).resolve().parent.parent
CODEC = REPO / "rust/birnam-artifact/src/lib.rs"
PROJECTION = REPO / "rust/birnam-artifact/src/module.rs"
HOST = REPO / "rust/birnam-host/src/lib.rs"
POLICY = REPO / "src/id/modules.birnam"


def fail(message: str) -> None:
    raise SystemExit(f"rust-artifact-authority: {message}")


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as exc:
        fail(f"cannot read {path.relative_to(REPO)}: {exc}")


def require(text: str, token: str, role: str) -> None:
    if token not in text:
        fail(f"{role} lost required marker {token!r}")


def main() -> None:
    codec = read(CODEC)
    projection = read(PROJECTION)
    host = read(HOST)
    policy = read(POLICY)

    # The codec is pointer-free and must not acquire runtime or activation
    # dependencies. These checks intentionally target source-level imports and
    # definitions, not explanatory comments in the module documentation.
    require(codec, "pub fn inspect", "descriptor codec")
    require(codec, "pub const DESCRIPTOR_MAGIC", "descriptor codec")
    if re.search(r"(?m)^\s*(?:pub\s+)?unsafe\s+extern", codec):
        fail("descriptor codec contains an FFI entrypoint")
    if re.search(r"(?m)^\s*(?:use|extern\s+crate)\s+birnam_abi", codec):
        fail("descriptor codec imports the IdApi ABI")

    # Only the dynamic artifact projection may export Artifact/ArtifactVt and
    # the frozen module_init entrypoint.
    for marker in (
        "const ARTIFACT_VT_NAME",
        "(api.exportObj)",
        "pub unsafe extern \"C\" fn module_init",
    ):
        require(projection, marker, "ArtifactVt projection")
    if len(re.findall(r"pub unsafe extern \"C\" fn module_init", projection)) != 1:
        fail("ArtifactVt projection does not have exactly one module_init")

    # The host may load higher-rung module_init symbols through the selected
    # floor, but it must not export an artifact projection or activate before
    # preflight completes.
    for marker in ("pub fn preflight_artifact", "pub unsafe fn activate_artifact", 'c"loadArtifact:"'):
        require(host, marker, "Rust host")
    if "ArtifactVt" in host:
        fail("Rust host names the ArtifactVt projection")
    if re.search(r"(?m)^\s*#\[unsafe\(no_mangle\)\].*\n\s*pub unsafe extern \"C\" fn module_init", host):
        fail("Rust host exports module_init")
    if host.index("preflight_artifact(path") > host.index('c"loadArtifact:"'):
        fail("Rust host sends loadArtifact: before preflight")

    # Runtime activation/publication stays in the native Birnam Module policy.
    require(policy, "defineMethod: #loadArtifact: on: Module", "native Module policy")

    print(
        "rust-artifact-authority: codec=pointer-free projection=single-module-init "
        "host=preflight-before-activation policy=native-loadArtifact"
    )


if __name__ == "__main__":
    main()
