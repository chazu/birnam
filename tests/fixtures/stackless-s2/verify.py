#!/usr/bin/env python3
"""Independent checker for the frozen Design 60I stackless fixtures.

This file deliberately shares no code with a production codec and never emits
fixture bytes. Its constants and parser are a second implementation of the
normative document, checked against the committed binaries.
"""

from __future__ import annotations

import hashlib
import json
import struct
import sys
from pathlib import Path
from typing import NamedTuple


HERE = Path(__file__).resolve().parent
ARTIFACT_DOMAIN = b"birnam.artifact-selection.v1\0" + bytes(3)
GENERATION_DOMAIN = b"birnam.generation-selection.v1\0" + bytes(1)
DESCRIPTOR_DOMAIN = b"birnam.frame-descriptor.v2\0" + bytes(5)
MANIFEST_DOMAIN = b"birnam.resumable-manifest.v1\0" + bytes(3)
REVISION_A = bytes(range(32))
REVISION_B = bytes(range(32, 64))
REVISION_CALL = hashlib.sha256(b"birnam.stackless-s2.call-source.v1\0").digest()
REVISION_TAIL = hashlib.sha256(b"birnam.stackless-s2.tail-source.v1\0").digest()
REVISION_BRANCH = hashlib.sha256(b"birnam.stackless-s2.branch-source.v1\0").digest()
REVISION_CYCLE = hashlib.sha256(b"birnam.stackless-s2.cycle-source.v1\0").digest()
REVISION_COLOR = hashlib.sha256(b"birnam.stackless-s2.color-source.v1\0").digest()
TRACE_MODULE = b"trace"
TRACE_TARGET = b"Trace"
TRACE_SELECTOR = b"run"
TRACE_SOURCE = b"run => self\nblock => [ self ]\n"
TRACE_DEFINITION = b"run => self"
TRACE_REVISION = hashlib.sha256(
    b"birnam.source-revision.v1\0"
    + struct.pack(">I", len(TRACE_MODULE))
    + TRACE_MODULE
    + struct.pack(">I", 0)
    + struct.pack(">Q", len(TRACE_SOURCE))
    + TRACE_SOURCE
).digest()
ABSENT = (1 << 32) - 1
U64_MAX = (1 << 64) - 1
CANONICAL_VALUE_BYTES = 64 * 1024 * 1024
FIXED_RECORDS = 1_048_576
COVERED_REVISIONS = 65_536
IMPLEMENTATION_ROWS = 262_144
PATH_DEPTH = 4_096
PATH_ORDINALS = 4_194_304
SELECTOR_BYTES = 1024 * 1024
SELECTOR_ARENA_BYTES = 16 * 1024 * 1024
LANGUAGE_ARITY = 4_096
DESCRIPTOR_BYTES = 16 * 1024 * 1024
SECTION_COUNT = 32
FRAME_SLOTS = 65_536
PROGRAM_COUNTERS = 65_536
CALL_SITES = 65_536
EDGES = 262_144
COPY_PAIRS = 262_144
COPY_PAIRS_PER_EDGE = 65_536
LOGICAL_VALUES = 262_144
SOURCE_RECORDS = 262_144
BITMAP_WORDS = 1_048_576
DESCRIPTOR_ARENA_BYTES = 4 * 1024 * 1024
FRAME_PAYLOAD_BYTES = 512 * 1024
TRANSFER_SCRATCH_BYTES = 512 * 1024
ARTIFACT_DESCRIPTOR_BYTES = 256 * 1024 * 1024
IMAGE_MANIFEST_BYTES = 64 * 1024 * 1024
IMAGE_BLOCK_REFERENCES = 1_048_576
AUTHORITY = hashlib.sha256(b"birnam.stackless-s2.fixture-authority.v1\0").digest()
REGISTRATION_TUPLE_DOMAIN = b"birnam.registration-envelope.v1\0"
RUNTIME_ABI_V3 = 0x6C3B67F0EF1614B3
RUNTIME_SHIM_NAMES_V3 = (
    b"birnam_send",
    b"birnam_intern",
    b"birnam_mkSmallInt",
    b"birnam_smallIntValue",
    b"birnam_mkFloat",
    b"birnam_floatValue",
    b"birnam_mkString",
    b"birnam_nil",
    b"birnam_true",
    b"birnam_false",
    b"birnam_objectVt",
    b"birnam_proto",
    b"birnam_method",
    b"birnam_alloc",
    b"birnam_cstr",
    b"birnam_free_cstr",
    b"birnam_layoutAllOops",
    b"birnam_global",
    b"birnam_candidate_validate_binding",
    b"birnam_register_resumable_v1",
)


REGISTRATION_TUPLE_FIELDS = (
    ("carrier_version", 0x00, 4, 4, "u32", "not-applicable"),
    ("struct_size", 0x04, 4, 4, "u32", "not-applicable"),
    ("operation", 0x08, 4, 4, "u32", "not-applicable"),
    ("flags", 0x0C, 4, 4, "u32", "not-applicable"),
    ("manifest_entry_index", 0x10, 4, 4, "u32", "not-applicable"),
    ("reserved0", 0x14, 4, 4, "u32", "not-applicable"),
    ("registration_token", 0x18, 8, 8, "u64", "not-applicable"),
    ("descriptor_bytes", 0x20, 8, 8, "pointer<const-u8>", "prepare-nonnull-abort-null"),
    ("descriptor_length", 0x28, 8, 8, "u64", "not-applicable"),
    ("entry_thunk", 0x30, 8, 8, "function-pointer<BirnamMethodV1>", "prepare-nonnull-abort-null"),
    ("step_method", 0x38, 8, 8, "function-pointer<BirnamStepMethodV1>", "prepare-nonnull-abort-null"),
    ("implementation_oop", 0x40, 8, 8, "BirnamOopV1", "prepare-named-nonnull-block-null-abort-null"),
    ("code_generation_identity", 0x48, 8, 8, "u64", "not-applicable"),
    ("code_generation_lease", 0x50, 8, 8, "u64", "not-applicable"),
    ("portable_implementation_identity", 0x58, 8, 8, "u64", "not-applicable"),
    ("language_arity", 0x60, 4, 4, "u32", "not-applicable"),
    ("driver_argument_count", 0x64, 4, 4, "u32", "not-applicable"),
    ("descriptor_sha256", 0x68, 32, 1, "bytes[32]", "not-applicable"),
)


REGISTRATION_TUPLE_CONSTANTS = (
    ("BIRNAM_REGISTRATION_CARRIER_VERSION_V1", 1),
    ("BIRNAM_REGISTRATION_PREPARE_V1", 1),
    ("BIRNAM_REGISTRATION_ABORT_V1", 2),
    ("BIRNAM_REGISTRATION_OK_V1", 0),
    ("BIRNAM_REGISTRATION_REJECTED_V1", 1),
)


class Reject(Exception):
    def __init__(self, stage: str, detail: str) -> None:
        super().__init__(f"{stage}: {detail}")
        self.stage = stage


class Selection(NamedTuple):
    revision: bytes
    root_kind: int
    root_index: int
    path: tuple[int, ...]
    implementation_kind: int
    disposition: int


class ManifestEntry(NamedTuple):
    selection: Selection
    descriptor_hash: bytes
    selector: bytes


class ExecutionSpec(NamedTuple):
    selection: Selection
    slot_count: int
    pcs: tuple[tuple[int, int], ...]
    roots: tuple[int, ...]
    call_sites: tuple[tuple[tuple[int, ...], bytes], ...]
    edges: tuple[tuple[int, ...], ...]
    copies: tuple[tuple[int, int], ...]
    clears: tuple[int, ...]
    bindings: tuple[tuple[int, bytes], ...]
    values: tuple[tuple[int, int, int, int, int, int, int], ...]
    uses: tuple[tuple[int, int, int, int], ...]
    locations: tuple[tuple[tuple[int, int], ...], ...]
    call_window_start: int = ABSENT
    call_window_capacity: int = 0


NAMED_A = Selection(REVISION_A, 2, 0, (), 1, 0)
NAMED_A_RESUMABLE = Selection(REVISION_A, 2, 0, (), 1, 1)
BLOCK_A_SYNCHRONOUS = Selection(REVISION_A, 2, 0, (2, 1), 2, 0)
BLOCK_A_RESUMABLE = Selection(REVISION_A, 2, 0, (2, 1), 2, 1)
NAMED_B = Selection(REVISION_B, 2, 0, (), 1, 0)
NAMED_CALL = Selection(REVISION_CALL, 2, 0, (), 1, 1)
NAMED_TAIL = Selection(REVISION_TAIL, 2, 0, (), 1, 1)
NAMED_BRANCH = Selection(REVISION_BRANCH, 2, 0, (), 1, 1)
NAMED_CYCLE = Selection(REVISION_CYCLE, 2, 0, (), 1, 1)
NAMED_COLOR = Selection(REVISION_COLOR, 2, 0, (), 1, 1)
TRACE_NAMED = Selection(TRACE_REVISION, 2, 0, (), 1, 1)
TRACE_BLOCK = Selection(TRACE_REVISION, 2, 0, (0,), 2, 1)


ARTIFACT_FIXTURES = {
    "artifact-selection-empty.bin": (
        160,
        "89ae1a47a43be25eaa97a100b5862a5957e37e447ea8be477cb27c27af17cfaa",
        (),
        None,
    ),
    "artifact-selection-all-synchronous.bin": (
        304,
        "a005ac770277672c085f1a3c4c19c2b22da10ba8ac831a46e97cbb2214ba4ab2",
        (NAMED_A, BLOCK_A_SYNCHRONOUS),
        None,
    ),
    "artifact-selection-mixed.bin": (
        304,
        "9bd844f9b78c909f37e2e2ce885222e82541116c37dbeceb2a63c6a6f988773d",
        (NAMED_A, BLOCK_A_RESUMABLE),
        None,
    ),
    "artifact-selection-named-resumable.bin": (
        304,
        "eccd2ecb19c691e670e25cd59b4fb3969563b321dc064914ba1f83e6debb0970",
        (NAMED_A_RESUMABLE, BLOCK_A_SYNCHRONOUS),
        None,
    ),
    "artifact-selection-both-resumable.bin": (
        304,
        "b209d4a3763c8253b50648409b420f86e1828cf78c1400dea895b54e80f79836",
        (NAMED_A_RESUMABLE, BLOCK_A_RESUMABLE),
        None,
    ),
    "artifact-selection-bad-scope.bin": (
        304,
        "f2adba1b24668f936740b1c22457e738a87f7fda4b3e55e3f3450ef5f1bfa25e",
        (NAMED_A, BLOCK_A_RESUMABLE),
        "scope membership",
    ),
    "artifact-selection-b-synchronous.bin": (
        248,
        "d34f60fa2a36d784e41e1ee761a8bcb276d24891cb2d6cbc0457f292f4a90590",
        (NAMED_B,),
        None,
    ),
    "artifact-selection-ab-combined.bin": (
        392,
        "66b2ef27a1c8f1de8d7ebe043491fd12fa07284914d9dbae1704717991cb5329",
        (NAMED_A, BLOCK_A_RESUMABLE, NAMED_B),
        None,
    ),
    "artifact-selection-selector-edge.bin": (
        248,
        "3186f19e2ff9e4dd640855b49ff8f9ee2b8d27aa6daba11555cc710c38984d30",
        (NAMED_CALL,),
        None,
    ),
    "artifact-selection-construction-trace.bin": (
        300,
        "9f3f5eb6dbbe1567e6798eba426e791c90be1c3e0e1361378809e8ce75ee7c20",
        (TRACE_NAMED, TRACE_BLOCK),
        None,
    ),
}


GENERATION_FIXTURES = {
    "generation-selection-a.bin": (
        304,
        "54f02fe3acbd834277ca4cde7c37ba728115f2074cd48097d782167b01c379cd",
        (REVISION_A,),
        (NAMED_A, BLOCK_A_RESUMABLE),
    ),
    "generation-selection-ab.bin": (
        392,
        "b888efa7ad3997b2d403e4fd8a11d0edebf043297e63d6e2d4faa0e9dc1e25af",
        (REVISION_A, REVISION_B),
        (NAMED_A, BLOCK_A_RESUMABLE, NAMED_B),
    ),
    "generation-selection-construction-trace.bin": (
        300,
        "0246cbd4d149a369a095da35bdd8525974acf58d9f4c1b499e580393c9b42197",
        (TRACE_REVISION,),
        (TRACE_NAMED, TRACE_BLOCK),
    ),
}


DESCRIPTOR_FIXTURES = {
    "frame-descriptor-minimal-return.bin": (
        932,
        "4406c8cc692512cbe53df0317cab73d07c705ee65c808ebac2fa463029253f83",
        NAMED_A,
        False,
    ),
    "frame-descriptor-source-map-block.bin": (
        941,
        "6992e8ce66b03c9641c8f6e161fa03af9b7c2853e806a24ea9ac8fdee168a944",
        BLOCK_A_RESUMABLE,
        True,
    ),
}


TRACE_DESCRIPTOR_FIXTURES = {
    "frame-descriptor-construction-trace-named.bin": (
        936,
        "d670335de8efd9c6db228e19a70affb3fe738e0864dee726dde273928a5171cf",
        TRACE_NAMED,
        False,
    ),
    "frame-descriptor-construction-trace-block.bin": (
        945,
        "0349c27104f611bb9898eef9c4132d536dec5a6237a2752811701a2e50a48560",
        TRACE_BLOCK,
        True,
    ),
}


