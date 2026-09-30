#!/usr/bin/env python3
"""Derive reviewed single-fault stackless fixtures from frozen valid inputs.

Usage: derive-malformed-fixtures.py INPUT_DIRECTORY OUTPUT_DIRECTORY

This is deliberately separate from build-fixtures.py: malformed bytes are not
outputs of the canonical serializers.
"""

from __future__ import annotations

import hashlib
import sys
from pathlib import Path


NAMED_MANIFEST_SHA256 = (
    "3b53236b12eee124188a4a2b64c3f1fb3f51d75ff15e042481e7344478d8c13d"
)
BLOCK_PORTABLE_ID_LE = bytes.fromhex("7398e2cfaf0270b5")
TRACE_IMAGE_SHA256 = (
    "a4c329ded2cda7ee712183370334516ca87e5466fc0ce0cc782df80b57becc7b"
)


def main() -> int:
    if len(sys.argv) != 3:
        print(
            "usage: derive-malformed-fixtures.py INPUT_DIRECTORY OUTPUT_DIRECTORY",
            file=sys.stderr,
        )
        return 2
    input_directory = Path(sys.argv[1])
    output_directory = Path(sys.argv[2])
    source = (input_directory / "resumable-manifest-named-only.bin").read_bytes()
    if hashlib.sha256(source).hexdigest() != NAMED_MANIFEST_SHA256:
        raise ValueError("named-only manifest source hash drifted")
    if source[0x110:0x118] == BLOCK_PORTABLE_ID_LE:
        raise ValueError("source already contains forged portable identity")

    forged = bytearray(source)
    forged[0x110:0x118] = BLOCK_PORTABLE_ID_LE
    output_directory.mkdir(parents=True, exist_ok=True)
    name = "resumable-manifest-forged-portable-id.bin"
    data = bytes(forged)
    (output_directory / name).write_bytes(data)
    print(f"{hashlib.sha256(data).hexdigest()}  {name}")

    image = (input_directory / "image-v5-construction-trace.bin").read_bytes()
    if hashlib.sha256(image).hexdigest() != TRACE_IMAGE_SHA256:
        raise ValueError("construction-trace image source hash drifted")
    image_mutations = (
        ("image-v5-out-of-range-index.bin", 0x29D, 2),
        ("image-v5-named-row-index.bin", 0x29D, 0),
        ("image-v5-authority-mismatch.bin", 0x74, ord("6")),
        ("image-v5-manifest-hash-mismatch.bin", 0xB4, image[0xB4] ^ 1),
    )
    for image_name, offset, value in image_mutations:
        malformed = bytearray(image)
        malformed[offset] = value
        image_data = bytes(malformed)
        (output_directory / image_name).write_bytes(image_data)
        print(f"{hashlib.sha256(image_data).hexdigest()}  {image_name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
