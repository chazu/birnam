#!/usr/bin/env python3
"""Manual reproduction aid for Design 60I golden binaries.

Usage: build-fixtures.py OUTPUT_DIRECTORY

This serializer is not used by ordinary verification. Always write to a
separate directory and compare the result deliberately with the frozen files.
"""

from __future__ import annotations

import hashlib
import struct
import sys
from pathlib import Path
from typing import NamedTuple


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
ABSENT = (1 << 32) - 1
AUTHORITY = hashlib.sha256(b"birnam.stackless-s2.fixture-authority.v1\0").digest()
REGISTRATION_TUPLE_DOMAIN = b"birnam.registration-envelope.v1\0"


REGISTRATION_FIELDS = (
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


REGISTRATION_CONSTANTS = (
    ("BIRNAM_REGISTRATION_CARRIER_VERSION_V1", 1),
    ("BIRNAM_REGISTRATION_PREPARE_V1", 1),
    ("BIRNAM_REGISTRATION_ABORT_V1", 2),
    ("BIRNAM_REGISTRATION_OK_V1", 0),
    ("BIRNAM_REGISTRATION_REJECTED_V1", 1),
)
TRACE_MODULE = b"trace"
TRACE_TARGET = b"Trace"
TRACE_SELECTOR = b"run"
TRACE_SOURCE = b"run => self\nblock => [ self ]\n"
TRACE_DEFINITION = b"run => self"


def trace_source_revision() -> bytes:
    return hashlib.sha256(
        b"birnam.source-revision.v1\0"
        + struct.pack(">I", len(TRACE_MODULE))
        + TRACE_MODULE
        + struct.pack(">I", 0)
        + struct.pack(">Q", len(TRACE_SOURCE))
        + TRACE_SOURCE
    ).digest()


TRACE_REVISION = trace_source_revision()


class Selection(NamedTuple):
    revision: bytes
    root_kind: int
    root_index: int
    path: tuple[int, ...]
    implementation_kind: int
    disposition: int


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


def align8(value: int) -> int:
    return (value + 7) & ~7


def prefix(domain: bytes, version: int, total: int, section_count: int) -> bytes:
    assert len(domain) == 32
    return domain + struct.pack("<IIQIIII", version, 64, total, 0, section_count, 32, 0)


def directory_entry(kind: int, stride: int, count: int, offset: int, length: int) -> bytes:
    return struct.pack("<IIIIQQ", kind, stride, count, 0, offset, length)


def serialize_selection(
    domain: bytes, revisions: tuple[bytes, ...], rows: tuple[Selection, ...]
) -> bytes:
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
    directory = b"".join(
        (
            directory_entry(1, 32, len(revisions), revision_offset, len(revision_bytes)),
            directory_entry(2, 52, len(rows), row_offset, len(row_bytes)),
            directory_entry(3, 4, len(path_bytes) // 4, path_offset, len(path_bytes)),
        )
    )
    result = prefix(domain, 1, total, 3) + directory + revision_bytes
    result += bytes(row_offset - len(result)) + row_bytes
    result += bytes(path_offset - len(result)) + path_bytes
    assert len(result) == total
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
    digest = hashlib.sha256(domain + stable_key(selection) + path + struct.pack("<I", lowering_version)).digest()
    return int.from_bytes(digest[:8], "little")


def serialize_descriptor(sections: tuple[tuple[int, int, bytes], ...]) -> bytes:
    assert len(sections) == 15
    cursor = 64 + len(sections) * 32
    entries = []
    payload = bytearray()
    for kind, stride, section in sections:
        offset = align8(cursor)
        payload.extend(bytes(offset - cursor))
        assert len(section) % stride == 0
        entries.append(directory_entry(kind, stride, len(section) // stride, offset, len(section)))
        payload.extend(section)
        cursor = offset + len(section)
    return prefix(DESCRIPTOR_DOMAIN, 2, cursor, 15) + b"".join(entries) + bytes(payload)


def metadata(
    selection: Selection,
    slots: int,
    captures: int,
    pc_count: int = 1,
    driver_argument_count: int = 0,
    call_window_start: int = ABSENT,
    call_window_capacity: int = 0,
) -> bytes:
    lowering_version = 1
    portable = derived_identity(b"birnam.portable-implementation.v1\0", selection, lowering_version)
    source_map = derived_identity(b"birnam.source-map-identity.v1\0", selection, lowering_version)
    implementation_slot = 1 if selection.implementation_kind == 1 else ABSENT
    return struct.pack(
        "<QQIIIIIIIIIIII",
        portable,
        source_map,
        lowering_version,
        selection.implementation_kind,
        slots,
        pc_count,
        driver_argument_count,
        0,
        1,
        0,
        implementation_slot,
        call_window_start,
        call_window_capacity,
        0,
    )


def source_file(
    path: bytes, revision: bytes = REVISION_A, source_length: int = 64
) -> bytes:
    return revision + struct.pack("<QII", source_length, len(path), 0)


def source_spans(source_length: int = 64) -> bytes:
    return struct.pack("<IIQQ", 0, 0, 0, 16) + struct.pack(
        "<IIQQ", 0, 0, 0, source_length
    )


def binding(identity: int, kind: int, name: bytes) -> bytes:
    return struct.pack("<QIIIIII", identity, kind, len(name), 0, 1, ABSENT, 0)


def logical_value(value_id: int, kind: int, binding_index: int, first_use: int, use_count: int) -> bytes:
    return struct.pack("<IIIIIIII", value_id, kind, binding_index, 0, first_use, use_count, 1, 0)


def location(value_id: int, slot: int) -> bytes:
    return struct.pack("<IIII", 0, value_id, 1, slot)


def return_descriptor(
    selection: Selection,
    block: bool,
    path: bytes = b"a.birnam",
    source_length: int = 64,
) -> bytes:
    receiver_name = b"self"
    second_name = b"x" if block else b""
    return_slot = 1 if block else 0
    captures = struct.pack("<I", 1) if block else b""
    root_words = struct.pack("<Q", 0b11)
    pc_record = struct.pack("<IIIIIIII", 3, 0, return_slot, 0, 0, 2, 0, 0)
    bindings = binding(1, 1, receiver_name) + binding(2, 4 if block else 5, second_name)
    values = (
        logical_value(0, 1, 0, 0, 0)
        + logical_value(1, 3 if block else 4, 1, 0, 1 if block else 0)
    )
    uses = struct.pack("<IIII", 1 if block else 0, 0, 0, 3)
    if not block:
        values = logical_value(0, 1, 0, 0, 1) + logical_value(1, 4, 1, 1, 0)
    locations = location(0, 0) + location(1, 1)
    source_bytes = path + receiver_name + second_name
    sections = (
        (1, 64, metadata(selection, 2, 1 if block else 0)),
        (2, 4, captures),
        (3, 32, pc_record),
        (4, 8, root_words),
        (5, 48, b""),
        (6, 32, b""),
        (7, 8, b""),
        (8, 8, b""),
        (9, 48, source_file(path, selection.revision, source_length)),
        (10, 24, source_spans(source_length)),
        (11, 32, bindings),
        (12, 32, values),
        (13, 16, uses),
        (14, 16, locations),
        (15, 1, source_bytes),
    )
    return serialize_descriptor(sections)


def execution_descriptor(
    selection: Selection,
    slot_count: int,
    pcs: tuple[tuple[int, int], ...],
    roots: tuple[int, ...],
    call_sites: tuple[tuple[tuple[int, ...], bytes], ...],
    edges: tuple[tuple[int, ...], ...],
    copies: tuple[tuple[int, int], ...],
    clears: tuple[int, ...],
    binding_specs: tuple[tuple[int, bytes], ...],
    value_specs: tuple[tuple[int, int, int, int, int, int, int], ...],
    use_specs: tuple[tuple[int, int, int, int], ...],
    locations_by_pc: tuple[tuple[tuple[int, int], ...], ...],
    call_window_start: int = ABSENT,
    call_window_capacity: int = 0,
) -> bytes:
    assert len(pcs) == len(roots) == len(locations_by_pc)
    assert len(edges) == len(clears)
    path = b"fixture.birnam"
    location_cursor = 0
    pc_bytes = bytearray()
    location_bytes = bytearray()
    for pc, ((kind, payload), locations) in enumerate(zip(pcs, locations_by_pc)):
        pc_bytes.extend(
            struct.pack(
                "<IIIIIIII", kind, 0, payload, 0, location_cursor, len(locations), 0, 0
            )
        )
        for value_id, slot in locations:
            location_bytes.extend(struct.pack("<IIII", pc, value_id, 1, slot))
        location_cursor += len(locations)

    call_bytes = b"".join(struct.pack("<IIIIIIIIIIII", *fields) for fields, _ in call_sites)
    selector_bytes = b"".join(selector for _, selector in call_sites)
    edge_bytes = b"".join(struct.pack("<IIIIIIII", *edge) for edge in edges)
    copy_bytes = b"".join(struct.pack("<II", *copy) for copy in copies)
    binding_bytes = b"".join(
        struct.pack("<QIIIIII", index + 1, kind, len(name), 0, 0, ABSENT, 0)
        for index, (kind, name) in enumerate(binding_specs)
    )
    value_bytes = b"".join(
        struct.pack("<IIIIIIII", value_id, kind, binding_index, definition, first_use, use_count, flags, 0)
        for value_id, kind, binding_index, definition, first_use, use_count, flags in value_specs
    )
    use_bytes = b"".join(struct.pack("<IIII", *use) for use in use_specs)
    source_bytes = selector_bytes + path + b"".join(name for _, name in binding_specs)
    sections = (
        (
            1,
            64,
            metadata(
                selection,
                slot_count,
                0,
                len(pcs),
                1 if len(pcs) > 1 else 0,
                call_window_start,
                call_window_capacity,
            ),
        ),
        (2, 4, b""),
        (3, 32, bytes(pc_bytes)),
        (4, 8, b"".join(struct.pack("<Q", word) for word in roots)),
        (5, 48, call_bytes),
        (6, 32, edge_bytes),
        (7, 8, copy_bytes),
        (8, 8, b"".join(struct.pack("<Q", word) for word in clears)),
        (9, 48, source_file(path, selection.revision)),
        (10, 24, struct.pack("<IIQQ", 0, 0, 0, 64)),
        (11, 32, binding_bytes),
        (12, 32, value_bytes),
        (13, 16, use_bytes),
        (14, 16, bytes(location_bytes)),
        (15, 1, source_bytes),
    )
    return serialize_descriptor(sections)


def call_descriptor() -> bytes:
    return execution_descriptor(
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
    )


def tail_call_descriptor() -> bytes:
    return execution_descriptor(
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
    )


def branch_join_descriptor() -> bytes:
    return execution_descriptor(
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
    )


def cyclic_copy_descriptor() -> bytes:
    return execution_descriptor(
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
    )


def colored_reuse_descriptor() -> bytes:
    return execution_descriptor(
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
    )


def selector_arity(selector: bytes) -> int:
    colons = selector.count(b":")
    if colons:
        return colons
    if not selector or chr(selector[0]).isascii() and (chr(selector[0]).isalpha() or selector[0] == 0x5F):
        return 0
    return 1


def serialize_manifest(
    entries: tuple[tuple[Selection, bytes, bytes], ...], authority: bytes = AUTHORITY
) -> bytes:
    sorted_entries = tuple(
        sorted(
            entries,
            key=lambda entry: (
                entry[0].implementation_kind,
                entry[0].revision,
                entry[0].root_kind,
                entry[0].root_index,
                entry[0].path,
                derived_identity(b"birnam.portable-implementation.v1\0", entry[0], 1),
            ),
        )
    )
    row_bytes = b"".join(
        stable_key(selection)
        + struct.pack(
            "<Q",
            derived_identity(b"birnam.portable-implementation.v1\0", selection, 1),
        )
        + descriptor_hash
        + struct.pack("<II", len(selector), selector_arity(selector))
        for selection, descriptor_hash, selector in sorted_entries
    )
    path_bytes = b"".join(
        struct.pack("<I", ordinal)
        for selection, _, _ in sorted_entries
        for ordinal in selection.path
    )
    selector_bytes = b"".join(selector for _, _, selector in sorted_entries)
    authority_offset = 192
    row_offset = 224
    path_offset = row_offset + len(row_bytes)
    selector_offset = align8(path_offset + len(path_bytes))
    total = selector_offset + len(selector_bytes)
    directory = b"".join(
        (
            directory_entry(1, 32, 1, authority_offset, 32),
            directory_entry(2, 96, len(sorted_entries), row_offset, len(row_bytes)),
            directory_entry(3, 4, len(path_bytes) // 4, path_offset, len(path_bytes)),
            directory_entry(4, 1, len(selector_bytes), selector_offset, len(selector_bytes)),
        )
    )
    result = prefix(MANIFEST_DOMAIN, 1, total, 4) + directory + authority + row_bytes + path_bytes
    result += bytes(selector_offset - len(result)) + selector_bytes
    assert len(result) == total
    return result


def length_prefixed(value: str) -> bytes:
    encoded = value.encode("ascii")
    return struct.pack("<I", len(encoded)) + encoded


def registration_abi_tuple() -> bytes:
    result = bytearray(REGISTRATION_TUPLE_DOMAIN)
    result.extend(length_prefixed("aarch64-apple-darwin|Mach-O|little-endian"))
    result.extend(length_prefixed("birnam_register_resumable_v1"))
    result.extend(length_prefixed("u32(BirnamResumableRegistrationV1 *)"))
    result.extend(struct.pack("<III", 136, 8, len(REGISTRATION_FIELDS)))
    for name, offset, width, alignment, abi_kind, nullability in REGISTRATION_FIELDS:
        result.extend(length_prefixed(name))
        result.extend(struct.pack("<III", offset, width, alignment))
        result.extend(length_prefixed(abi_kind))
        result.extend(length_prefixed(nullability))
    result.extend(struct.pack("<I", len(REGISTRATION_CONSTANTS)))
    for name, value in REGISTRATION_CONSTANTS:
        result.extend(length_prefixed(name))
        result.extend(struct.pack("<I", value))
    return bytes(result)


def be_length_prefixed(value: bytes) -> bytes:
    return struct.pack(">I", len(value)) + value


def trace_dispatch_authority(
    named_descriptor_hash: bytes, generation_selection_hash: bytes
) -> bytes:
    definition_hash = hashlib.sha256(
        b"birnam.definition.v1\0" + TRACE_DEFINITION
    ).digest()
    source_map = be_length_prefixed(TRACE_MODULE) + TRACE_REVISION
    operation = (
        be_length_prefixed(TRACE_MODULE)
        + be_length_prefixed(TRACE_TARGET)
        + be_length_prefixed(TRACE_SELECTOR)
        + bytes((1,))
        + definition_hash
        + bytes((2,))
        + named_descriptor_hash
    )
    return hashlib.sha256(
        b"birnam.dispatch-base.v2\0"
        + struct.pack(">I", 1)
        + source_map
        + struct.pack(">I", 1)
        + operation
        + generation_selection_hash
    ).digest()


def image_record(kind: int, payload: bytes) -> bytes:
    return struct.pack("<IQ", kind, len(payload)) + payload


def image_v5(
    authority: bytes,
    manifest: bytes | None,
    heap_payloads: tuple[bytes, ...] = (),
    facts_payloads: tuple[bytes, ...] = (),
) -> bytes:
    authority_spelling = authority.hex().encode("ascii")
    result = bytearray(
        b"BIRNAMIM"
        + struct.pack("<IIIQ", 5, 0x01020304, 1, 0x6C3B67F0EF1614B3)
    )
    result.extend(image_record(5, authority_spelling))
    if manifest is None:
        assert not heap_payloads or all(b"\x0a" not in payload for payload in heap_payloads)
    else:
        resumables = (
            authority_spelling
            + hashlib.sha256(manifest).digest()
            + struct.pack("<Q", len(manifest))
            + manifest
        )
        result.extend(image_record(6, resumables))
    for payload in heap_payloads:
        result.extend(image_record(3, payload))
    for payload in facts_payloads:
        result.extend(image_record(2, payload))
    if manifest is not None:
        empty_unit_checksum = 0x0409638EE2BDE459
        result.extend(
            image_record(
                4,
                struct.pack("<Q", empty_unit_checksum) + authority_spelling,
            )
        )
    return bytes(result)


def registration_envelope(
    operation: int,
    index: int,
    token: int,
    descriptor_pointer: int,
    descriptor_length: int,
    entry_thunk: int,
    step_method: int,
    implementation_oop: int,
    code_identity: int,
    code_lease: int,
    portable_identity: int,
    language_arity: int,
    driver_argument_count: int,
    descriptor_hash: bytes,
) -> bytes:
    return struct.pack(
        "<IIIIIIQQQQQQQQQII32s",
        1,
        136,
        operation,
        0,
        index,
        0,
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
    )


def fixture_bytes() -> dict[str, bytes]:
    artifact_a = serialize_selection(
        ARTIFACT_DOMAIN, (REVISION_A,), (NAMED_A, BLOCK_A_RESUMABLE)
    )
    artifact_b = serialize_selection(ARTIFACT_DOMAIN, (REVISION_B,), (NAMED_B,))
    artifact_ab = serialize_selection(
        ARTIFACT_DOMAIN,
        (REVISION_A, REVISION_B),
        (NAMED_A, BLOCK_A_RESUMABLE, NAMED_B),
    )
    named_descriptor = return_descriptor(NAMED_A, False)
    block_descriptor = return_descriptor(BLOCK_A_RESUMABLE, True)
    call_descriptor_bytes = call_descriptor()
    named_hash = hashlib.sha256(named_descriptor).digest()
    block_hash = hashlib.sha256(block_descriptor).digest()
    call_hash = hashlib.sha256(call_descriptor_bytes).digest()
    named_portable = derived_identity(
        b"birnam.portable-implementation.v1\0", NAMED_A, 1
    )
    block_portable = derived_identity(
        b"birnam.portable-implementation.v1\0", BLOCK_A_RESUMABLE, 1
    )
    trace_named_descriptor = return_descriptor(
        TRACE_NAMED, False, b"trace.birnam", len(TRACE_SOURCE)
    )
    trace_block_descriptor = return_descriptor(
        TRACE_BLOCK, True, b"trace.birnam", len(TRACE_SOURCE)
    )
    trace_named_hash = hashlib.sha256(trace_named_descriptor).digest()
    trace_block_hash = hashlib.sha256(trace_block_descriptor).digest()
    trace_artifact_selection = serialize_selection(
        ARTIFACT_DOMAIN, (TRACE_REVISION,), (TRACE_NAMED, TRACE_BLOCK)
    )
    trace_generation_selection = serialize_selection(
        GENERATION_DOMAIN, (TRACE_REVISION,), (TRACE_NAMED, TRACE_BLOCK)
    )
    trace_authority = trace_dispatch_authority(
        trace_named_hash, hashlib.sha256(trace_generation_selection).digest()
    )
    trace_manifest = serialize_manifest(
        (
            (TRACE_NAMED, trace_named_hash, TRACE_SELECTOR),
            (TRACE_BLOCK, trace_block_hash, b"value"),
        ),
        trace_authority,
    )
    repeated_heap = (
        struct.pack("<I", 1)
        + bytes((10,))
        + struct.pack("<I", 1)
        + struct.pack("<I", 1)
        + bytes((10,))
        + struct.pack("<I", 1)
    )
    repeated_facts = (
        struct.pack("<QI", 0, 1)
        + bytes((1,))
        + struct.pack("<Q", 1)
        + bytes((10,))
        + struct.pack("<I", 1)
        + bytes((5,))
        + struct.pack("<I", 1)
        + b"v"
        + bytes((10,))
        + struct.pack("<I", 1)
    )
    return {
        "artifact-selection-empty.bin": serialize_selection(ARTIFACT_DOMAIN, (), ()),
        "artifact-selection-all-synchronous.bin": serialize_selection(
            ARTIFACT_DOMAIN, (REVISION_A,), (NAMED_A, BLOCK_A_SYNCHRONOUS)
        ),
        "artifact-selection-mixed.bin": artifact_a,
        "artifact-selection-named-resumable.bin": serialize_selection(
            ARTIFACT_DOMAIN,
            (REVISION_A,),
            (NAMED_A_RESUMABLE, BLOCK_A_SYNCHRONOUS),
        ),
        "artifact-selection-both-resumable.bin": serialize_selection(
            ARTIFACT_DOMAIN,
            (REVISION_A,),
            (NAMED_A_RESUMABLE, BLOCK_A_RESUMABLE),
        ),
        "artifact-selection-selector-edge.bin": serialize_selection(
            ARTIFACT_DOMAIN, (REVISION_CALL,), (NAMED_CALL,)
        ),
        "artifact-selection-b-synchronous.bin": artifact_b,
        "artifact-selection-ab-combined.bin": artifact_ab,
        "generation-selection-a.bin": serialize_selection(
            GENERATION_DOMAIN, (REVISION_A,), (NAMED_A, BLOCK_A_RESUMABLE)
        ),
        "generation-selection-ab.bin": serialize_selection(
            GENERATION_DOMAIN,
            (REVISION_A, REVISION_B),
            (NAMED_A, BLOCK_A_RESUMABLE, NAMED_B),
        ),
        "artifact-selection-construction-trace.bin": trace_artifact_selection,
        "generation-selection-construction-trace.bin": trace_generation_selection,
        "frame-descriptor-minimal-return.bin": named_descriptor,
        "frame-descriptor-source-map-block.bin": block_descriptor,
        "frame-descriptor-call.bin": call_descriptor_bytes,
        "frame-descriptor-tail-call.bin": tail_call_descriptor(),
        "frame-descriptor-branch-join.bin": branch_join_descriptor(),
        "frame-descriptor-cyclic-copy.bin": cyclic_copy_descriptor(),
        "frame-descriptor-colored-reuse.bin": colored_reuse_descriptor(),
        "frame-descriptor-construction-trace-named.bin": trace_named_descriptor,
        "frame-descriptor-construction-trace-block.bin": trace_block_descriptor,
        "resumable-manifest-empty.bin": serialize_manifest(()),
        "resumable-manifest-named-only.bin": serialize_manifest(
            ((NAMED_A_RESUMABLE, named_hash, b"run"),)
        ),
        "resumable-manifest-block-only.bin": serialize_manifest(
            ((BLOCK_A_RESUMABLE, block_hash, b"value"),)
        ),
        "resumable-manifest-mixed.bin": serialize_manifest(
            (
                (NAMED_A_RESUMABLE, named_hash, b"run"),
                (BLOCK_A_RESUMABLE, block_hash, b"value"),
            )
        ),
        "resumable-manifest-selector-edge.bin": serialize_manifest(
            ((NAMED_CALL, call_hash, b"\xff"),)
        ),
        "resumable-manifest-construction-trace.bin": trace_manifest,
        "registration-prepare-named.bin": registration_envelope(
            1,
            0,
            0,
            0x0000000100001000,
            len(named_descriptor),
            0x0000000100002000,
            0x0000000100003000,
            0x0000000100004000,
            0x0102030405060708,
            0x1112131415161718,
            named_portable,
            0,
            0,
            named_hash,
        ),
        "registration-prepare-block.bin": registration_envelope(
            1,
            0,
            0,
            0x0000000100005000,
            len(block_descriptor),
            0x0000000100006000,
            0x0000000100007000,
            0,
            0x0102030405060708,
            0x1112131415161718,
            block_portable,
            0,
            0,
            block_hash,
        ),
        "registration-abort.bin": registration_envelope(
            2,
            0,
            0x2122232425262728,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            bytes(32),
        ),
        "registration-abi-tuple-v1.bin": registration_abi_tuple(),
        "image-v5-empty-manifest.bin": image_v5(
            AUTHORITY, serialize_manifest(())
        ),
        "image-v5-construction-trace.bin": image_v5(
            trace_authority,
            trace_manifest,
            (repeated_heap,),
            (repeated_facts,),
        ),
        "image-v5-no-code.bin": image_v5(trace_authority, None),
    }


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: build-fixtures.py OUTPUT_DIRECTORY", file=sys.stderr)
        return 2
    output = Path(sys.argv[1])
    output.mkdir(parents=True, exist_ok=True)
    for name, data in fixture_bytes().items():
        (output / name).write_bytes(data)
        print(f"{hashlib.sha256(data).hexdigest()}  {name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