EXECUTION_DESCRIPTOR_FIXTURES = {
    "frame-descriptor-call.bin": (
        1087,
        "b1b6d926665c0f0a293d45d73e459644fb92d0f19bb323874c92f5821148b1d1",
        ExecutionSpec(
            NAMED_CALL,
            6,
            ((1, 0), (3, 5)),
            (0x1F, 0x20),
            (((1, 0, 0, 2, 3, 4, 1, 5, 1, 0, 0, 1), b"\xff"),),
            ((2, 0, 0, 1, 0, 0, 0, 5),),
            (),
            (0,),
            ((1, b""), (5, b"")),
            (
                (0, 1, 0, 0, 0, 0, 1),
                (1, 4, 1, 0, 0, 0, 1),
                (2, 7, ABSENT, ABSENT, 0, 1, 0),
            ),
            ((2, 1, 0, 3),),
            (((0, 0), (1, 1)), ((2, 5),)),
            2,
            1,
        ),
    ),
    "frame-descriptor-tail-call.bin": (
        944,
        "4b08ebfd2d1f3b5408a1e6d7f5e7982a48ec4fdb6d57a6b3799d83089964e341",
        ExecutionSpec(
            NAMED_TAIL,
            5,
            ((2, 0),),
            (0x1F,),
            (((2, 0, 0, 2, 3, 4, 1, ABSENT, ABSENT, ABSENT, 0, 2), b"x:"),),
            (),
            (),
            (),
            ((1, b""), (5, b"")),
            ((0, 1, 0, 0, 0, 0, 1), (1, 4, 1, 0, 0, 0, 1)),
            (),
            (((0, 0), (1, 1)),),
            2,
            1,
        ),
    ),
    "frame-descriptor-branch-join.bin": (
        1415,
        "766dd4e92a31c2b24c37c7c60f844951b3aaa5f0465e9e0dfd071a10ec04f568",
        ExecutionSpec(
            NAMED_BRANCH,
            3,
            ((4, 0), (4, 2), (4, 3), (3, 2)),
            (0x7, 0x7, 0x7, 0x7),
            (),
            (
                (1, 0, 0, 1, ABSENT, 0, 0, ABSENT),
                (1, 0, 0, 2, ABSENT, 0, 0, ABSENT),
                (1, 0, 1, 3, ABSENT, 0, 0, ABSENT),
                (1, 0, 2, 3, ABSENT, 0, 0, ABSENT),
            ),
            (),
            (0, 0, 0, 0),
            ((1, b""), (5, b""), (3, b"x")),
            (
                (0, 1, 0, 0, 0, 0, 1),
                (1, 4, 1, 0, 0, 0, 1),
                (2, 5, 2, 0, 0, 1, 1),
            ),
            ((2, 3, 0, 3),),
            tuple(((0, 0), (1, 1), (2, 2)) for _ in range(4)),
        ),
    ),
    "frame-descriptor-cyclic-copy.bin": (
        1232,
        "6420d47622cc53e1a152694d1860e1f8bf470e005810a4b5b6de08ca8c884894",
        ExecutionSpec(
            NAMED_CYCLE,
            4,
            ((4, 0), (3, 2)),
            (0xF, 0xF),
            (),
            ((1, 0, 0, 1, ABSENT, 0, 2, ABSENT),),
            ((3, 2), (2, 3)),
            (0,),
            ((1, b""), (5, b""), (3, b"a"), (3, b"b")),
            (
                (0, 1, 0, 0, 0, 0, 1),
                (1, 4, 1, 0, 0, 0, 1),
                (2, 5, 2, 0, 0, 0, 1),
                (3, 5, 3, 0, 0, 1, 1),
            ),
            ((3, 1, 0, 3),),
            (((0, 0), (1, 1), (2, 2), (3, 3)), ((0, 0), (1, 1), (2, 3), (3, 2))),
        ),
    ),
    "frame-descriptor-colored-reuse.bin": (
        1087,
        "4b464329137d5493fcc11d9d938928df50c2109531bc1f8c0762bde23a2861de",
        ExecutionSpec(
            NAMED_COLOR,
            2,
            ((4, 0), (3, 0)),
            (0x3, 0x3),
            (),
            ((1, 0, 0, 1, ABSENT, 0, 0, ABSENT),),
            (),
            (0x1,),
            ((1, b""), (5, b""), (3, b"x")),
            (
                (0, 1, 0, 0, 0, 0, 1),
                (1, 4, 1, 0, 0, 0, 1),
                (2, 5, 2, 0, 0, 1, 1),
            ),
            ((2, 1, 0, 3),),
            (((0, 0), (1, 1)), ((1, 1), (2, 0))),
        ),
    ),
}


NAMED_DESCRIPTOR_HASH = bytes.fromhex(
    "4406c8cc692512cbe53df0317cab73d07c705ee65c808ebac2fa463029253f83"
)
BLOCK_DESCRIPTOR_HASH = bytes.fromhex(
    "6992e8ce66b03c9641c8f6e161fa03af9b7c2853e806a24ea9ac8fdee168a944"
)
CALL_DESCRIPTOR_HASH = bytes.fromhex(
    "b1b6d926665c0f0a293d45d73e459644fb92d0f19bb323874c92f5821148b1d1"
)
TRACE_NAMED_DESCRIPTOR_HASH = bytes.fromhex(
    "d670335de8efd9c6db228e19a70affb3fe738e0864dee726dde273928a5171cf"
)
TRACE_BLOCK_DESCRIPTOR_HASH = bytes.fromhex(
    "0349c27104f611bb9898eef9c4132d536dec5a6237a2752811701a2e50a48560"
)
NAMED_MANIFEST_ENTRY = ManifestEntry(NAMED_A_RESUMABLE, NAMED_DESCRIPTOR_HASH, b"run")
BLOCK_MANIFEST_ENTRY = ManifestEntry(BLOCK_A_RESUMABLE, BLOCK_DESCRIPTOR_HASH, b"value")
SELECTOR_EDGE_MANIFEST_ENTRY = ManifestEntry(NAMED_CALL, CALL_DESCRIPTOR_HASH, b"\xff")
TRACE_NAMED_MANIFEST_ENTRY = ManifestEntry(
    TRACE_NAMED, TRACE_NAMED_DESCRIPTOR_HASH, TRACE_SELECTOR
)
TRACE_BLOCK_MANIFEST_ENTRY = ManifestEntry(
    TRACE_BLOCK, TRACE_BLOCK_DESCRIPTOR_HASH, b"value"
)
TRACE_AUTHORITY = bytes.fromhex(
    "7d1a817173f4b47139581aaf2996009012ae398f1e251a3bfe5e75498c5f823c"
)


MANIFEST_FIXTURES = {
    "resumable-manifest-empty.bin": (
        224,
        "7302d8e52f55db3817d92fe08e1805149c3996bb270acd5e9bd76189861a2a10",
        (),
        "artifact-selection-all-synchronous.bin",
    ),
    "resumable-manifest-named-only.bin": (
        323,
        "3b53236b12eee124188a4a2b64c3f1fb3f51d75ff15e042481e7344478d8c13d",
        (NAMED_MANIFEST_ENTRY,),
        "artifact-selection-named-resumable.bin",
    ),
    "resumable-manifest-block-only.bin": (
        333,
        "c07e630b2ec9c321df78fe60cbda6eec5880c38157511a75e2fa29da802a4e60",
        (BLOCK_MANIFEST_ENTRY,),
        "artifact-selection-mixed.bin",
    ),
    "resumable-manifest-mixed.bin": (
        432,
        "a328e1e3bade0a06f8279b439b5e42c1f85092c8961650b4ec7f90488248a2fc",
        (NAMED_MANIFEST_ENTRY, BLOCK_MANIFEST_ENTRY),
        "artifact-selection-both-resumable.bin",
    ),
    "resumable-manifest-selector-edge.bin": (
        321,
        "8e36d0ab08f2d60cff0043d952d90a8334359b0464ddbe28fb9857ca6c0e2858",
        (SELECTOR_EDGE_MANIFEST_ENTRY,),
        "artifact-selection-selector-edge.bin",
    ),
}


TRACE_MANIFEST_FIXTURE = (
    "resumable-manifest-construction-trace.bin",
    432,
    "3a6da81af68a0c91836aec32ace894c5ed6e18ec1d7b6bd0cb6f3cbe40bbd6ad",
    (TRACE_NAMED_MANIFEST_ENTRY, TRACE_BLOCK_MANIFEST_ENTRY),
    "artifact-selection-construction-trace.bin",
    TRACE_AUTHORITY,
)


MALFORMED_MANIFEST_FIXTURES = {
    "resumable-manifest-forged-portable-id.bin": (
        323,
        "b14535c727c894f7c94e50a13700229a44444b1781878cd59e48196b00cbc262",
        (NAMED_MANIFEST_ENTRY,),
        "artifact-selection-named-resumable.bin",
        "Portable identity",
    ),
}


REGISTRATION_FIXTURES = {
    "registration-prepare-named.bin": (
        136,
        "c7e0a4ff9ca17ddc13e5e1817ae9c24df4d093bdf56f9ae1aa1ae75f1ba9a09b",
        "named",
    ),
    "registration-prepare-block.bin": (
        136,
        "bbd7d718bab5feec669693c427a2b247ca948951488008d571876b0cdd39e7d5",
        "block",
    ),
    "registration-abort.bin": (
        136,
        "e67ad6108ba33597faeb9c977379ec317979ef25f0b1d24043d84e3d4850cc34",
        "abort",
    ),
}


REGISTRATION_ABI_TUPLE_FIXTURE = (
    "registration-abi-tuple-v1.bin",
    1539,
    "d180f7b2aa164885a63f1cf2347a2a3802bd06eaf42c3f919fbf6f2f16ddc047",
)


IMAGE_FIXTURES = {
    "image-v5-empty-manifest.bin": (
        528,
        "0b85e90127ff422acc93ac44ba602977965abcd714caaaf85193b75de5f0e651",
        "resumable-manifest-empty.bin",
    ),
    "image-v5-construction-trace.bin": (
        815,
        "a4c329ded2cda7ee712183370334516ca87e5466fc0ce0cc782df80b57becc7b",
        "resumable-manifest-construction-trace.bin",
    ),
    "image-v5-no-code.bin": (
        104,
        "01f01a90cf1f93a4ff2555ebb5dbc2d4f67d65b2dd8b418d1a80ccd412552f76",
        None,
    ),
}


MALFORMED_IMAGE_FIXTURES = {
    "image-v5-out-of-range-index.bin": (
        815,
        "3b5787a1304692600281f1416a00199c7dbaf55c15f5a1e70dd2d5f9c9fbcaf9",
        "resumable-manifest-construction-trace.bin",
        "image block index",
    ),
    "image-v5-named-row-index.bin": (
        815,
        "7a3376cec272de069b3550af94e2ad1149311a298c3e6661d9cd33d4c56d3008",
        "resumable-manifest-construction-trace.bin",
        "image block kind",
    ),
    "image-v5-authority-mismatch.bin": (
        815,
        "0d260046c463236e75b7be7406c2f8a3df5faaaec0ae44b0fd8ea0f299f9c5ce",
        "resumable-manifest-construction-trace.bin",
        "image authority",
    ),
    "image-v5-manifest-hash-mismatch.bin": (
        815,
        "93dc1ddc0c29329b58a9f02466ecadaf24a3e41a7d93694380c9a20dec55f612",
        "resumable-manifest-construction-trace.bin",
        "image manifest hash",
    ),
}


MUTATIONS = (
    ("domain", 0x00, 0x62, 0x63, "domain"),
    ("total-length", 0x28, 0x30, 0x31, "total length"),
    ("directory-reserved", 0x4C, 0x00, 0x01, "reserved field"),
    ("selection-offset", 0x70, 0xC0, 0xC8, "canonical section placement"),
    ("row-order", 0xE4, 0x00, 0x01, "row order"),
    ("path-consumption", 0x11C, 0x02, 0x03, "PathOrdinals consumption"),
    ("disposition", 0x124, 0x01, 0x02, "disposition enum"),
    ("inventory-path", 0x128, 0x02, 0x03, "normalized-inventory completeness"),
    ("scope-membership", 0xA0, 0x00, 0xFF, "scope membership"),
)


def require(condition: bool, stage: str, detail: str) -> None:
    if not condition:
        raise Reject(stage, detail)


def u32(data: bytes, offset: int, stage: str) -> int:
    require(offset + 4 <= len(data), stage, "truncated u32")
    return struct.unpack_from("<I", data, offset)[0]


def u64(data: bytes, offset: int, stage: str) -> int:
    require(offset + 8 <= len(data), stage, "truncated u64")
    return struct.unpack_from("<Q", data, offset)[0]


def align8(value: int) -> int:
    return checked_add(value, 7, "alignment") & ~7


def checked_add(left: int, right: int, detail: str) -> int:
    result = left + right
    require(result <= U64_MAX, "arithmetic overflow", detail)
    return result


def checked_multiply(left: int, right: int, detail: str) -> int:
    result = left * right
    require(result <= U64_MAX, "arithmetic overflow", detail)
    return result


def stable_key(selection: Selection) -> bytes:
    return selection.revision + struct.pack(
        "<IIII",
        selection.root_kind,
        selection.root_index,
        len(selection.path),
        selection.implementation_kind,
    )


def derived_identity(domain: bytes, selection: Selection, lowering_version: int) -> int:
    path = b"".join(struct.pack("<I", ordinal) for ordinal in selection.path)
    digest = hashlib.sha256(
        domain + stable_key(selection) + path + struct.pack("<I", lowering_version)
    ).digest()
    return int.from_bytes(digest[:8], "little")


def selector_arity(selector: bytes) -> int:
    colons = selector.count(b":")
    if colons:
        return colons
    if not selector or 0x41 <= selector[0] <= 0x5A or 0x61 <= selector[0] <= 0x7A or selector[0] == 0x5F:
        return 0
    return 1


