#!/usr/bin/env python3
"""Manual second encoder for the Design 60L PortableProgram fixtures.

Usage: build-fixtures.py OUTPUT_DIRECTORY

This serializer is intentionally independent of the Rust codec. It refuses to
choose the committed fixture directory implicitly; generate elsewhere first
and compare deliberately.
"""

from __future__ import annotations

import hashlib
import struct
import sys
from pathlib import Path
from typing import NamedTuple


DOMAIN = b"birnam.portable-program.v1\0"
ABSENT = (1 << 32) - 1


class Node(NamedTuple):
    kind: int
    payload: int = 0
    auxiliary_0: int = 0
    auxiliary_1: int = 0
    operands: tuple[int, ...] = ()
    data: bytes = b""


def align8(value: int) -> int:
    return (value + 7) & ~7


def directory(kind: int, stride: int, count: int, offset: int) -> bytes:
    return struct.pack("<IIIIQQ", kind, stride, count, 0, offset, stride * count)


def program(
    parameters: tuple[bytes, ...] = (),
    captures: tuple[bytes, ...] = (),
    lets: tuple[tuple[bytes, int], ...] = (),
    statements: tuple[tuple[int, int, int], ...] = (),
    nodes: tuple[Node, ...] = (),
) -> bytes:
    bindings = tuple((1, ordinal, name, ABSENT) for ordinal, name in enumerate(parameters))
    bindings += tuple((2, ordinal, name, ABSENT) for ordinal, name in enumerate(captures))
    bindings += tuple((3, ordinal, name, defining) for ordinal, (name, defining) in enumerate(lets))
    operands = tuple(operand for node in nodes for operand in node.operands)
    arena = b"".join(name for _, _, name, _ in bindings) + b"".join(node.data for node in nodes)

    bindings_offset = 288
    statements_offset = bindings_offset + 24 * len(bindings)
    nodes_offset = statements_offset + 16 * len(statements)
    operands_offset = nodes_offset + 40 * len(nodes)
    bytes_offset = align8(operands_offset + 4 * len(operands))
    total = bytes_offset + len(arena)

    out = bytearray(DOMAIN)
    out.extend(bytes(32 - len(out)))
    out.extend(struct.pack("<IIQIIII", 1, 64, total, 0, 6, 32, 0))
    for entry in (
        directory(1, 32, 1, 256),
        directory(2, 24, len(bindings), bindings_offset),
        directory(3, 16, len(statements), statements_offset),
        directory(4, 40, len(nodes), nodes_offset),
        directory(5, 4, len(operands), operands_offset),
        directory(6, 1, len(arena), bytes_offset),
    ):
        out.extend(entry)
    out.extend(
        struct.pack(
            "<8I",
            1,
            len(parameters),
            len(captures),
            len(bindings),
            len(statements),
            len(nodes),
            len(operands),
            0,
        )
    )
    for kind, ordinal, name, defining in bindings:
        out.extend(struct.pack("<IIIIQ", kind, ordinal, len(name), defining, 0))
    for kind, root, binding in statements:
        out.extend(struct.pack("<IIII", kind, root, binding, 0))
    for node in nodes:
        out.extend(
            struct.pack(
                "<IIIIQIIQ",
                node.kind,
                0,
                len(node.operands),
                len(node.data),
                node.payload,
                node.auxiliary_0,
                node.auxiliary_1,
                0,
            )
        )
    for operand in operands:
        out.extend(struct.pack("<I", operand))
    out.extend(bytes(bytes_offset - len(out)))
    out.extend(arena)
    assert len(out) == total
    return bytes(out)


def inventory(programs: tuple[bytes, ...]) -> bytes:
    unique = sorted(set(programs), key=lambda value: (hashlib.sha256(value).digest(), value))
    rows_offset = 128
    programs_offset = align8(rows_offset + 48 * len(unique))
    arena = b"".join(unique)
    total = programs_offset + len(arena)
    out = bytearray(b"birnam.portable-inventory.v1\0")
    out.extend(bytes(32 - len(out)))
    out.extend(struct.pack("<IIQIIII", 1, 64, total, 0, 2, 32, 0))
    out.extend(directory(1, 48, len(unique), rows_offset))
    out.extend(directory(2, 1, len(arena), programs_offset))
    for value in unique:
        out.extend(hashlib.sha256(value).digest())
        out.extend(struct.pack("<4I", len(value), 0, struct.unpack_from("<I", value, 260)[0], struct.unpack_from("<I", value, 264)[0]))
    out.extend(bytes(programs_offset - len(out)))
    out.extend(arena)
    assert len(out) == total
    return bytes(out)


