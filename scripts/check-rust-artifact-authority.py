#!/usr/bin/env python3
"""Fail closed if Rust artifact roles cross the host/policy boundaries."""

from __future__ import annotations

import re
from pathlib import Path


REPO = Path(__file__).resolve().parent.parent
CODEC = REPO / "rust/birnam-artifact/src/lib.rs"
PROJECTION = REPO / "rust/birnam-artifact/src/module.rs"
ARTIFACT_CODEC = REPO / "rust/birnam-artifact-codec/src/lib.rs"
IMAGE_PROVIDER = REPO / "rust/birnam-image/src/lib.rs"
IMAGE_V8 = REPO / "rust/birnam-image/src/v8.rs"
ARTIFACT_CODEC_MANIFEST = REPO / "rust/birnam-artifact-codec/Cargo.toml"
CODEGEN_RUNTIME_MANIFEST = REPO / "rust/birnam-codegen-runtime/Cargo.toml"
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
    artifact_codec = read(ARTIFACT_CODEC)
    image_provider = read(IMAGE_PROVIDER)
    image_v8 = read(IMAGE_V8)
    artifact_codec_manifest = read(ARTIFACT_CODEC_MANIFEST)
    codegen_runtime_manifest = read(CODEGEN_RUNTIME_MANIFEST)
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

    # Artifact parsing remains provider-free because Codegen consumes it. The
    # exact-current image-v8 codec is owned solely by Image; Codegen must not
    # regain an image-format dependency or a second reader authority.
    require(artifact_codec, "pub use provider_source::*", "artifact codec")
    if re.search(r"(?m)^\s*pub\s+unsafe\s+extern", artifact_codec) or "module_init" in artifact_codec:
        fail("artifact codec contains a provider entrypoint")
    if "default = []" not in artifact_codec_manifest or 'crate-type = ["rlib"]' not in artifact_codec_manifest:
        fail("artifact codec Cargo surface is not provider-free")
    require(image_v8, "pub const FORMAT_VERSION: u32 = 8", "Image v8 codec")
    require(image_v8, "pub fn structural_preflight", "Image v8 codec")
    if "birnam-image" in codegen_runtime_manifest:
        fail("Codegen runtime depends on the Image provider")

    # Image's dynamic provider remains the owner of its initializer and
    # identity; the codec wrapper above must not become that owner.
    require(image_provider, "pub unsafe extern \"C\" fn module_init", "Image provider")
    require(image_provider, "birnam_image_rust_identity", "Image provider")

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