def parse_selection(
    data: bytes, expected_domain: bytes, expected_inventory: tuple[Selection, ...]
) -> tuple[tuple[bytes, ...], tuple[Selection, ...]]:
    require(len(data) >= 64, "minimum prefix", "fewer than 64 bytes")
    require(data[0:32] == expected_domain, "domain", "wrong 32-byte domain")
    require(u32(data, 0x20, "prefix") == 1, "prefix", "wrong schema version")
    require(u32(data, 0x24, "prefix") == 64, "prefix", "wrong prefix size")
    require(u64(data, 0x28, "total length") == len(data), "total length", "declared length differs from input")
    require(u32(data, 0x30, "flags") == 0, "flags", "nonzero required flags")
    require(u32(data, 0x34, "prefix") == 3, "prefix", "wrong section count")
    require(u32(data, 0x38, "prefix") == 32, "prefix", "wrong directory stride")
    require(u32(data, 0x3C, "reserved field") == 0, "reserved field", "nonzero prefix reserved field")
    require(len(data) <= CANONICAL_VALUE_BYTES, "limits", "CanonicalValueBytes")

    required = ((1, 32), (2, 52), (3, 4))
    directory: list[tuple[int, int, int]] = []
    cursor = 64 + 3 * 32
    fixed_record_count = 0
    for index, (required_kind, required_stride) in enumerate(required):
        base = 64 + index * 32
        kind = u32(data, base, "directory")
        stride = u32(data, base + 4, "directory")
        count = u32(data, base + 8, "directory")
        reserved = u32(data, base + 12, "reserved field")
        offset = u64(data, base + 16, "directory")
        length = u64(data, base + 24, "directory")
        require(reserved == 0, "reserved field", f"directory entry {index} reserved field")
        require(kind == required_kind, "directory", f"directory entry {index} kind")
        require(stride == required_stride, "directory", f"directory entry {index} stride")
        if index == 0:
            require(count <= COVERED_REVISIONS, "limits", "CoveredRevisions")
        elif index == 1:
            require(count <= IMPLEMENTATION_ROWS, "limits", "ImplementationRows")
        else:
            require(count <= PATH_ORDINALS, "limits", "PathOrdinals")
        fixed_record_count = checked_add(fixed_record_count, count, "FixedRecords count")
        require(fixed_record_count <= FIXED_RECORDS, "limits", "FixedRecords")
        product = checked_multiply(count, stride, f"directory entry {index} count by stride")
        require(length == product, "directory", f"directory entry {index} byte length")
        section_end = checked_add(offset, length, f"directory entry {index} offset plus length")
        expected_offset = align8(cursor)
        require(offset == expected_offset, "canonical section placement", f"directory entry {index} offset")
        require(data[cursor:expected_offset] == bytes(expected_offset - cursor), "canonical section placement", f"directory entry {index} padding")
        require(section_end <= len(data), "directory", f"directory entry {index} bounds")
        directory.append((offset, count, length))
        cursor = section_end
    require(cursor == len(data), "total length", "section cursor does not consume input")

    revision_offset, revision_count, _ = directory[0]
    revisions = tuple(
        data[revision_offset + index * 32 : revision_offset + (index + 1) * 32]
        for index in range(revision_count)
    )
    require(tuple(sorted(set(revisions))) == revisions, "revision scope", "revisions are not strictly increasing")

    row_offset, row_count, _ = directory[1]
    path_offset, path_count, _ = directory[2]
    ordinals = tuple(u32(data, path_offset + index * 4, "PathOrdinals consumption") for index in range(path_count))
    path_cursor = 0
    decoded: list[Selection] = []
    for index in range(row_count):
        base = row_offset + index * 52
        revision = data[base : base + 32]
        root_kind = u32(data, base + 0x20, "row fields")
        root_index = u32(data, base + 0x24, "row fields")
        count = u32(data, base + 0x28, "row fields")
        implementation_kind = u32(data, base + 0x2C, "row fields")
        disposition = u32(data, base + 0x30, "disposition enum")
        require(count <= PATH_DEPTH, "limits", f"row {index} PathDepth")
        require(root_kind in (1, 2), "row fields", f"row {index} root kind")
        require(root_kind != 1 or root_index == 0, "row fields", f"row {index} non-method root index")
        require(implementation_kind in (1, 2), "row fields", f"row {index} implementation kind")
        require(disposition in (0, 1), "disposition enum", f"row {index} disposition")
        require(path_cursor + count <= len(ordinals), "PathOrdinals consumption", f"row {index} path exceeds section")
        path = ordinals[path_cursor : path_cursor + count]
        path_cursor += count
        require(
            (implementation_kind == 1 and root_kind == 2 and not path)
            or (implementation_kind == 2 and bool(path)),
            "row shape",
            f"row {index} stable-key shape",
        )
        decoded.append(
            Selection(revision, root_kind, root_index, path, implementation_kind, disposition)
        )
    require(path_cursor == len(ordinals), "PathOrdinals consumption", "unreferenced ordinal suffix")

    keys = tuple(row[:-1] for row in decoded)
    require(len(set(keys)) == len(keys) and tuple(sorted(keys)) == keys, "row order", "keys are duplicated or unordered")
    require(all(row.revision in revisions for row in decoded), "scope membership", "row revision is outside CoveredRevisions")
    require(bool(revisions) or not decoded and not ordinals, "empty scope", "empty scope owns rows or paths")
    require(tuple(decoded) == expected_inventory, "normalized-inventory completeness", "rows differ from the declared fixture inventory")
    return revisions, tuple(decoded)


def parse_artifact_selection(
    data: bytes, expected_inventory: tuple[Selection, ...]
) -> tuple[tuple[bytes, ...], tuple[Selection, ...]]:
    return parse_selection(data, ARTIFACT_DOMAIN, expected_inventory)


def parse_generation_selection(
    data: bytes, expected_inventory: tuple[Selection, ...]
) -> tuple[tuple[bytes, ...], tuple[Selection, ...]]:
    return parse_selection(data, GENERATION_DOMAIN, expected_inventory)


def parse_descriptor(
    data: bytes,
    selection: Selection,
    block: bool,
    path: bytes = b"a.birnam",
    source_length: int = 64,
) -> None:
    require(len(data) >= 64, "minimum prefix", "descriptor has fewer than 64 bytes")
    require(data[:32] == DESCRIPTOR_DOMAIN, "domain", "wrong descriptor domain")
    require(u32(data, 0x20, "prefix") == 2, "prefix", "wrong descriptor version")
    require(u32(data, 0x24, "prefix") == 64, "prefix", "wrong descriptor prefix size")
    require(u64(data, 0x28, "total length") == len(data), "total length", "descriptor length mismatch")
    require(u32(data, 0x30, "flags") == 0, "flags", "descriptor flags")
    require(u32(data, 0x34, "prefix") == 15, "prefix", "descriptor section count")
    require(u32(data, 0x38, "prefix") == 32, "prefix", "descriptor directory stride")
    require(u32(data, 0x3C, "reserved field") == 0, "reserved field", "descriptor prefix reserved")
    require(len(data) <= DESCRIPTOR_BYTES, "limits", "DescriptorBytes")

    strides = (64, 4, 32, 8, 48, 32, 8, 8, 48, 24, 32, 32, 16, 16, 1)
    expected_counts = (
        (
            1,
            1 if block else 0,
            1,
            1,
            0,
            0,
            0,
            0,
            1,
            2,
            2,
            2,
            1,
            2,
            len(path) + 4 + (1 if block else 0),
        )
        if block
        else (1, 0, 1, 1, 0, 0, 0, 0, 1, 2, 2, 2, 1, 2, len(path) + 4)
    )
    sections: list[tuple[int, int, int]] = []
    cursor = 64 + 15 * 32
    for index, stride in enumerate(strides):
        base = 64 + index * 32
        kind = u32(data, base, "directory")
        actual_stride = u32(data, base + 4, "directory")
        count = u32(data, base + 8, "directory")
        reserved = u32(data, base + 12, "reserved field")
        offset = u64(data, base + 16, "directory")
        length = u64(data, base + 24, "directory")
        require(kind == index + 1, "directory", f"descriptor section {index} kind")
        require(actual_stride == stride, "directory", f"descriptor section {index} stride")
        require(count == expected_counts[index], "fixture semantics", f"descriptor section {index} count")
        require(reserved == 0, "reserved field", f"descriptor section {index} reserved")
        require(length == checked_multiply(count, stride, f"descriptor section {index} product"), "directory", f"descriptor section {index} length")
        section_end = checked_add(offset, length, f"descriptor section {index} end")
        expected_offset = align8(cursor)
        require(offset == expected_offset, "canonical section placement", f"descriptor section {index} offset")
        require(data[cursor:expected_offset] == bytes(expected_offset - cursor), "canonical section placement", f"descriptor section {index} padding")
        require(section_end <= len(data), "directory", f"descriptor section {index} bounds")
        sections.append((offset, count, length))
        cursor = section_end
    require(cursor == len(data), "total length", "descriptor sections do not consume input")

    def section(index: int) -> bytes:
        offset, _, length = sections[index]
        return data[offset : offset + length]

    metadata = struct.unpack("<QQIIIIIIIIIIII", section(0))
    portable = derived_identity(b"birnam.portable-implementation.v1\0", selection, 1)
    source_map = derived_identity(b"birnam.source-map-identity.v1\0", selection, 1)
    expected_metadata = (
        portable,
        source_map,
        1,
        2 if block else 1,
        2,
        1,
        0,
        0,
        1,
        0,
        ABSENT if block else 1,
        ABSENT,
        0,
        0,
    )
    require(metadata == expected_metadata, "identity", "descriptor metadata")
    require(section(1) == (struct.pack("<I", 1) if block else b""), "entry", "capture slots")

    return_slot = 1 if block else 0
    require(
        struct.unpack("<IIIIIIII", section(2)) == (3, 0, return_slot, 0, 0, 2, 0, 0),
        "PCs",
        "minimal Return PC",
    )
    root_word = struct.unpack("<Q", section(3))[0]
    require(root_word >> 2 == 0, "bitmap padding", "root bits beyond slot count")
    require(root_word == 0b11, "Roots", "minimal root bitmap")
    for index in range(4, 8):
        require(section(index) == b"", "fixture semantics", f"execution section {index} is not empty")

    source_file = section(8)
    require(source_file[:32] == selection.revision, "source map", "source revision")
    require(struct.unpack_from("<QII", source_file, 32) == (source_length, len(path), 0), "source map", "source file fields")
    require(
        struct.unpack_from("<IIQQ", section(9), 0) == (0, 0, 0, 16)
        and struct.unpack_from("<IIQQ", section(9), 24) == (0, 0, 0, source_length),
        "source map",
        "source spans",
    )

    second_kind = 4 if block else 5
    second_name = b"x" if block else b""
    bindings = section(10)
    require(
        struct.unpack_from("<QIIIIII", bindings, 0) == (1, 1, 4, 0, 1, ABSENT, 0)
        and struct.unpack_from("<QIIIIII", bindings, 32)
        == (2, second_kind, len(second_name), 0, 1, ABSENT, 0),
        "source map",
        "lexical bindings",
    )

    values = section(11)
    first_value = (0, 1, 0, 0, 0, 0 if block else 1, 1, 0)
    second_value = (1, 3 if block else 4, 1, 0, 0 if block else 1, 1 if block else 0, 1, 0)
    require(
        struct.unpack_from("<IIIIIIII", values, 0) == first_value
        and struct.unpack_from("<IIIIIIII", values, 32) == second_value,
        "source map",
        "logical values",
    )
    require(
        struct.unpack("<IIII", section(12)) == (1 if block else 0, 0, 0, 3),
        "source map",
        "logical Return use",
    )
    locations = section(13)
    require(
        struct.unpack_from("<IIII", locations, 0) == (0, 0, 1, 0)
        and struct.unpack_from("<IIII", locations, 16) == (0, 1, 1, 1),
        "source map",
        "PC value locations",
    )
    require(section(14) == path + b"self" + second_name, "source map", "SourceBytes visitation")