def field(tag: int, value: int | bytes | None = None) -> bytes:
    if tag in (0, 1, 2, 8):
        assert value is None
        return bytes((tag,))
    if tag in (3, 13, 14, 15):
        assert isinstance(value, int)
        return bytes((tag,)) + struct.pack("<q" if tag in (3, 13) else "<Q", value)
    if tag in (7, 11, 12):
        assert isinstance(value, int)
        return bytes((tag,)) + struct.pack("<I", value)
    assert tag in (5, 9, 16) and isinstance(value, bytes)
    return bytes((tag,)) + struct.pack("<I", len(value)) + value


def selector_arity(selector: bytes) -> int:
    colons = selector.count(b":")
    if colons:
        return colons
    return 0 if chr(selector[0]).isascii() and (chr(selector[0]).isalpha() or selector[0] == 95) else 1


def code_bindings(rows: tuple[tuple[bytes, bytes, bytes, bytes], ...]) -> bytes:
    rows = tuple(sorted(rows, key=lambda row: (row[0], row[1][5:])))
    fields = b"".join(value for row in rows for value in row)
    fields_offset = align8(128 + 32 * len(rows))
    total = fields_offset + len(fields)
    out = bytearray(b"birnam.code-bindings.v2\0")
    out.extend(bytes(32 - len(out)))
    out.extend(struct.pack("<IIQIIII", 1, 64, total, 0, 2, 32, 0))
    out.extend(directory(1, 32, len(rows), 128))
    out.extend(directory(2, 1, len(fields), fields_offset))
    for target, selector, implementation, provenance in rows:
        arity = selector_arity(selector[5:])
        out.extend(struct.pack("<6IQ", len(target), len(selector), len(implementation), len(provenance), arity, 0, 0))
    out.extend(bytes(fields_offset - len(out)))
    out.extend(fields)
    assert len(out) == total
    return bytes(out)


def binding_semantics(value: bytes) -> tuple[bytes, tuple[tuple[bytes, int, bytes], ...]]:
    count = struct.unpack_from("<I", value, 72)[0]
    fields_offset = struct.unpack_from("<Q", value, 112)[0]
    cursor = fields_offset
    digests = []
    rows = []
    for index in range(count):
        base = 128 + 32 * index
        lengths = struct.unpack_from("<4I", value, base)
        arity = struct.unpack_from("<I", value, base + 16)[0]
        encoded = []
        for length in lengths:
            encoded.append(value[cursor : cursor + length])
            cursor += length
        target, selector, implementation, _ = encoded
        digest = hashlib.sha256(
            b"birnam.code-binding-row.v2\0"
            + struct.pack("<5I", index, len(target), len(selector), len(implementation), arity)
            + target
            + selector
            + implementation
        ).digest()
        digests.append(digest)
        rows.append((selector[5:], arity, digest))
    semantics = hashlib.sha256(
        b"birnam.code-binding-semantics.v2\0" + struct.pack("<I", count) + b"".join(digests)
    ).digest()
    return semantics, tuple(rows)


def materializations(
    programs: bytes,
    bindings: bytes,
    named: tuple[int, ...],
    blocks: tuple[int, ...],
) -> bytes:
    binding_digest, binding_rows = binding_semantics(bindings)
    program_count = struct.unpack_from("<I", programs, 72)[0]
    assert len(named) == len(binding_rows) and len(blocks) == program_count
    program_rows = []
    for index in range(program_count):
        base = 128 + 48 * index
        digest = programs[base : base + 32]
        parameters, captures = struct.unpack_from("<2I", programs, base + 40)
        selector = b"value" if parameters == 0 else b"value:" * parameters
        program_rows.append((selector, parameters, captures, digest))
    selectors = b"".join(row[0] for row in binding_rows) + b"".join(row[0] for row in program_rows)
    row_count = len(named) + len(blocks)
    selectors_offset = 224 + 64 * row_count
    total = selectors_offset + len(selectors)
    out = bytearray(b"birnam.code-selection.v1\0")
    out.extend(bytes(32 - len(out)))
    out.extend(struct.pack("<IIQIIII", 1, 64, total, 0, 3, 32, 0))
    out.extend(directory(1, 64, 1, 160))
    out.extend(directory(2, 64, row_count, 224))
    out.extend(directory(3, 1, len(selectors), selectors_offset))
    out.extend(hashlib.sha256(programs).digest())
    out.extend(binding_digest)
    for index, disposition in enumerate(named):
        selector, arity, digest = binding_rows[index]
        out.extend(struct.pack("<8I32s", 1, disposition, index, arity, 0, len(selector), 0, 0, digest))
    for index, disposition in enumerate(blocks):
        selector, parameters, captures, digest = program_rows[index]
        out.extend(struct.pack("<8I32s", 2, disposition, index, parameters, captures, len(selector), 0, 0, digest))
    out.extend(selectors)
    assert len(out) == total
    return bytes(out)


def image_v8(
    programs: bytes,
    bindings: bytes,
    selection: bytes,
    heaps: tuple[bytes, ...] = (),
    facts: tuple[bytes, ...] = (),
    code: bytes | None = None,
) -> bytes:
    binding_digest, _ = binding_semantics(bindings)
    programs_digest = hashlib.sha256(programs).digest()
    selection_digest = hashlib.sha256(selection).digest()
    authority = hashlib.sha256(
        b"birnam.portable-image-authority.v1\0"
        + programs_digest
        + binding_digest
        + selection_digest
    ).hexdigest().encode("ascii")

    def record(kind: int, payload: bytes) -> bytes:
        return struct.pack("<IQ", kind, len(payload)) + payload

    def hashed(kind: int, value: bytes) -> bytes:
        return record(kind, hashlib.sha256(value).digest() + struct.pack("<Q", len(value)) + value)

    out = bytearray(b"BIRNAMIM" + struct.pack("<IIIQ", 8, 0x01020304, 1, 0xCDB641F978540D45))
    out.extend(record(5, authority))
    out.extend(hashed(7, programs))
    out.extend(hashed(8, bindings))
    out.extend(hashed(9, selection))
    for heap in heaps:
        out.extend(record(3, heap))
    for fact_set in facts:
        out.extend(record(2, fact_set))
    if code is not None:
        assert code
        out.extend(record(4, struct.pack("<Q", wyhash(code)) + authority + code))
    return bytes(out)


WYHASH_SECRET = (0xA0761D6478BD642F, 0xE7037ED1A0B428DB, 0x8EBC6AF09C88C6E3, 0x589965CC75374CC3)


def wymix(left: int, right: int) -> int:
    product = left * right
    return ((product & ((1 << 64) - 1)) ^ (product >> 64)) & ((1 << 64) - 1)


def wyhash(value: bytes, seed: int = 0) -> int:
    state = [seed ^ wymix(seed ^ WYHASH_SECRET[0], WYHASH_SECRET[1])] * 3
    if len(value) <= 16:
        a = b = 0
        if len(value) >= 4:
            end = len(value) - 4
            quarter = (len(value) >> 3) << 2
            a = int.from_bytes(value[:4], "little") << 32 | int.from_bytes(value[quarter : quarter + 4], "little")
            b = int.from_bytes(value[end : end + 4], "little") << 32 | int.from_bytes(value[end - quarter : end - quarter + 4], "little")
        elif value:
            a = value[0] << 16 | value[len(value) >> 1] << 8 | value[-1]
    else:
        offset = 0
        if len(value) >= 48:
            while offset + 48 < len(value):
                chunk = value[offset : offset + 48]
                for index in range(3):
                    state[index] = wymix(
                        int.from_bytes(chunk[index * 16 : index * 16 + 8], "little") ^ WYHASH_SECRET[index + 1],
                        int.from_bytes(chunk[index * 16 + 8 : index * 16 + 16], "little") ^ state[index],
                    )
                offset += 48
            state[0] ^= state[1] ^ state[2]
        remaining = value[offset:]
        inner = 0
        while inner + 16 < len(remaining):
            state[0] = wymix(
                int.from_bytes(remaining[inner : inner + 8], "little") ^ WYHASH_SECRET[1],
                int.from_bytes(remaining[inner + 8 : inner + 16], "little") ^ state[0],
            )
            inner += 16
        a = int.from_bytes(value[-16:-8], "little")
        b = int.from_bytes(value[-8:], "little")
    product = ((a ^ WYHASH_SECRET[1]) * (b ^ state[0])) & ((1 << 128) - 1)
    low = product & ((1 << 64) - 1)
    high = product >> 64
    return wymix(low ^ WYHASH_SECRET[0] ^ len(value), high ^ WYHASH_SECRET[1])