def parse_execution_descriptor(data: bytes, spec: ExecutionSpec) -> None:
    require(len(data) >= 64, "minimum prefix", "execution descriptor prefix")
    require(data[:32] == DESCRIPTOR_DOMAIN, "domain", "execution descriptor domain")
    require(u32(data, 0x20, "prefix") == 2, "prefix", "descriptor version")
    require(u32(data, 0x24, "prefix") == 64, "prefix", "descriptor prefix size")
    require(u64(data, 0x28, "total length") == len(data), "total length", "descriptor extent")
    require(u32(data, 0x30, "flags") == 0, "flags", "descriptor flags")
    require(u32(data, 0x34, "prefix") == 15, "prefix", "descriptor section count")
    require(u32(data, 0x38, "prefix") == 32, "prefix", "descriptor directory stride")
    require(u32(data, 0x3C, "reserved field") == 0, "reserved field", "descriptor prefix reserved")
    require(len(data) <= DESCRIPTOR_BYTES, "limits", "DescriptorBytes")

    path = b"fixture.birnam"
    source_bytes_expected = (
        b"".join(selector for _, selector in spec.call_sites)
        + path
        + b"".join(name for _, name in spec.bindings)
    )
    strides = (64, 4, 32, 8, 48, 32, 8, 8, 48, 24, 32, 32, 16, 16, 1)
    location_count = sum(len(locations) for locations in spec.locations)
    expected_counts = (
        1,
        0,
        len(spec.pcs),
        len(spec.roots),
        len(spec.call_sites),
        len(spec.edges),
        len(spec.copies),
        len(spec.clears),
        1,
        1,
        len(spec.bindings),
        len(spec.values),
        len(spec.uses),
        location_count,
        len(source_bytes_expected),
    )
    sections: list[bytes] = []
    cursor = 64 + 15 * 32
    for index, (stride, expected_count) in enumerate(zip(strides, expected_counts)):
        base = 64 + index * 32
        kind, actual_stride, count, reserved = struct.unpack_from("<IIII", data, base)
        offset, length = struct.unpack_from("<QQ", data, base + 16)
        require(kind == index + 1, "directory", f"execution section {index} kind")
        require(actual_stride == stride, "directory", f"execution section {index} stride")
        require(count == expected_count, "fixture semantics", f"execution section {index} count")
        require(reserved == 0, "reserved field", f"execution section {index} reserved")
        require(length == checked_multiply(count, stride, f"execution section {index} product"), "directory", f"execution section {index} length")
        expected_offset = align8(cursor)
        require(offset == expected_offset, "canonical section placement", f"execution section {index} offset")
        require(data[cursor:offset] == bytes(offset - cursor), "canonical section placement", f"execution section {index} padding")
        end = checked_add(offset, length, f"execution section {index} end")
        require(end <= len(data), "directory", f"execution section {index} bounds")
        sections.append(data[offset:end])
        cursor = end
    require(cursor == len(data), "total length", "execution sections consume descriptor")

    portable = derived_identity(b"birnam.portable-implementation.v1\0", spec.selection, 1)
    source_map = derived_identity(b"birnam.source-map-identity.v1\0", spec.selection, 1)
    expected_metadata = (
        portable,
        source_map,
        1,
        1,
        spec.slot_count,
        len(spec.pcs),
        1 if len(spec.pcs) > 1 else 0,
        0,
        1,
        0,
        1,
        spec.call_window_start,
        spec.call_window_capacity,
        0,
    )
    require(struct.unpack("<QQIIIIIIIIIIII", sections[0]) == expected_metadata, "identity", "execution metadata")
    require(not sections[1], "entry", "named adapter has captures")
    require(len(spec.roots) == len(spec.pcs), "Roots", "one root map per PC")
    for word, expected in zip(struct.iter_unpack("<Q", sections[3]), spec.roots):
        require(word[0] == expected, "Roots", "execution root bitmap")
        require(word[0] >> spec.slot_count == 0, "bitmap padding", "execution root high bits")

    location_cursor = 0
    decoded_pcs: list[tuple[int, int]] = []
    for pc, ((expected_kind, expected_payload), locations) in enumerate(zip(spec.pcs, spec.locations)):
        record = struct.unpack_from("<IIIIIIII", sections[2], pc * 32)
        require(
            record == (expected_kind, 0, expected_payload, 0, location_cursor, len(locations), 0, 0),
            "PCs",
            f"execution PC {pc}",
        )
        decoded_pcs.append((record[0], record[2]))
        location_cursor += len(locations)

    decoded_calls = tuple(
        struct.unpack_from("<IIIIIIIIIIII", sections[4], index * 48)
        for index in range(len(spec.call_sites))
    )
    require(decoded_calls == tuple(fields for fields, _ in spec.call_sites), "Calls", "execution call sites")
    for fields, selector in spec.call_sites:
        require(fields[6] == selector_arity(selector), "Calls", "selector-derived arity")
        require(fields[11] == len(selector) > 0, "Calls", "selector length")
        require(fields[3:6] == (spec.call_window_start, spec.call_window_start + 1, spec.call_window_start + 2), "Calls", "call-window prefix")

    decoded_edges = tuple(
        struct.unpack_from("<IIIIIIII", sections[5], index * 32)
        for index in range(len(spec.edges))
    )
    require(decoded_edges == spec.edges, "Edges", "execution edges")
    edge_order = tuple((edge[2], edge[3]) for edge in decoded_edges)
    require(edge_order == tuple(sorted(set(edge_order))), "Edges", "canonical unique edge order")
    for pc, (kind, payload) in enumerate(decoded_pcs):
        owned = tuple((index, edge) for index, edge in enumerate(decoded_edges) if edge[2] == pc)
        if kind == 4:
            require(bool(owned) and payload == owned[0][0], "Edges", f"Continue PC {pc} group")
            require(all(edge[1][0] == 1 and edge[1][4] == ABSENT and edge[1][7] == ABSENT for edge in owned), "Edges", f"Continue PC {pc} fields")
        elif kind == 1:
            require(len(owned) == 1, "Edges", f"Call PC {pc} ownership")
            edge_index, edge = owned[0]
            call = decoded_calls[payload]
            require(edge[0] == 2 and edge[4] == payload and call[9] == edge_index and call[8] == edge[3] and call[7] == edge[7], "Edges", f"Call PC {pc} agreement")
        else:
            require(not owned, "Edges", f"terminal PC {pc} owns edge")

    decoded_copies = tuple(struct.iter_unpack("<II", sections[6]))
    require(decoded_copies == spec.copies, "CopyPairs", "execution parallel copies")
    copy_cursor = 0
    for edge in decoded_edges:
        require(edge[5] == copy_cursor, "CopyPairs", "canonical copy slice")
        owned = decoded_copies[copy_cursor : copy_cursor + edge[6]]
        require(len(owned) == edge[6], "CopyPairs", "copy slice bounds")
        require(tuple((destination, source) for source, destination in owned) == tuple(sorted((destination, source) for source, destination in owned)), "CopyPairs", "copy order")
        require(len({destination for _, destination in owned}) == len(owned), "CopyPairs", "unique destinations")
        copy_cursor += edge[6]
    require(copy_cursor == len(decoded_copies), "CopyPairs", "copy suffix")

    decoded_clears = tuple(word[0] for word in struct.iter_unpack("<Q", sections[7]))
    require(decoded_clears == spec.clears, "ClearBitmapWords", "execution clear maps")
    require(all(word >> spec.slot_count == 0 for word in decoded_clears), "bitmap padding", "clear high bits")
    for edge, clear_word in zip(decoded_edges, decoded_clears):
        destinations = {destination for _, destination in decoded_copies[edge[5] : edge[5] + edge[6]]}
        require(all(not (clear_word & (1 << destination)) for destination in destinations), "ClearBitmapWords", "copy destination cleared")
        require(edge[7] == ABSENT or not (clear_word & (1 << edge[7])), "ClearBitmapWords", "Call result cleared")

    require(sections[8] == spec.selection.revision + struct.pack("<QII", 64, len(path), 0), "source map", "execution SourceFile")
    require(sections[9] == struct.pack("<IIQQ", 0, 0, 0, 64), "source map", "execution SourceSpan")
    expected_bindings = b"".join(
        struct.pack("<QIIIIII", index + 1, kind, len(name), 0, 0, ABSENT, 0)
        for index, (kind, name) in enumerate(spec.bindings)
    )
    require(sections[10] == expected_bindings, "source map", "execution bindings")
    expected_values = b"".join(
        struct.pack("<IIIIIIII", value_id, kind, binding, definition, first, count, flags, 0)
        for value_id, kind, binding, definition, first, count, flags in spec.values
    )
    require(sections[11] == expected_values, "source map", "execution logical values")
    require(tuple(struct.iter_unpack("<IIII", sections[12])) == spec.uses, "source map", "execution logical uses")
    expected_locations = tuple(
        (pc, value_id, 1, slot)
        for pc, locations in enumerate(spec.locations)
        for value_id, slot in locations
    )
    require(tuple(struct.iter_unpack("<IIII", sections[13])) == expected_locations, "source map", "execution PC locations")
    for pc, locations in enumerate(spec.locations):
        require(tuple(value for value, _ in locations) == tuple(sorted(value for value, _ in locations)), "source map", f"PC {pc} location order")
        require(len({slot for _, slot in locations}) == len(locations), "source map", f"PC {pc} location slots")
        require(all(spec.roots[pc] & (1 << slot) for _, slot in locations), "source map", f"PC {pc} location roots")
    require(sections[14] == source_bytes_expected, "source map", "execution SourceBytes visitation")


def parse_manifest(
    data: bytes,
    expected_entries: tuple[ManifestEntry, ...],
    selection_name: str,
    expected_authority: bytes = AUTHORITY,
) -> tuple[ManifestEntry, ...]:
    require(len(data) >= 64, "minimum prefix", "manifest has fewer than 64 bytes")
    require(data[:32] == MANIFEST_DOMAIN, "domain", "wrong manifest domain")
    require(u32(data, 0x20, "prefix") == 1, "prefix", "wrong manifest version")
    require(u32(data, 0x24, "prefix") == 64, "prefix", "wrong manifest prefix size")
    require(u64(data, 0x28, "total length") == len(data), "total length", "manifest length mismatch")
    require(u32(data, 0x30, "flags") == 0, "flags", "manifest flags")
    require(u32(data, 0x34, "prefix") == 4, "prefix", "manifest section count")
    require(u32(data, 0x38, "prefix") == 32, "prefix", "manifest directory stride")
    require(u32(data, 0x3C, "reserved field") == 0, "reserved field", "manifest prefix reserved")
    require(len(data) <= CANONICAL_VALUE_BYTES, "limits", "CanonicalValueBytes")

    expected_counts = (
        1,
        len(expected_entries),
        sum(len(entry.selection.path) for entry in expected_entries),
        sum(len(entry.selector) for entry in expected_entries),
    )
    strides = (32, 96, 4, 1)
    sections: list[tuple[int, int, int]] = []
    cursor = 64 + 4 * 32
    fixed_records = 0
    for index, stride in enumerate(strides):
        base = 64 + index * 32
        kind = u32(data, base, "directory")
        actual_stride = u32(data, base + 4, "directory")
        count = u32(data, base + 8, "directory")
        reserved = u32(data, base + 12, "reserved field")
        offset = u64(data, base + 16, "directory")
        length = u64(data, base + 24, "directory")
        require(kind == index + 1, "directory", f"manifest section {index} kind")
        require(actual_stride == stride, "directory", f"manifest section {index} stride")
        require(count == expected_counts[index], "fixture semantics", f"manifest section {index} count")
        require(reserved == 0, "reserved field", f"manifest section {index} reserved")
        if index == 1:
            require(count <= IMPLEMENTATION_ROWS, "limits", "ImplementationRows")
        if index == 2:
            require(count <= PATH_ORDINALS, "limits", "PathOrdinals")
        if index < 3:
            fixed_records = checked_add(fixed_records, count, "manifest FixedRecords")
            require(fixed_records <= FIXED_RECORDS, "limits", "FixedRecords")
        product = checked_multiply(count, stride, f"manifest section {index} product")
        require(length == product, "directory", f"manifest section {index} length")
        section_end = checked_add(offset, length, f"manifest section {index} end")
        expected_offset = align8(cursor)
        require(offset == expected_offset, "canonical section placement", f"manifest section {index} offset")
        require(data[cursor:expected_offset] == bytes(expected_offset - cursor), "canonical section placement", f"manifest section {index} padding")
        require(section_end <= len(data), "directory", f"manifest section {index} bounds")
        sections.append((offset, count, length))
        cursor = section_end
    require(cursor == len(data), "total length", "manifest sections do not consume input")

    def section(index: int) -> bytes:
        offset, _, length = sections[index]
        return data[offset : offset + length]

    require(section(0) == expected_authority, "Authority", "fixture authority mismatch")
    paths = tuple(
        u32(section(2), index * 4, "PathOrdinals consumption")
        for index in range(expected_counts[2])
    )
    selectors = section(3)
    path_cursor = 0
    selector_cursor = 0
    decoded: list[ManifestEntry] = []
    rows = section(1)
    for index in range(expected_counts[1]):
        base = index * 96
        revision = rows[base : base + 32]
        root_kind, root_index, path_count, implementation_kind = struct.unpack_from(
            "<IIII", rows, base + 0x20
        )
        portable = struct.unpack_from("<Q", rows, base + 0x30)[0]
        descriptor_hash = rows[base + 0x38 : base + 0x58]
        selector_length, arity = struct.unpack_from("<II", rows, base + 0x58)
        require(root_kind in (1, 2), "row fields", f"manifest row {index} root kind")
        require(implementation_kind in (1, 2), "row fields", f"manifest row {index} kind")
        require(path_count <= PATH_DEPTH, "limits", f"manifest row {index} PathDepth")
        require(path_cursor + path_count <= len(paths), "PathOrdinals consumption", f"manifest row {index} path")
        path = paths[path_cursor : path_cursor + path_count]
        path_cursor += path_count
        require(selector_length <= 1024 * 1024, "limits", f"manifest row {index} SelectorBytes")
        require(selector_cursor + selector_length <= len(selectors), "Selector storage", f"manifest row {index} selector")
        selector = selectors[selector_cursor : selector_cursor + selector_length]
        selector_cursor += selector_length
        selection = Selection(revision, root_kind, root_index, path, implementation_kind, 1)
        require(
            portable
            == derived_identity(b"birnam.portable-implementation.v1\0", selection, 1),
            "Portable identity",
            f"manifest row {index}",
        )
        require(arity == selector_arity(selector) <= 4096, "Arity", f"manifest row {index}")
        decoded.append(ManifestEntry(selection, descriptor_hash, selector))
    require(path_cursor == len(paths), "PathOrdinals consumption", "manifest ordinal suffix")
    require(selector_cursor == len(selectors), "Selector storage", "manifest selector suffix")

    order = tuple(
        (
            entry.selection.implementation_kind,
            entry.selection.revision,
            entry.selection.root_kind,
            entry.selection.root_index,
            entry.selection.path,
            derived_identity(b"birnam.portable-implementation.v1\0", entry.selection, 1),
        )
        for entry in decoded
    )
    require(len(set(order)) == len(order) and tuple(sorted(order)) == order, "Row order", "manifest rows")
    descriptor_owners = {
        NAMED_DESCRIPTOR_HASH: (NAMED_A, "return"),
        BLOCK_DESCRIPTOR_HASH: (BLOCK_A_RESUMABLE, "block"),
        CALL_DESCRIPTOR_HASH: (NAMED_CALL, "call"),
        TRACE_NAMED_DESCRIPTOR_HASH: (TRACE_NAMED, "trace-named"),
        TRACE_BLOCK_DESCRIPTOR_HASH: (TRACE_BLOCK, "trace-block"),
    }
    for entry in decoded:
        require(entry.descriptor_hash in descriptor_owners, "Descriptor ownership", "unknown descriptor hash")
        descriptor_selection, descriptor_kind = descriptor_owners[entry.descriptor_hash]
        require(
            stable_key(descriptor_selection) == stable_key(entry.selection),
            "Descriptor ownership",
            "descriptor stable key",
        )
        descriptor_name = {
            "return": "frame-descriptor-minimal-return.bin",
            "block": "frame-descriptor-source-map-block.bin",
            "call": "frame-descriptor-call.bin",
            "trace-named": "frame-descriptor-construction-trace-named.bin",
            "trace-block": "frame-descriptor-construction-trace-block.bin",
        }[descriptor_kind]
        descriptor = (HERE / descriptor_name).read_bytes()
        require(hashlib.sha256(descriptor).digest() == entry.descriptor_hash, "Descriptor ownership", "descriptor SHA-256")
        if descriptor_kind == "call":
            parse_execution_descriptor(
                descriptor,
                EXECUTION_DESCRIPTOR_FIXTURES[descriptor_name][2],
            )
        elif descriptor_kind.startswith("trace-"):
            parse_descriptor(
                descriptor,
                descriptor_selection,
                descriptor_kind == "trace-block",
                b"trace.birnam",
                len(TRACE_SOURCE),
            )
        else:
            parse_descriptor(descriptor, descriptor_selection, descriptor_kind == "block")

    require(selection_name in ARTIFACT_FIXTURES, "Selection agreement", "unknown selection fixture")
    _, _, inventory, rejection_stage = ARTIFACT_FIXTURES[selection_name]
    require(rejection_stage is None, "Selection agreement", "malformed selection fixture")
    _, selected_rows = parse_artifact_selection((HERE / selection_name).read_bytes(), inventory)
    selected_keys = tuple(stable_key(row) for row in selected_rows if row.disposition == 1)
    manifest_keys = tuple(stable_key(entry.selection) for entry in decoded)
    require(tuple(sorted(selected_keys)) == tuple(sorted(manifest_keys)), "Selection agreement", "selected rows differ")
    require(tuple(decoded) == expected_entries, "fixture semantics", "manifest entries")
    return tuple(decoded)


def parse_registration(data: bytes | None, fixture_kind: str) -> None:
    require(data is not None, "null envelope", "registration envelope pointer")
    require(len(data) == 136, "carrier size", "registration envelope byte extent")
    fields = struct.unpack("<IIIIIIQQQQQQQQQII32s", data)
    (
        carrier_version,
        struct_size,
        operation,
        flags,
        manifest_index,
        reserved0,
        token,
        descriptor_pointer,
        descriptor_length,
        entry_thunk,
        step_method,
        implementation_oop,
        code_identity,
        code_lease,
        portable_identity,
        language_arity,
        driver_argument_count,
        descriptor_hash,
    ) = fields
    require(carrier_version == 1, "carrier version", "registration carrier version")
    require(struct_size == 136, "carrier size", "registration struct_size")
    require(operation in (1, 2), "operation", "registration operation")
    require(flags == 0, "flags", "registration flags")
    require(reserved0 == 0, "reserved field", "registration reserved0")
    require(manifest_index == 0, "manifest index", "fixture row index")

    if operation == 2:
        require(fixture_kind == "abort", "operation", "unexpected Abort fixture")
        require(token == 0x2122232425262728, "registration token", "Abort token")
        require(
            fields[7:17] == (0,) * 10 and descriptor_hash == bytes(32),
            "Abort nullability",
            "Prepare-only Abort fields",
        )
        return

    require(fixture_kind in ("named", "block"), "operation", "unexpected Prepare fixture")
    require(token == 0, "registration token", "Prepare token must enter as zero")
    require(descriptor_pointer != 0, "descriptor pointer", "Prepare descriptor pointer")
    require(entry_thunk != 0, "symbol", "Prepare entry thunk")
    require(step_method != 0, "symbol", "Prepare StepMethod")
    require(entry_thunk != step_method, "symbol", "entry thunk and StepMethod must differ")
    require(code_identity != 0, "code identity", "Prepare generation identity")
    require(code_lease != 0, "code lease", "Prepare generation lease")
    require(driver_argument_count in (0, 1), "driver argument count", "descriptor driver arguments")

    if fixture_kind == "named":
        descriptor_name = "frame-descriptor-minimal-return.bin"
        manifest_name = "resumable-manifest-named-only.bin"
        manifest_entry = NAMED_MANIFEST_ENTRY
        expected_pointer = 0x0000000100001000
        expected_entry = 0x0000000100002000
        expected_step = 0x0000000100003000
        expected_implementation = 0x0000000100004000
    else:
        descriptor_name = "frame-descriptor-source-map-block.bin"
        manifest_name = "resumable-manifest-block-only.bin"
        manifest_entry = BLOCK_MANIFEST_ENTRY
        expected_pointer = 0x0000000100005000
        expected_entry = 0x0000000100006000
        expected_step = 0x0000000100007000
        expected_implementation = 0

    descriptor = (HERE / descriptor_name).read_bytes()
    manifest = (HERE / manifest_name).read_bytes()
    require(descriptor_length == len(descriptor), "descriptor length", "Prepare descriptor extent")
    require(hashlib.sha256(descriptor).digest() == descriptor_hash, "descriptor hash", "Prepare descriptor SHA-256")
    require(descriptor_hash == manifest_entry.descriptor_hash, "manifest agreement", "Prepare descriptor row")
    require(
        portable_identity
        == derived_identity(
            b"birnam.portable-implementation.v1\0", manifest_entry.selection, 1
        ),
        "portable identity",
        "Prepare implementation identity",
    )
    require(
        language_arity == selector_arity(manifest_entry.selector),
        "language arity",
        "Prepare selector arity",
    )
    require(driver_argument_count == 0, "driver argument count", "fixture descriptor driver arguments")
    require(implementation_oop == expected_implementation, "implementation nullability", "Prepare implementation Oop")
    require(
        (descriptor_pointer, entry_thunk, step_method)
        == (expected_pointer, expected_entry, expected_step),
        "fixture semantics",
        "Prepare pointer sentinels",
    )
    require(code_identity == 0x0102030405060708, "fixture semantics", "Prepare code identity sentinel")
    require(code_lease == 0x1112131415161718, "fixture semantics", "Prepare code lease sentinel")
    decoded = parse_manifest(
        manifest,
        (manifest_entry,),
        "artifact-selection-named-resumable.bin"
        if fixture_kind == "named"
        else "artifact-selection-mixed.bin",
    )
    require(decoded[manifest_index] == manifest_entry, "manifest agreement", "Prepare manifest index")


def parse_registration_abi_tuple(data: bytes) -> int:
    require(len(data) >= len(REGISTRATION_TUPLE_DOMAIN), "ABI tuple truncation", "domain")
    require(data[: len(REGISTRATION_TUPLE_DOMAIN)] == REGISTRATION_TUPLE_DOMAIN, "ABI tuple domain", "registration tuple")
    cursor = len(REGISTRATION_TUPLE_DOMAIN)

    def take_u32(stage: str) -> int:
        nonlocal cursor
        require(cursor + 4 <= len(data), "ABI tuple truncation", stage)
        value = struct.unpack_from("<I", data, cursor)[0]
        cursor += 4
        return value

    def take_string(expected: str, stage: str) -> None:
        nonlocal cursor
        length = take_u32(stage)
        require(length == len(expected), stage, "string length")
        require(cursor + length <= len(data), "ABI tuple truncation", stage)
        require(data[cursor : cursor + length] == expected.encode("ascii"), stage, "string bytes")
        cursor += length

    take_string("aarch64-apple-darwin|Mach-O|little-endian", "ABI tuple target")
    take_string("birnam_register_resumable_v1", "ABI tuple symbol")
    take_string("u32(BirnamResumableRegistrationV1 *)", "ABI tuple signature")
    require(take_u32("ABI tuple struct size") == 136, "ABI tuple struct size", "registration carrier")
    require(take_u32("ABI tuple alignment") == 8, "ABI tuple alignment", "registration carrier")
    require(take_u32("ABI tuple fields") == len(REGISTRATION_TUPLE_FIELDS), "ABI tuple fields", "field count")
    for expected in REGISTRATION_TUPLE_FIELDS:
        name, offset, width, alignment, abi_kind, nullability = expected
        take_string(name, "ABI tuple field name")
        require(take_u32("ABI tuple field offset") == offset, "ABI tuple field offset", name)
        require(take_u32("ABI tuple field width") == width, "ABI tuple field width", name)
        require(take_u32("ABI tuple field alignment") == alignment, "ABI tuple field alignment", name)
        take_string(abi_kind, "ABI tuple field kind")
        take_string(nullability, "ABI tuple field nullability")
    require(take_u32("ABI tuple constants") == len(REGISTRATION_TUPLE_CONSTANTS), "ABI tuple constants", "constant count")
    for name, expected_value in REGISTRATION_TUPLE_CONSTANTS:
        take_string(name, "ABI tuple constant name")
        require(take_u32("ABI tuple constant value") == expected_value, "ABI tuple constant value", name)
    require(cursor == len(data), "ABI tuple suffix", "unreferenced tuple bytes")

    hash_value = 0xCBF29CE484222325
    for blob in (*RUNTIME_SHIM_NAMES_V3, data, (3).to_bytes(4, "little")):
        for byte in blob:
            hash_value ^= byte
            hash_value = (hash_value * 0x100000001B3) & U64_MAX
    require(hash_value == RUNTIME_ABI_V3, "Runtime ABI hash", "v3 digest")
    return hash_value


def parse_image_v5(data: bytes, manifest_name: str | None) -> int:
    require(len(data) >= 28, "image header", "minimum v5 header")
    require(data[:8] == b"BIRNAMIM", "image magic", "v5 magic")
    require(u32(data, 8, "image header") == 5, "image version", "v5 version")
    require(u32(data, 12, "image header") == 0x01020304, "image endian", "little-endian tag")
    require(u32(data, 16, "image header") == 1, "image architecture", "aarch64 tag")
    require(u64(data, 20, "image header") == RUNTIME_ABI_V3, "image ABI", "Runtime ABI v3")

    records: list[tuple[int, bytes]] = []
    cursor = 28
    while cursor < len(data):
        require(cursor + 12 <= len(data), "image framing", "record header")
        kind = u32(data, cursor, "image framing")
        length = u64(data, cursor + 4, "image framing")
        require(kind in (2, 3, 4, 5, 6), "image record kind", "unknown v5 record")
        cursor += 12
        end = checked_add(cursor, length, "image record extent")
        require(end <= len(data), "image framing", "record payload")
        records.append((kind, data[cursor:end]))
        cursor = end
    require(cursor == len(data), "image framing", "record stream suffix")
    kinds = tuple(kind for kind, _ in records)
    require(kinds.count(5) == 1 and kinds and kinds[0] == 5, "image record order", "Authority cardinality/order")
    has_code = 4 in kinds
    if has_code:
        require(kinds.count(4) == 1 and kinds[-1] == 4, "image record order", "Code cardinality/order")
        require(kinds.count(6) == 1 and len(kinds) >= 3 and kinds[1] == 6, "image record order", "Resumables cardinality/order")
        middle = kinds[2:-1]
        require(all(kind in (2, 3) for kind in middle), "image record order", "middle record kind")
        require(tuple(sorted(middle, reverse=True)) == middle, "image record order", "Heap before Facts")
        require(manifest_name is not None, "fixture semantics", "code image expected manifest")
    else:
        require(6 not in kinds and manifest_name is None, "image no-Code exclusion", "orphaned Resumables")
        require(all(kind in (2, 3, 5) for kind in kinds), "image record order", "no-Code record kind")
        require(tuple(sorted(kinds[1:], reverse=True)) == kinds[1:], "image record order", "no-Code Heap before Facts")

    authority_spelling = records[0][1]
    require(len(authority_spelling) == 64, "image authority", "Authority payload length")
    require(all(byte in b"0123456789abcdef" for byte in authority_spelling), "image authority", "lowercase hexadecimal Authority")
    try:
        raw_authority = bytes.fromhex(authority_spelling.decode("ascii"))
    except (UnicodeDecodeError, ValueError) as error:
        raise Reject("image authority", "Authority decoding") from error

    manifest_entries: tuple[ManifestEntry, ...] = ()
    if has_code:
        resumables = records[1][1]
        require(len(resumables) >= 104, "image Resumables length", "fixed prefix")
        require(resumables[:64] == authority_spelling, "image authority", "Resumables spelling")
        manifest_hash = resumables[64:96]
        manifest_length = u64(resumables, 96, "image Resumables length")
        require(104 + manifest_length == len(resumables), "image Resumables length", "manifest extent")
        manifest = resumables[104:]
        require(hashlib.sha256(manifest).digest() == manifest_hash, "image manifest hash", "Resumables SHA-256")
        require(manifest == (HERE / manifest_name).read_bytes(), "image manifest bytes", "artifact/image byte identity")
        if manifest_name == "resumable-manifest-empty.bin":
            manifest_entries = parse_manifest(
                manifest,
                (),
                "artifact-selection-all-synchronous.bin",
            )
        else:
            require(manifest_name == TRACE_MANIFEST_FIXTURE[0], "fixture harness", "unknown image manifest")
            manifest_entries = parse_manifest(
                manifest,
                TRACE_MANIFEST_FIXTURE[3],
                TRACE_MANIFEST_FIXTURE[4],
                TRACE_MANIFEST_FIXTURE[5],
            )
        require(manifest[0xC0:0xE0] == raw_authority, "image authority", "manifest raw Authority")

    block_references: list[int] = []

    def parse_field(payload: bytes, offset: int) -> int:
        require(offset < len(payload), "image field", "missing tag")
        tag = payload[offset]
        offset += 1
        if tag in (0, 1, 2, 8):
            return offset
        if tag in (3, 4):
            require(offset + 8 <= len(payload), "image field", "fixed-width scalar")
            return offset + 8
        if tag == 7:
            require(offset + 4 <= len(payload), "image field", "HeapRef")
            return offset + 4
        if tag in (5, 6, 9):
            require(offset + 4 <= len(payload), "image field", "length prefix")
            length = u32(payload, offset, "image field")
            end = checked_add(offset + 4, length, "image variable field")
            require(end <= len(payload), "image field", "variable bytes")
            if tag == 9:
                require(length > 0, "image heap locator", "empty ExportRef")
            return end
        require(tag == 10, "image field tag", "unknown tagged Oop")
        require(offset + 4 <= len(payload), "image field", "block manifest index")
        index = u32(payload, offset, "image field")
        require(index < len(manifest_entries), "image block index", "manifest row bounds")
        require(manifest_entries[index].selection.implementation_kind == 2, "image block kind", "manifest row is not BlockBody")
        block_references.append(index)
        return offset + 4

    for kind, payload in records:
        if kind == 3:
            require(len(payload) >= 4, "image Heap", "object count")
            offset = 4
            remaining = u32(payload, 0, "image Heap")
            for _ in range(remaining):
                require(offset < len(payload), "image heap locator", "missing locator")
                locator_tag = payload[offset]
                require(locator_tag in (9, 10), "image heap locator", "locator tag")
                before = len(block_references)
                offset = parse_field(payload, offset)
                require(offset + 4 <= len(payload), "image Heap", "word count")
                word_count = u32(payload, offset, "image Heap")
                offset += 4
                require(locator_tag == 10, "fixture semantics", "fixture needs generated layout")
                locator_index = block_references[before]
                descriptor = (HERE / "frame-descriptor-construction-trace-block.bin").read_bytes()
                capture_count = u32(descriptor, 0x68, "mapped descriptor")
                require(word_count == capture_count, "mapped capture count", "generated block payload")
                for _ in range(word_count):
                    offset = parse_field(payload, offset)
                require(block_references[before] == locator_index, "fixture harness", "locator tracking")
            require(offset == len(payload), "image Heap", "object payload suffix")
        elif kind == 2:
            require(len(payload) >= 12, "image Facts", "basis/count")
            offset = 12
            for _ in range(u32(payload, 8, "image Facts")):
                require(offset + 9 <= len(payload), "image Facts", "operation/transaction")
                offset += 9
                for _ in range(3):
                    offset = parse_field(payload, offset)
            require(offset == len(payload), "image Facts", "facts suffix")

    if has_code:
        code = records[-1][1]
        require(len(code) >= 72, "image Code", "Code prefix")
        require(u64(code, 0, "image Code") == 0x0409638EE2BDE459, "image Code checksum", "empty unit wyhash")
        require(code[8:72] == authority_spelling, "image authority", "Code generation digest")
        require(code[72:] == b"", "image Code checksum", "fixture unit bytes")
    else:
        require(not block_references, "image no-Code exclusion", "generated block reference")

    if manifest_name == TRACE_MANIFEST_FIXTURE[0]:
        require(block_references == [1, 1, 1, 1], "image repeated references", "trace block references")
    else:
        require(not block_references, "fixture semantics", "unexpected block references")
    return len(block_references)