def fixtures() -> dict[str, bytes]:
    nested = program(captures=(b"a", b"b"))
    nested_one = program(captures=(b"a",))
    nonempty_nested = program(
        captures=(b"\xffdup",),
        statements=((1, 2, ABSENT),),
        nodes=(
            Node(8, auxiliary_0=0),
            Node(1, 1),
            Node(10, auxiliary_0=1, operands=(0, 1), data=b"\xff"),
        ),
    )
    outer = program(
        parameters=(b"p",),
        statements=((1, 0, ABSENT),),
        nodes=(Node(11, auxiliary_0=0, auxiliary_1=2, operands=(0, 0), data=nested),),
    )
    nonempty_outer = program(
        parameters=(b"\xffdup",),
        captures=(b"\xffdup",),
        statements=((1, 0, ABSENT),),
        nodes=(Node(11, auxiliary_0=0, auxiliary_1=1, operands=(0,), data=nonempty_nested),),
    )
    binding_rows = (
        (field(5, b"Type"), field(5, b"run"), field(7, 7), field(7, 11)),
        (field(7, 8), field(5, b"+"), field(9, b"Plus"), field(0)),
        (field(0), field(5, b"at:put:"), field(5, b"method"), field(7, 3)),
    )
    provenance_variant = binding_rows[:-3] + (
        (field(5, b"Type"), field(5, b"run"), field(7, 7), field(7, 10)),
        binding_rows[1],
        binding_rows[2],
    )
    block_implementation_rows = (
        (field(5, b"Type"), field(5, b"run"), field(7, 1), field(7, 11)),
        binding_rows[1],
        binding_rows[2],
    )
    program_inventory = inventory((outer, nested))
    single_program_inventory = inventory((nested,))
    duplicate_program_inventory = inventory((nested, nested))
    shared_program_inventory = inventory((nonempty_outer, nonempty_nested))
    empty_program_inventory = inventory(())
    binding_inventory = code_bindings(binding_rows)
    block_binding_inventory = code_bindings(block_implementation_rows)
    empty_binding_inventory = code_bindings(())
    mixed_selection = materializations(program_inventory, binding_inventory, (1, 2, 1), (2, 1))
    block_binding_selection = materializations(
        program_inventory, block_binding_inventory, (1, 2, 1), (2, 1)
    )
    shared_selection = materializations(shared_program_inventory, empty_binding_inventory, (), (1, 2))
    empty_selection = materializations(empty_program_inventory, empty_binding_inventory, (), ())
    heap = bytearray(struct.pack("<I", 12))
    heap.extend(field(9, b"CodeObjectVt") + struct.pack("<I", 2) + field(11, 1) + field(0))
    heap.extend(field(12, 0) + struct.pack("<I", 1) + field(7, 0))
    for _ in range(6):
        heap.extend(field(9, b"ObjectVt") + struct.pack("<I", 0))
    heap.extend(field(13, -1) + struct.pack("<I", 0))
    heap.extend(field(14, 0x7FF8000000000042) + struct.pack("<I", 0))
    heap.extend(field(15, 0xFEDCBA9876543210) + struct.pack("<I", 0))
    heap.extend(field(16, b"source") + struct.pack("<I", 0))
    facts = (
        struct.pack("<QQB7xQI", 10, 5, 2, 3, 1)
        + struct.pack("<BBQ", 1, 0, 9)
        + field(7, 1)
        + field(5, b"seen")
        + field(12, 0)
    )
    shared_programs = sorted(
        {nonempty_outer, nonempty_nested},
        key=lambda value: (hashlib.sha256(value).digest(), value),
    )
    nested_program_index = shared_programs.index(nonempty_nested)
    outer_program_index = shared_programs.index(nonempty_outer)
    shared_heap = bytearray(struct.pack("<I", 8))
    for _ in range(2):
        shared_heap.extend(
            field(9, b"CodeObjectVt")
            + struct.pack("<I", 2)
            + field(11, nested_program_index)
            + field(0)
        )
    shared_heap.extend(
        field(9, b"CodeObjectVt")
        + struct.pack("<I", 2)
        + field(11, outer_program_index)
        + field(0)
    )
    shared_heap.extend(field(12, 0) + struct.pack("<I", 2) + field(7, 0) + field(7, 4))
    shared_heap.extend(field(12, 0) + struct.pack("<I", 2) + field(7, 0) + field(7, 3))
    shared_heap.extend(field(12, 1) + struct.pack("<I", 2) + field(7, 1) + field(7, 5))
    shared_heap.extend(field(12, 2) + struct.pack("<I", 2) + field(7, 2) + field(7, 6))
    shared_heap.extend(field(9, b"ObjectVt") + struct.pack("<I", 0))
    return {
        "program-empty.bin": program(),
        "program-explicit-captures.bin": program(parameters=(b"x",), captures=(b"held", b"held")),
        "program-signalling-nan.bin": program(
            statements=((1, 0, ABSENT),),
            nodes=(Node(2, 0x7FF0_0000_0000_0042),),
        ),
        "program-numeric-boundaries.bin": program(
            statements=tuple((1, index, ABSENT) for index in range(8)),
            nodes=(
                Node(1, 1 << 63),
                Node(1, (1 << 63) - 1),
                Node(2, 0x0000000000000000),
                Node(2, 0x8000000000000000),
                Node(2, 0x7FF0000000000000),
                Node(2, 0xFFF0000000000000),
                Node(2, 0x7FF8000000000001),
                Node(2, 0x7FF8000000000042),
            ),
        ),
        "program-send.bin": program(
            statements=((1, 2, ABSENT),),
            nodes=(Node(1, 41), Node(1, 1), Node(10, auxiliary_0=1, operands=(0, 1), data=b"+")),
        ),
        "program-send-arities.bin": program(
            statements=((1, 1, ABSENT), (1, 4, ABSENT), (1, 8, ABSENT), (1, 13, ABSENT)),
            nodes=(
                Node(1, 0),
                Node(10, auxiliary_0=0, operands=(0,), data=b"run"),
                Node(1, 1),
                Node(1, 2),
                Node(10, auxiliary_0=1, operands=(2, 3), data=b"+"),
                Node(1, 3),
                Node(1, 4),
                Node(1, 5),
                Node(10, auxiliary_0=2, operands=(5, 6, 7), data=b"at:put:"),
                Node(1, 6),
                Node(1, 7),
                Node(1, 8),
                Node(1, 9),
                Node(10, auxiliary_0=3, operands=(9, 10, 11, 12), data=b"if:then:else:"),
            ),
        ),
        "program-let-scope.bin": program(
            parameters=(b"x",),
            captures=(b"held",),
            lets=((b"y", 0),),
            statements=((2, 0, 2), (1, 1, ABSENT)),
            nodes=(Node(1, 7), Node(8, auxiliary_0=2)),
        ),
        "program-auxiliary-values.bin": program(
            statements=tuple((1, index, ABSENT) for index in range(7)),
            nodes=(
                Node(3),
                Node(3, data=b"\x00\xff"),
                Node(4, data=b"\xffsym"),
                Node(5),
                Node(6),
                Node(7),
                Node(9, data=b"\xfeG"),
            ),
        ),
        "program-nested-inner.bin": nested,
        "program-nested-captures.bin": outer,
        "program-padding.bin": program(
            parameters=(b"p",),
            statements=((1, 0, ABSENT),),
            nodes=(Node(11, auxiliary_0=0, auxiliary_1=1, operands=(0,), data=nested_one),),
        ),
        "program-future-let.bin": program(
            parameters=(b"p",),
            lets=((b"y", 1),),
            statements=((1, 0, ABSENT), (2, 1, 1)),
            nodes=(Node(8, auxiliary_0=0), Node(1, 1)),
        ),
        "program-nonutf8-names-block.bin": nonempty_outer,
        "inventory-empty.bin": empty_program_inventory,
        "inventory-one.bin": single_program_inventory,
        "inventory-duplicate-elided.bin": duplicate_program_inventory,
        "inventory-nested-closure.bin": program_inventory,
        "inventory-shared-cyclic.bin": shared_program_inventory,
        "bindings-empty.bin": empty_binding_inventory,
        "bindings-block-implementation.bin": block_binding_inventory,
        "bindings-mixed.bin": binding_inventory,
        "bindings-provenance-only-change.bin": code_bindings(provenance_variant),
        "selection-empty.bin": empty_selection,
        "selection-block-implementation.bin": block_binding_selection,
        "selection-mixed.bin": mixed_selection,
        "selection-shared-cyclic.bin": shared_selection,
        "image-empty-stripped.bin": image_v8(empty_program_inventory, empty_binding_inventory, empty_selection),
        "image-mixed-stripped.bin": image_v8(program_inventory, binding_inventory, mixed_selection, (bytes(heap),)),
        "image-block-binding-stripped.bin": image_v8(
            program_inventory,
            block_binding_inventory,
            block_binding_selection,
            (bytes(heap),),
        ),
        "image-mixed-code.bin": image_v8(
            program_inventory,
            binding_inventory,
            mixed_selection,
            (bytes(heap),),
            code=b"\x7fELFbirnam-v8-fixture",
        ),
        "image-mixed-facts-stripped.bin": image_v8(
            program_inventory,
            binding_inventory,
            mixed_selection,
            (bytes(heap),),
            (facts,),
        ),
        "image-shared-cyclic-stripped.bin": image_v8(
            shared_program_inventory,
            empty_binding_inventory,
            shared_selection,
            (bytes(shared_heap),),
        ),
    }