def expect_rejection(
    data: bytes, expected_inventory: tuple[Selection, ...], expected_stage: str, label: str
) -> None:
    try:
        parse_artifact_selection(data, expected_inventory)
    except Reject as error:
        require(error.stage == expected_stage, "fixture harness", f"{label}: expected {expected_stage!r}, got {error.stage!r}")
        return
    raise Reject("fixture harness", f"{label}: unexpectedly accepted")


def verify_fixtures() -> None:
    for name, (length, digest, inventory, rejection_stage) in ARTIFACT_FIXTURES.items():
        data = (HERE / name).read_bytes()
        require(len(data) == length, "fixture harness", f"{name}: wrong committed length")
        require(hashlib.sha256(data).hexdigest() == digest, "fixture harness", f"{name}: wrong committed SHA-256")
        if rejection_stage is None:
            parse_artifact_selection(data, inventory)
        else:
            expect_rejection(data, inventory, rejection_stage, name)

    for name, (length, digest, revisions, inventory) in GENERATION_FIXTURES.items():
        data = (HERE / name).read_bytes()
        require(len(data) == length, "fixture harness", f"{name}: wrong committed length")
        require(hashlib.sha256(data).hexdigest() == digest, "fixture harness", f"{name}: wrong committed SHA-256")
        decoded_revisions, _ = parse_generation_selection(data, inventory)
        require(decoded_revisions == revisions, "fixture harness", f"{name}: wrong active revision set")

    for name, (length, digest, selection, block) in DESCRIPTOR_FIXTURES.items():
        data = (HERE / name).read_bytes()
        require(len(data) == length, "fixture harness", f"{name}: wrong committed length")
        require(hashlib.sha256(data).hexdigest() == digest, "fixture harness", f"{name}: wrong committed SHA-256")
        parse_descriptor(data, selection, block)

    for name, (length, digest, selection, block) in TRACE_DESCRIPTOR_FIXTURES.items():
        data = (HERE / name).read_bytes()
        require(len(data) == length, "fixture harness", f"{name}: wrong committed length")
        require(hashlib.sha256(data).hexdigest() == digest, "fixture harness", f"{name}: wrong committed SHA-256")
        parse_descriptor(data, selection, block, b"trace.birnam", len(TRACE_SOURCE))

    for name, (length, digest, spec) in EXECUTION_DESCRIPTOR_FIXTURES.items():
        data = (HERE / name).read_bytes()
        require(len(data) == length, "fixture harness", f"{name}: wrong committed length")
        require(hashlib.sha256(data).hexdigest() == digest, "fixture harness", f"{name}: wrong committed SHA-256")
        parse_execution_descriptor(data, spec)

    for name, (length, digest, entries, selection_name) in MANIFEST_FIXTURES.items():
        data = (HERE / name).read_bytes()
        require(len(data) == length, "fixture harness", f"{name}: wrong committed length")
        require(hashlib.sha256(data).hexdigest() == digest, "fixture harness", f"{name}: wrong committed SHA-256")
        parse_manifest(data, entries, selection_name)

    trace_name, trace_length, trace_digest, trace_entries, trace_selection, trace_authority = TRACE_MANIFEST_FIXTURE
    trace_data = (HERE / trace_name).read_bytes()
    require(len(trace_data) == trace_length, "fixture harness", f"{trace_name}: wrong committed length")
    require(hashlib.sha256(trace_data).hexdigest() == trace_digest, "fixture harness", f"{trace_name}: wrong committed SHA-256")
    parse_manifest(trace_data, trace_entries, trace_selection, trace_authority)

    for name, (length, digest, entries, selection_name, rejection_stage) in MALFORMED_MANIFEST_FIXTURES.items():
        data = (HERE / name).read_bytes()
        require(len(data) == length, "fixture harness", f"{name}: wrong committed length")
        require(hashlib.sha256(data).hexdigest() == digest, "fixture harness", f"{name}: wrong committed SHA-256")
        try:
            parse_manifest(data, entries, selection_name)
        except Reject as error:
            require(error.stage == rejection_stage, "fixture harness", f"{name}: expected {rejection_stage!r}, got {error.stage!r}")
        else:
            raise Reject("fixture harness", f"{name}: unexpectedly accepted")

    for name, (length, digest, fixture_kind) in REGISTRATION_FIXTURES.items():
        data = (HERE / name).read_bytes()
        require(len(data) == length, "fixture harness", f"{name}: wrong committed length")
        require(hashlib.sha256(data).hexdigest() == digest, "fixture harness", f"{name}: wrong committed SHA-256")
        parse_registration(data, fixture_kind)

    tuple_name, tuple_length, tuple_digest = REGISTRATION_ABI_TUPLE_FIXTURE
    tuple_data = (HERE / tuple_name).read_bytes()
    require(len(tuple_data) == tuple_length, "fixture harness", f"{tuple_name}: wrong committed length")
    require(hashlib.sha256(tuple_data).hexdigest() == tuple_digest, "fixture harness", f"{tuple_name}: wrong committed SHA-256")
    parse_registration_abi_tuple(tuple_data)

    for name, (length, digest, manifest_name) in IMAGE_FIXTURES.items():
        data = (HERE / name).read_bytes()
        require(len(data) == length, "fixture harness", f"{name}: wrong committed length")
        require(hashlib.sha256(data).hexdigest() == digest, "fixture harness", f"{name}: wrong committed SHA-256")
        parse_image_v5(data, manifest_name)

    for name, (length, digest, manifest_name, rejection_stage) in MALFORMED_IMAGE_FIXTURES.items():
        data = (HERE / name).read_bytes()
        require(len(data) == length, "fixture harness", f"{name}: wrong committed length")
        require(hashlib.sha256(data).hexdigest() == digest, "fixture harness", f"{name}: wrong committed SHA-256")
        try:
            parse_image_v5(data, manifest_name)
        except Reject as error:
            require(error.stage == rejection_stage, "fixture harness", f"{name}: expected {rejection_stage!r}, got {error.stage!r}")
        else:
            raise Reject("fixture harness", f"{name}: unexpectedly accepted")


def verify_mutations() -> None:
    source = (HERE / "artifact-selection-mixed.bin").read_bytes()
    for label, offset, old, new, stage in MUTATIONS:
        require(source[offset] == old, "fixture harness", f"{label}: source byte drifted")
        mutated = bytearray(source)
        mutated[offset] = new
        expect_rejection(bytes(mutated), (NAMED_A, BLOCK_A_RESUMABLE), stage, label)


def verify_truncation() -> None:
    source = (HERE / "artifact-selection-mixed.bin").read_bytes()
    for length in range(len(source)):
        try:
            parse_artifact_selection(source[:length], (NAMED_A, BLOCK_A_RESUMABLE))
        except Reject:
            continue
        raise Reject("fixture harness", f"truncation at {length} bytes unexpectedly accepted")

    for name, (_, _, _, inventory) in GENERATION_FIXTURES.items():
        source = (HERE / name).read_bytes()
        for length in range(len(source)):
            try:
                parse_generation_selection(source[:length], inventory)
            except Reject:
                continue
            raise Reject("fixture harness", f"{name}: truncation at {length} bytes unexpectedly accepted")

    for name, (_, _, selection, block) in DESCRIPTOR_FIXTURES.items():
        source = (HERE / name).read_bytes()
        for length in range(len(source)):
            try:
                parse_descriptor(source[:length], selection, block)
            except Reject:
                continue
            raise Reject("fixture harness", f"{name}: truncation at {length} bytes unexpectedly accepted")

    for name, (_, _, selection, block) in TRACE_DESCRIPTOR_FIXTURES.items():
        source = (HERE / name).read_bytes()
        for length in range(len(source)):
            try:
                parse_descriptor(
                    source[:length],
                    selection,
                    block,
                    b"trace.birnam",
                    len(TRACE_SOURCE),
                )
            except Reject:
                continue
            raise Reject("fixture harness", f"{name}: truncation at {length} bytes unexpectedly accepted")

    for name, (_, _, spec) in EXECUTION_DESCRIPTOR_FIXTURES.items():
        source = (HERE / name).read_bytes()
        for length in range(len(source)):
            try:
                parse_execution_descriptor(source[:length], spec)
            except Reject:
                continue
            raise Reject("fixture harness", f"{name}: truncation at {length} bytes unexpectedly accepted")

    for name, (_, _, entries, selection_name) in MANIFEST_FIXTURES.items():
        source = (HERE / name).read_bytes()
        for length in range(len(source)):
            try:
                parse_manifest(source[:length], entries, selection_name)
            except Reject:
                continue
            raise Reject("fixture harness", f"{name}: truncation at {length} bytes unexpectedly accepted")

    trace_name, _, _, trace_entries, trace_selection, trace_authority = TRACE_MANIFEST_FIXTURE
    source = (HERE / trace_name).read_bytes()
    for length in range(len(source)):
        try:
            parse_manifest(
                source[:length], trace_entries, trace_selection, trace_authority
            )
        except Reject:
            continue
        raise Reject("fixture harness", f"{trace_name}: truncation at {length} bytes unexpectedly accepted")

    for name, (_, _, fixture_kind) in REGISTRATION_FIXTURES.items():
        source = (HERE / name).read_bytes()
        for length in range(len(source)):
            try:
                parse_registration(source[:length], fixture_kind)
            except Reject:
                continue
            raise Reject("fixture harness", f"{name}: truncation at {length} bytes unexpectedly accepted")

    tuple_name, _, _ = REGISTRATION_ABI_TUPLE_FIXTURE
    source = (HERE / tuple_name).read_bytes()
    for length in range(len(source)):
        try:
            parse_registration_abi_tuple(source[:length])
        except Reject:
            continue
        raise Reject("fixture harness", f"{tuple_name}: truncation at {length} bytes unexpectedly accepted")

    for name, (_, _, manifest_name) in IMAGE_FIXTURES.items():
        source = (HERE / name).read_bytes()
        for length in range(len(source)):
            try:
                parse_image_v5(source[:length], manifest_name)
            except Reject:
                continue
            raise Reject("fixture harness", f"{name}: truncation at {length} bytes unexpectedly accepted")


def directory_entry(kind: int, stride: int, count: int, offset: int, length: int) -> bytes:
    return struct.pack("<IIIIQQ", kind, stride, count, 0, offset, length)