def boundary_vectors() -> dict[str, tuple[bytes, str]]:
    one_mib = 1 << 20
    sixteen_mib = 16 << 20
    maximum_individual_lengths = program(
        parameters=(b"n" * one_mib,),
        statements=(
            (1, 0, ABSENT),
            (1, 1, ABSENT),
            (1, 2, ABSENT),
            (1, 5, ABSENT),
        ),
        nodes=(
            Node(3, data=b"\x00" * sixteen_mib),
            Node(4, data=b"s" * one_mib),
            Node(9, data=b"g" * one_mib),
            Node(1, 0),
            Node(1, 1),
            Node(10, auxiliary_0=1, operands=(3, 4), data=b"+" * one_mib),
        ),
    )
    maximum_bytes_section = program(
        statements=((1, 0, ABSENT), (1, 1, ABSENT)),
        nodes=(
            Node(3, data=b"a" * sixteen_mib),
            Node(3, data=b"b" * sixteen_mib),
        ),
    )
    return {
        "program-max-individual-lengths.bin": (
            maximum_individual_lengths,
            "name=1MiB,string=16MiB,symbol=1MiB,global=1MiB,selector=1MiB",
        ),
        "program-max-bytes-section.bin": (
            maximum_bytes_section,
            "two-16MiB-strings,total-bytes-section=32MiB",
        ),
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: build-fixtures.py OUTPUT_DIRECTORY")
    output = Path(sys.argv[1]).resolve()
    if output == Path(__file__).resolve().parent:
        raise SystemExit("refusing to regenerate the committed fixture directory in place")
    output.mkdir(parents=True, exist_ok=True)
    hashes = []
    for name, data in sorted(fixtures().items()):
        (output / name).write_bytes(data)
        hashes.append(f"{hashlib.sha256(data).hexdigest()}  {name}\n")
    (output / "SHA256SUMS").write_text("".join(hashes), encoding="ascii")
    boundary_manifest = ["# length sha256 name recipe\n"]
    for name, (data, recipe) in sorted(boundary_vectors().items()):
        (output / name).write_bytes(data)
        boundary_manifest.append(
            f"{len(data)} {hashlib.sha256(data).hexdigest()} {name} {recipe}\n"
        )
    (output / "BOUNDARY_VECTORS").write_text("".join(boundary_manifest), encoding="ascii")


if __name__ == "__main__":
    main()