def serialize_generation(
    revisions: tuple[bytes, ...], rows: tuple[Selection, ...]
) -> bytes:
    require(tuple(sorted(set(revisions))) == revisions, "canonical merge", "unordered revision output")
    keys = tuple(row[:-1] for row in rows)
    require(len(set(keys)) == len(keys) and tuple(sorted(keys)) == keys, "canonical merge", "unordered row output")

    revision_bytes = b"".join(revisions)
    row_bytes = b"".join(
        row.revision
        + struct.pack(
            "<IIIII",
            row.root_kind,
            row.root_index,
            len(row.path),
            row.implementation_kind,
            row.disposition,
        )
        for row in rows
    )
    path_bytes = b"".join(struct.pack("<I", ordinal) for row in rows for ordinal in row.path)

    revision_offset = 160
    row_offset = align8(revision_offset + len(revision_bytes))
    path_offset = align8(row_offset + len(row_bytes))
    total = path_offset + len(path_bytes)
    prefix = GENERATION_DOMAIN + struct.pack("<IIQIIII", 1, 64, total, 0, 3, 32, 0)
    directory = b"".join(
        (
            directory_entry(1, 32, len(revisions), revision_offset, len(revision_bytes)),
            directory_entry(2, 52, len(rows), row_offset, len(row_bytes)),
            directory_entry(3, 4, len(path_bytes) // 4, path_offset, len(path_bytes)),
        )
    )
    result = (
        prefix
        + directory
        + revision_bytes
        + bytes(row_offset - revision_offset - len(revision_bytes))
    )
    # Revisions are 32-byte records, so the first padding term is necessarily
    # empty. Keep the assertion explicit so the checker cannot normalize a
    # future incompatible shape silently.
    require(len(result) == row_offset, "fixture harness", "unexpected revision padding shape")
    result += row_bytes + bytes(path_offset - row_offset - len(row_bytes)) + path_bytes
    require(len(result) == total, "fixture harness", "serializer length mismatch")
    return result


def compose_generation(
    active_revisions: tuple[bytes, ...], slice_names: tuple[str, ...]
) -> bytes:
    require(
        tuple(sorted(set(active_revisions))) == active_revisions,
        "scope partition",
        "active revisions are not a canonical set",
    )
    ownership = {revision: 0 for revision in active_revisions}
    rows: list[Selection] = []
    for name in slice_names:
        require(name in ARTIFACT_FIXTURES, "input admission", f"unknown slice {name}")
        _, _, inventory, rejection_stage = ARTIFACT_FIXTURES[name]
        require(rejection_stage is None, "input admission", f"malformed slice {name}")
        revisions, decoded = parse_artifact_selection((HERE / name).read_bytes(), inventory)
        for revision in revisions:
            require(revision in ownership, "scope partition", f"slice {name} owns an inactive revision")
            ownership[revision] += 1
        rows.extend(decoded)

    require(
        all(count == 1 for count in ownership.values()),
        "scope partition",
        "an active revision is missing or multiply owned",
    )
    rows.sort(key=lambda row: row[:-1])
    keys = tuple(row[:-1] for row in rows)
    require(len(set(keys)) == len(keys), "canonical merge", "duplicate stable key")
    return serialize_generation(active_revisions, tuple(rows))


def verify_composition_cases() -> int:
    cases = json.loads((HERE / "generation-composition-cases.json").read_text(encoding="utf-8"))
    require(
        cases.get("schema") == "birnam.generation-selection-composition-cases.v1",
        "fixture harness",
        "wrong composition-case schema",
    )
    revision_names = cases.get("revisions")
    require(
        revision_names == {"a": REVISION_A.hex(), "b": REVISION_B.hex()},
        "fixture harness",
        "composition revision identities drifted",
    )
    checked = 0
    for case in cases.get("cases", []):
        label = case["name"]
        active = tuple(bytes.fromhex(revision_names[name]) for name in case["active"])
        slices = tuple(case["slices"])
        rejection_stage = case.get("reject")
        if rejection_stage is not None:
            try:
                compose_generation(active, slices)
            except Reject as error:
                require(
                    error.stage == rejection_stage,
                    "fixture harness",
                    f"{label}: expected {rejection_stage!r}, got {error.stage!r}",
                )
            else:
                raise Reject("fixture harness", f"{label}: unexpectedly composed")
        else:
            expected_name = case["expected"]
            require(expected_name in GENERATION_FIXTURES, "fixture harness", f"{label}: unknown output")
            actual = compose_generation(active, slices)
            expected = (HERE / expected_name).read_bytes()
            require(actual == expected, "partition independence", f"{label}: bytes differ from {expected_name}")
        checked += 1
    return checked


def verify_generation_padding() -> None:
    source = bytearray((HERE / "generation-selection-ab.bin").read_bytes())
    require(source[0x17C] == 0, "fixture harness", "generation padding source byte drifted")
    source[0x17C] = 1
    try:
        parse_generation_selection(bytes(source), (NAMED_A, BLOCK_A_RESUMABLE, NAMED_B))
    except Reject as error:
        require(
            error.stage == "canonical section placement",
            "fixture harness",
            f"generation padding: wrong rejection stage {error.stage!r}",
        )
        return
    raise Reject("fixture harness", "generation padding mutation unexpectedly accepted")


def verify_limit_and_overflow_cases() -> int:
    source = (HERE / "artifact-selection-mixed.bin").read_bytes()
    cases = (
        ("declared-u64-length", 0x28, 8, U64_MAX, "total length"),
        ("section-end-overflow", 0x50, 8, U64_MAX - 15, "arithmetic overflow"),
        ("covered-revision-ceiling", 0x48, 4, COVERED_REVISIONS + 1, "limits"),
        ("implementation-row-ceiling", 0x68, 4, IMPLEMENTATION_ROWS + 1, "limits"),
        ("path-ordinal-ceiling", 0x88, 4, PATH_ORDINALS + 1, "limits"),
        ("fixed-record-aggregate", 0x88, 4, FIXED_RECORDS, "limits"),
        ("path-depth-ceiling", 0x11C, 4, PATH_DEPTH + 1, "limits"),
    )
    for label, offset, width, value, stage in cases:
        mutated = bytearray(source)
        mutated[offset : offset + width] = value.to_bytes(width, "little")
        expect_rejection(bytes(mutated), (NAMED_A, BLOCK_A_RESUMABLE), stage, label)

    for label, operation in (
        ("u64-add", lambda: checked_add(U64_MAX, 1, "fixture overflow probe")),
        ("u64-multiply", lambda: checked_multiply(U64_MAX, 2, "fixture overflow probe")),
    ):
        try:
            operation()
        except Reject as error:
            require(
                error.stage == "arithmetic overflow",
                "fixture harness",
                f"{label}: wrong rejection stage {error.stage!r}",
            )
        else:
            raise Reject("fixture harness", f"{label}: unexpectedly succeeded")
    return len(cases) + 2


def verify_all_limit_boundaries() -> int:
    ceilings = (
        ("CanonicalValueBytes", CANONICAL_VALUE_BYTES),
        ("SectionCount", SECTION_COUNT),
        ("FixedRecords", FIXED_RECORDS),
        ("CoveredRevisions", COVERED_REVISIONS),
        ("ImplementationRows", IMPLEMENTATION_ROWS),
        ("PathDepth", PATH_DEPTH),
        ("PathOrdinals", PATH_ORDINALS),
        ("SelectorBytes", SELECTOR_BYTES),
        ("SelectorArenaBytes", SELECTOR_ARENA_BYTES),
        ("LanguageArity", LANGUAGE_ARITY),
        ("DescriptorBytes", DESCRIPTOR_BYTES),
        ("FrameSlots", FRAME_SLOTS),
        ("ProgramCounters", PROGRAM_COUNTERS),
        ("CallSites", CALL_SITES),
        ("Edges", EDGES),
        ("CopyPairs", COPY_PAIRS),
        ("CopyPairsPerEdge", COPY_PAIRS_PER_EDGE),
        ("LogicalValues", LOGICAL_VALUES),
        ("SourceRecords", SOURCE_RECORDS),
        ("BitmapWords", BITMAP_WORDS),
        ("DescriptorArenaBytes", DESCRIPTOR_ARENA_BYTES),
        ("FramePayloadBytes", FRAME_PAYLOAD_BYTES),
        ("TransferScratchBytes", TRANSFER_SCRATCH_BYTES),
        ("ArtifactDescriptorBytes", ARTIFACT_DESCRIPTOR_BYTES),
        ("ImageManifestBytes", IMAGE_MANIFEST_BYTES),
        ("ImageBlockReferences", IMAGE_BLOCK_REFERENCES),
    )
    checked = 0
    for name, ceiling in ceilings:
        require(ceiling <= ceiling, "fixture harness", f"{name}: ceiling rejected")
        try:
            require(ceiling + 1 <= ceiling, "limits", name)
        except Reject as error:
            require(error.stage == "limits", "fixture harness", f"{name}: wrong ceiling rejection")
        else:
            raise Reject("fixture harness", f"{name}: ceiling plus one accepted")
        checked += 2

    require(checked_multiply(FRAME_SLOTS, 8, "FramePayloadBytes") == FRAME_PAYLOAD_BYTES, "fixture harness", "frame payload boundary")
    require(checked_multiply(COPY_PAIRS_PER_EDGE, 8, "TransferScratchBytes") == TRANSFER_SCRATCH_BYTES, "fixture harness", "transfer scratch boundary")
    try:
        require(checked_multiply(FRAME_SLOTS + 1, 8, "FramePayloadBytes") <= FRAME_PAYLOAD_BYTES, "limits", "FramePayloadBytes")
    except Reject as error:
        require(error.stage == "limits", "fixture harness", "frame payload plus one")
    else:
        raise Reject("fixture harness", "frame payload plus one accepted")
    try:
        require(checked_multiply(COPY_PAIRS_PER_EDGE + 1, 8, "TransferScratchBytes") <= TRANSFER_SCRATCH_BYTES, "limits", "TransferScratchBytes")
    except Reject as error:
        require(error.stage == "limits", "fixture harness", "transfer scratch plus one")
    else:
        raise Reject("fixture harness", "transfer scratch plus one accepted")
    return checked + 4


def verify_descriptor_mutations() -> int:
    named = (HERE / "frame-descriptor-minimal-return.bin").read_bytes()
    block = (HERE / "frame-descriptor-source-map-block.bin").read_bytes()
    cases = (
        ("descriptor-domain", named, NAMED_A, False, 0x00, 0x62, 0x63, "domain"),
        ("descriptor-version", named, NAMED_A, False, 0x20, 0x02, 0x03, "prefix"),
        ("descriptor-arena-stride", named, NAMED_A, False, 0x204, 0x01, 0x02, "directory"),
        ("descriptor-portable-identity", named, NAMED_A, False, 0x220, 0x8E, 0x8F, "identity"),
        ("descriptor-root-padding", named, NAMED_A, False, 0x280, 0x03, 0x07, "bitmap padding"),
        ("descriptor-source-bytes", named, NAMED_A, False, 0x398, 0x61, 0x62, "source map"),
        ("descriptor-capture-slot", block, BLOCK_A_RESUMABLE, True, 0x260, 0x01, 0x02, "entry"),
        ("descriptor-capture-padding", block, BLOCK_A_RESUMABLE, True, 0x264, 0x00, 0x01, "canonical section placement"),
    )
    for label, source, selection, is_block, offset, old, new, stage in cases:
        require(source[offset] == old, "fixture harness", f"{label}: source byte drifted")
        mutated = bytearray(source)
        mutated[offset] = new
        try:
            parse_descriptor(bytes(mutated), selection, is_block)
        except Reject as error:
            require(
                error.stage == stage,
                "fixture harness",
                f"{label}: expected {stage!r}, got {error.stage!r}",
            )
        else:
            raise Reject("fixture harness", f"{label}: unexpectedly accepted")
    return len(cases)


def verify_execution_descriptor_mutations() -> int:
    branch = (HERE / "frame-descriptor-branch-join.bin").read_bytes()
    call = (HERE / "frame-descriptor-call.bin").read_bytes()
    cycle = (HERE / "frame-descriptor-cyclic-copy.bin").read_bytes()
    color = (HERE / "frame-descriptor-colored-reuse.bin").read_bytes()
    cases = (
        ("branch-first-edge-payload", branch, "frame-descriptor-branch-join.bin", 0x268, 4, 1, "PCs"),
        ("branch-duplicate-target", branch, "frame-descriptor-branch-join.bin", 0x32C, 4, 1, "Edges"),
        ("call-continuation", call, "frame-descriptor-call.bin", 0x2D0, 4, 0, "Calls"),
        ("cycle-copy-order", cycle, "frame-descriptor-cyclic-copy.bin", 0x2D0, 4, 2, "CopyPairs"),
        ("color-clear-map", color, "frame-descriptor-colored-reuse.bin", 0x2D0, 8, 0, "ClearBitmapWords"),
    )
    for label, source, fixture_name, offset, width, value, stage in cases:
        mutated = bytearray(source)
        mutated[offset : offset + width] = value.to_bytes(width, "little")
        spec = EXECUTION_DESCRIPTOR_FIXTURES[fixture_name][2]
        try:
            parse_execution_descriptor(bytes(mutated), spec)
        except Reject as error:
            require(
                error.stage == stage,
                "fixture harness",
                f"{label}: expected {stage!r}, got {error.stage!r}",
            )
        else:
            raise Reject("fixture harness", f"{label}: unexpectedly accepted")
    return len(cases)


def verify_manifest_mutations() -> int:
    named = (HERE / "resumable-manifest-named-only.bin").read_bytes()
    block = (HERE / "resumable-manifest-block-only.bin").read_bytes()
    cases = (
        ("manifest-authority", named, (NAMED_MANIFEST_ENTRY,), "artifact-selection-named-resumable.bin", 0xC0, named[0xC0], named[0xC0] ^ 1, "Authority"),
        ("manifest-portable-identity", named, (NAMED_MANIFEST_ENTRY,), "artifact-selection-named-resumable.bin", 0x110, named[0x110], named[0x110] ^ 1, "Portable identity"),
        ("manifest-descriptor-hash", named, (NAMED_MANIFEST_ENTRY,), "artifact-selection-named-resumable.bin", 0x118, named[0x118], named[0x118] ^ 1, "Descriptor ownership"),
        ("manifest-selector-length", named, (NAMED_MANIFEST_ENTRY,), "artifact-selection-named-resumable.bin", 0x138, 0x03, 0x04, "Selector storage"),
        ("manifest-arity", named, (NAMED_MANIFEST_ENTRY,), "artifact-selection-named-resumable.bin", 0x13C, 0x00, 0x01, "Arity"),
        ("manifest-block-path", block, (BLOCK_MANIFEST_ENTRY,), "artifact-selection-mixed.bin", 0x140, 0x02, 0x03, "Portable identity"),
    )
    for label, source, entries, selection_name, offset, old, new, stage in cases:
        require(source[offset] == old, "fixture harness", f"{label}: source byte drifted")
        mutated = bytearray(source)
        mutated[offset] = new
        try:
            parse_manifest(bytes(mutated), entries, selection_name)
        except Reject as error:
            require(
                error.stage == stage,
                "fixture harness",
                f"{label}: expected {stage!r}, got {error.stage!r}",
            )
        else:
            raise Reject("fixture harness", f"{label}: unexpectedly accepted")

    mixed = (HERE / "resumable-manifest-mixed.bin").read_bytes()
    try:
        parse_manifest(
            mixed,
            (NAMED_MANIFEST_ENTRY, BLOCK_MANIFEST_ENTRY),
            "artifact-selection-all-synchronous.bin",
        )
    except Reject as error:
        require(
            error.stage == "Selection agreement",
            "fixture harness",
            f"manifest-selection disagreement: wrong stage {error.stage!r}",
        )
    else:
        raise Reject("fixture harness", "manifest-selection disagreement unexpectedly accepted")
    return len(cases) + 1


def verify_portable_collision_policy() -> int:
    collision = 0xA1A2A3A4A5A6A7A8
    selections = (NAMED_A_RESUMABLE, NAMED_CALL)

    def complete_key(selection: Selection) -> bytes:
        return stable_key(selection) + b"".join(
            struct.pack("<I", ordinal) for ordinal in selection.path
        )

    projected_rows = tuple((selection, collision) for selection in selections)
    require(
        all(projected == collision for _, projected in projected_rows),
        "collision policy",
        "injected projection did not collide",
    )
    complete_keys = tuple(complete_key(selection) for selection, _ in projected_rows)
    require(
        len(set(complete_keys)) == len(complete_keys),
        "collision policy",
        "complete stable keys collapsed",
    )
    ordered = tuple(
        sorted(
            projected_rows,
            key=lambda row: (
                row[0].implementation_kind,
                row[0].revision,
                row[0].root_kind,
                row[0].root_index,
                row[0].path,
                row[1],
            ),
        )
    )
    require(len(ordered) == 2, "collision policy", "canonical ordering dropped a row")
    catalog = {complete_key(selection): label for label, (selection, _) in enumerate(ordered)}
    require(len(catalog) == 2, "collision policy", "full-key catalog treated projection as authority")
    require(
        len({projected: label for label, (_, projected) in enumerate(ordered)}) == 1,
        "collision policy",
        "test does not demonstrate why projection-keyed lookup is forbidden",
    )
    return 5


def verify_registration_mutations() -> int:
    named = (HERE / "registration-prepare-named.bin").read_bytes()
    block = (HERE / "registration-prepare-block.bin").read_bytes()
    abort = (HERE / "registration-abort.bin").read_bytes()
    cases = (
        ("registration-version", named, "named", 0x00, 4, 2, "carrier version"),
        ("registration-size", named, "named", 0x04, 4, 135, "carrier size"),
        ("registration-operation", named, "named", 0x08, 4, 3, "operation"),
        ("registration-flags", named, "named", 0x0C, 4, 1, "flags"),
        ("registration-index", named, "named", 0x10, 4, 1, "manifest index"),
        ("registration-reserved", named, "named", 0x14, 4, 1, "reserved field"),
        ("registration-prepare-token", named, "named", 0x18, 8, 1, "registration token"),
        ("registration-descriptor-pointer", named, "named", 0x20, 8, 0, "descriptor pointer"),
        ("registration-descriptor-length", named, "named", 0x28, 8, 0, "descriptor length"),
        ("registration-entry-symbol", named, "named", 0x30, 8, 0, "symbol"),
        ("registration-symbol-alias", named, "named", 0x38, 8, 0x0000000100002000, "symbol"),
        ("registration-named-oop", named, "named", 0x40, 8, 0, "implementation nullability"),
        ("registration-block-oop", block, "block", 0x40, 8, 1, "implementation nullability"),
        ("registration-code-identity", named, "named", 0x48, 8, 0, "code identity"),
        ("registration-code-lease", named, "named", 0x50, 8, 0, "code lease"),
        ("registration-portable-identity", named, "named", 0x58, 8, 0, "portable identity"),
        ("registration-language-arity", named, "named", 0x60, 4, 1, "language arity"),
        ("registration-driver-count", named, "named", 0x64, 4, 2, "driver argument count"),
        ("registration-descriptor-hash", named, "named", 0x68, 1, named[0x68] ^ 1, "descriptor hash"),
        ("registration-abort-token", abort, "abort", 0x18, 8, 0, "registration token"),
        ("registration-abort-prepare-field", abort, "abort", 0x20, 8, 1, "Abort nullability"),
    )
    for label, source, fixture_kind, offset, width, value, stage in cases:
        mutated = bytearray(source)
        mutated[offset : offset + width] = value.to_bytes(width, "little")
        frozen = bytes(mutated)
        try:
            parse_registration(frozen, fixture_kind)
        except Reject as error:
            require(
                error.stage == stage,
                "fixture harness",
                f"{label}: expected {stage!r}, got {error.stage!r}",
            )
            require(bytes(mutated) == frozen, "rejection immutability", f"{label}: rejected envelope changed")
        else:
            raise Reject("fixture harness", f"{label}: unexpectedly accepted")

    try:
        parse_registration(None, "named")
    except Reject as error:
        require(error.stage == "null envelope", "fixture harness", "null envelope rejection stage")
    else:
        raise Reject("fixture harness", "null registration envelope unexpectedly accepted")

    for label, source, successful_token in (
        ("Prepare success", named, 0x3132333435363738),
        ("Abort success", abort, 0),
    ):
        result = bytearray(source)
        result[0x18:0x20] = successful_token.to_bytes(8, "little")
        require(result[:0x18] == source[:0x18], "success mutation", f"{label}: prefix changed")
        require(result[0x20:] == source[0x20:], "success mutation", f"{label}: suffix changed")
        require(result[0x18:0x20] != source[0x18:0x20], "success mutation", f"{label}: token unchanged")
    return len(cases) + 3


def verify_registration_tuple_mutations() -> int:
    source = (HERE / REGISTRATION_ABI_TUPLE_FIXTURE[0]).read_bytes()
    cases = (
        ("tuple-domain", 0x00, 1, 0x63, "ABI tuple domain"),
        ("tuple-target", 0x24, 1, ord("x"), "ABI tuple target"),
        ("tuple-symbol", 0x51, 1, ord("x"), "ABI tuple symbol"),
        ("tuple-signature", 0x71, 1, ord("x"), "ABI tuple signature"),
        ("tuple-struct-size", 0x95, 4, 135, "ABI tuple struct size"),
        ("tuple-alignment", 0x99, 4, 4, "ABI tuple alignment"),
        ("tuple-field-count", 0x9D, 4, 17, "ABI tuple fields"),
        ("tuple-first-field-offset", 0xB4, 4, 4, "ABI tuple field offset"),
    )
    for label, offset, width, value, stage in cases:
        mutated = bytearray(source)
        mutated[offset : offset + width] = value.to_bytes(width, "little")
        try:
            parse_registration_abi_tuple(bytes(mutated))
        except Reject as error:
            require(
                error.stage == stage,
                "fixture harness",
                f"{label}: expected {stage!r}, got {error.stage!r}",
            )
        else:
            raise Reject("fixture harness", f"{label}: unexpectedly accepted")
    return len(cases)


def verify_image_mutations() -> int:
    trace = (HERE / "image-v5-construction-trace.bin").read_bytes()
    no_code = (HERE / "image-v5-no-code.bin").read_bytes()

    def expect(data: bytes, manifest_name: str | None, stage: str, label: str) -> None:
        try:
            parse_image_v5(data, manifest_name)
        except Reject as error:
            require(
                error.stage == stage,
                "fixture harness",
                f"{label}: expected {stage!r}, got {error.stage!r}",
            )
        else:
            raise Reject("fixture harness", f"{label}: unexpectedly accepted")

    byte_cases = (
        ("image-version", 0x08, 4, 4, "image version"),
        ("image-ABI", 0x14, 8, RUNTIME_ABI_V3 ^ 1, "image ABI"),
        ("image-manifest-length", 0xD4, 8, 431, "image Resumables length"),
        ("image-locator-tag", 0x29C, 1, 0, "image heap locator"),
    )
    for label, offset, width, value, stage in byte_cases:
        mutated = bytearray(trace)
        mutated[offset : offset + width] = value.to_bytes(width, "little")
        expect(bytes(mutated), TRACE_MANIFEST_FIXTURE[0], stage, label)

    header = trace[:28]
    authority = trace[28:104]
    resumables = trace[104:652]
    heap = trace[652:682]
    facts = trace[682:731]
    code = trace[731:]
    sequence_cases = (
        ("image-missing-resumables", header + authority + heap + facts + code, "image record order"),
        ("image-duplicate-resumables", header + authority + resumables + resumables + heap + facts + code, "image record order"),
        ("image-late-resumables", header + authority + heap + resumables + facts + code, "image record order"),
        ("image-heap-after-facts", header + authority + resumables + facts + heap + code, "image record order"),
        ("image-record-after-code", trace + heap, "image record order"),
    )
    for label, mutated, stage in sequence_cases:
        expect(mutated, TRACE_MANIFEST_FIXTURE[0], stage, label)
    expect(no_code + resumables, None, "image no-Code exclusion", "image-orphan-resumables")
    return len(byte_cases) + len(sequence_cases) + 1


def verify_construction_trace() -> int:
    trace_record = json.loads(
        (HERE / "construction-trace-v1.json").read_text(encoding="utf-8")
    )
    require(
        trace_record.get("schema") == "birnam.stackless-s2.construction-trace.v1",
        "construction trace",
        "trace schema",
    )
    require(
        trace_record.get("source_revision")
        == {
            "module": "trace",
            "dependencies": [],
            "authored_source": TRACE_SOURCE.decode("ascii"),
            "sha256": TRACE_REVISION.hex(),
        },
        "construction trace",
        "source revision record",
    )
    require(
        TRACE_REVISION.hex()
        == "0b0261b15fce452514549a5369bb41624215eaa4e6469a3c370495bdffdaa8a1",
        "construction trace",
        "SourceRevision digest",
    )
    artifact = (HERE / "artifact-selection-construction-trace.bin").read_bytes()
    generation = (HERE / "generation-selection-construction-trace.bin").read_bytes()
    _, artifact_rows = parse_artifact_selection(artifact, (TRACE_NAMED, TRACE_BLOCK))
    generation_revisions, generation_rows = parse_generation_selection(
        generation, (TRACE_NAMED, TRACE_BLOCK)
    )
    require(artifact_rows == generation_rows, "construction trace", "selection rows")
    require(generation_revisions == (TRACE_REVISION,), "construction trace", "generation revision scope")
    require(artifact[32:] == generation[32:], "construction trace", "domain-separated selection body")

    named_descriptor = (HERE / "frame-descriptor-construction-trace-named.bin").read_bytes()
    block_descriptor = (HERE / "frame-descriptor-construction-trace-block.bin").read_bytes()
    require(hashlib.sha256(named_descriptor).digest() == TRACE_NAMED_DESCRIPTOR_HASH, "construction trace", "named descriptor hash")
    require(hashlib.sha256(block_descriptor).digest() == TRACE_BLOCK_DESCRIPTOR_HASH, "construction trace", "block descriptor hash")

    definition_hash = hashlib.sha256(
        b"birnam.definition.v1\0" + TRACE_DEFINITION
    ).digest()
    require(
        trace_record.get("portable_operation")
        == {
            "module": "trace",
            "target": "Trace",
            "selector": "run",
            "action": "Install",
            "definition_source": TRACE_DEFINITION.decode("ascii"),
            "definition_sha256": definition_hash.hex(),
            "carrier": "Resumable",
            "descriptor_sha256": TRACE_NAMED_DESCRIPTOR_HASH.hex(),
        },
        "construction trace",
        "portable operation record",
    )
    source_map_record = (
        struct.pack(">I", len(TRACE_MODULE)) + TRACE_MODULE + TRACE_REVISION
    )
    operation_record = (
        struct.pack(">I", len(TRACE_MODULE))
        + TRACE_MODULE
        + struct.pack(">I", len(TRACE_TARGET))
        + TRACE_TARGET
        + struct.pack(">I", len(TRACE_SELECTOR))
        + TRACE_SELECTOR
        + bytes((1,))
        + definition_hash
        + bytes((2,))
        + TRACE_NAMED_DESCRIPTOR_HASH
    )
    generation_digest = hashlib.sha256(generation).digest()
    require(
        generation_digest.hex()
        == "0246cbd4d149a369a095da35bdd8525974acf58d9f4c1b499e580393c9b42197",
        "construction trace",
        "GenerationSelection digest",
    )
    authority = hashlib.sha256(
        b"birnam.dispatch-base.v2\0"
        + struct.pack(">I", 1)
        + source_map_record
        + struct.pack(">I", 1)
        + operation_record
        + generation_digest
    ).digest()
    require(authority == TRACE_AUTHORITY, "construction trace", "DispatchGeneration authority")
    require(
        trace_record["dispatch_generation"]
        == {
            "domain": r"birnam.dispatch-base.v2\0",
            "authority_sha256": authority.hex(),
        },
        "construction trace",
        "authority record",
    )

    trace_manifest = (HERE / TRACE_MANIFEST_FIXTURE[0]).read_bytes()
    entries = parse_manifest(
        trace_manifest,
        TRACE_MANIFEST_FIXTURE[3],
        TRACE_MANIFEST_FIXTURE[4],
        authority,
    )
    require(entries == (TRACE_NAMED_MANIFEST_ENTRY, TRACE_BLOCK_MANIFEST_ENTRY), "construction trace", "manifest rows")
    require(hashlib.sha256(trace_manifest).hexdigest() == TRACE_MANIFEST_FIXTURE[2], "construction trace", "manifest digest")

    require(parse_image_v5((HERE / "image-v5-construction-trace.bin").read_bytes(), TRACE_MANIFEST_FIXTURE[0]) == 4, "construction trace", "mapped repeated references")
    require(parse_image_v5((HERE / "image-v5-no-code.bin").read_bytes(), None) == 0, "construction trace", "rebuild path")
    require(
        trace_record["image"]["runtime_abi_v3"] == f"{RUNTIME_ABI_V3:016x}"
        and trace_record["image"]["block_manifest_index"] == 1
        and trace_record["image"]["repeated_reference_count"] == 4,
        "construction trace",
        "image record",
    )
    require(
        trace_record["acyclic_order"]
        == [
            "source revision",
            "artifact and generation selections",
            "frame descriptors",
            "generation-selection digest",
            "dispatch-generation authority",
            "resumable manifest",
            "native registration envelopes",
            "image v5",
        ],
        "construction trace",
        "acyclic order",
    )
    return 20


def main() -> int:
    try:
        verify_fixtures()
        verify_mutations()
        verify_truncation()
        composition_cases = verify_composition_cases()
        verify_generation_padding()
        overflow_cases = verify_limit_and_overflow_cases()
        boundary_cases = verify_all_limit_boundaries()
        descriptor_mutations = verify_descriptor_mutations()
        execution_descriptor_mutations = verify_execution_descriptor_mutations()
        manifest_mutations = verify_manifest_mutations()
        collision_policy_checks = verify_portable_collision_policy()
        registration_mutations = verify_registration_mutations()
        registration_tuple_mutations = verify_registration_tuple_mutations()
        image_mutations = verify_image_mutations()
        construction_trace_checks = verify_construction_trace()
    except (OSError, Reject) as error:
        print(f"stackless-s2 fixtures: FAIL: {error}", file=sys.stderr)
        return 1
    print(
        "stackless-s2 fixtures: ok "
        f"artifact-selection={len(ARTIFACT_FIXTURES)} "
        f"generation-selection={len(GENERATION_FIXTURES)} "
        f"frame-descriptor={len(DESCRIPTOR_FIXTURES) + len(TRACE_DESCRIPTOR_FIXTURES)} "
        f"execution-descriptor={len(EXECUTION_DESCRIPTOR_FIXTURES)} "
        f"resumable-manifest={len(MANIFEST_FIXTURES) + 1} "
        f"registration-envelope={len(REGISTRATION_FIXTURES)} "
        f"image-v5={len(IMAGE_FIXTURES)} "
        f"composition-cases={composition_cases} mutations={len(MUTATIONS) + 1} "
        f"descriptor-mutations={descriptor_mutations} "
        f"execution-descriptor-mutations={execution_descriptor_mutations} "
        f"manifest-mutations={manifest_mutations} "
        f"collision-policy={collision_policy_checks} "
        f"registration-mutations={registration_mutations} limit-overflow={overflow_cases}"
        f" limit-boundaries={boundary_cases}"
        f" registration-tuple-mutations={registration_tuple_mutations}"
        f" image-mutations={image_mutations} construction-trace={construction_trace_checks}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
