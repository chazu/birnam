#!/usr/bin/env python3
"""Independent reader for the frozen Design 60L PortableProgram fixtures.

This checker deliberately imports no production code and never emits expected
bytes. It implements the normative grammar, then exercises targeted mutations
and truncation at every byte boundary.
"""

from __future__ import annotations

import hashlib
import struct
from pathlib import Path


HERE = Path(__file__).resolve().parent
DOMAIN = b"birnam.portable-program.v1\0"
ABSENT = (1 << 32) - 1
FIXTURES = {
    "program-auxiliary-values.bin": (688, "44740ace20f4eb49e97e36304d193bf9dc5156d368a568783e975b6612b972af"),
    "program-empty.bin": (288, "7be9194096c7267376d4f70067725f20230a4568c3cb8cae4b6f481d99b8c8fb"),
    "program-explicit-captures.bin": (369, "d6f1eb0018b9e3e74ff5f68193cb333ee7a291708bac6bd0fd8a52eb6d0af970"),
    "program-future-let.bin": (450, "c7abcd73441cbb0702cc2c5b3e9745ebe8aaeddf0bbe336f8ae5437535793227"),
    "program-let-scope.bin": (478, "1715f085a58adb4a76a1599f7e29d32730d2516d2e461470907db123a22da9fa"),
    "program-nested-captures.bin": (715, "a9ed635321c04db37ac680cad55bed8c5e06665d4c3f89dd5b9f297c59f45280"),
    "program-nested-inner.bin": (338, "8bf148cff28f52e499bb5dcae358cd4a02c0ae8e042403fa8768c0330537dd8e"),
    "program-nonutf8-names-block.bin": (869, "1bb69af01495687588e008e2d42e2161bf593b3fd8605a59274b829bf2d7ccdc"),
    "program-numeric-boundaries.bin": (736, "1a639e45bac59fe67c9125f36949dc476b307cc413d5865832348f7690248351"),
    "program-padding.bin": (690, "4e3e049d7c01529e016f2cfacfac6aa165ba7bde554159b4a9e80e75c4ba2537"),
    "program-send.bin": (433, "ee652c1f949275fbe5b99685ae957399cd242d6a5db1e4448c7797fd45b49345"),
    "program-send-arities.bin": (976, "36e282c42e2ee51488650194bb286201979c984d0e8daa34002f34b177d84d5f"),
    "program-signalling-nan.bin": (344, "9546c7f0f182d886a3c75e5518f43d396de9897d3ca1495d9433acd26bbae7ad"),
}
INVENTORIES = {
    "inventory-duplicate-elided.bin": (514, "e555410f8c93763bfd493ce0a61c68dcaa775f20c2a9dfa4a3cdebbdf1e3ffcc"),
    "inventory-empty.bin": (128, "0fe1f4056dbcd5379ab2b2ee62bc28930940e6d19b25ed2cb69eee0615d77351"),
    "inventory-nested-closure.bin": (1277, "f9355fb5a338081f2a41b813f821f2d669d3acedbfcf3bd5025907841a283948"),
    "inventory-one.bin": (514, "e555410f8c93763bfd493ce0a61c68dcaa775f20c2a9dfa4a3cdebbdf1e3ffcc"),
    "inventory-shared-cyclic.bin": (1554, "e4b9de8f3fc899ae908357a3180adadbe77c8cc4e628a411a2f151eba9667324"),
}
BINDINGS = {
    "bindings-block-implementation.bin": (301, "f8edb5918b18c001f7046a94a3edc94a7fae1a3e11c570246ce7aed92cf2526d"),
    "bindings-empty.bin": (128, "a9f34270ec76eef44ccde2463750358bccf149fa6e0d13243297ebb36c127559"),
    "bindings-mixed.bin": (301, "d0f9a5d2b42d226ff07929289597d13ad104dc5c21b6b7ecb282e33c959bfcd0"),
    "bindings-provenance-only-change.bin": (301, "d249bbade6844865e58888fbcffff479ba286a52132727a05be1f4db80f35c75"),
}
SELECTIONS = {
    "selection-block-implementation.bin": (566, "668def22178af86381a48ad60b13fa2ab79dd0838d16929beb0b64c90fc1b7bd"),
    "selection-empty.bin": (224, "324d0a4841ad194a13e9b4265f80e548cd3c9af940856351cad11be0ad0cda5f"),
    "selection-mixed.bin": (566, "edf4886953004e7f1baba07d08dbe351a2beca25794f405d4cb1b9c31eb1930c"),
    "selection-shared-cyclic.bin": (363, "b5d5554a58ead5f26d884c515806a647909a937bb9a7dc0bc6c56f924699680b"),
}
IMAGES = {
    "image-block-binding-stripped.bin": (2617, "7456ffc6a1c20b19839fbe966c5413dc448a3909259196f50fabd4ea7a8f4bc7"),
    "image-empty-stripped.bin": (740, "14612bb39cf197be3f304996431b85534927e19934f6bd79de224e2aa57f6097"),
    "image-mixed-stripped.bin": (2617, "57c23b8e2f848d52403d2d9fe0fdf2bf2fae751276d4dbf477b71ccdbaadb6ac"),
    "image-mixed-code.bin": (2722, "ff1a05c75ab7232c8e5de1b741ff69fc1b81403522417e15b91ef24d274e213e"),
    "image-mixed-facts-stripped.bin": (2694, "652fc3d59db0713f11136dd22c34e9be522ac190a0e9031f934d1960cc6b12a5"),
    "image-shared-cyclic-stripped.bin": (2495, "e6b03ccd40fb8b0a77519dc189f117bd661ebe0a9924e44d884f603db77f954d"),
}


class Reject(Exception):
    def __init__(self, stage: str, detail: str) -> None:
        super().__init__(f"{stage}: {detail}")
        self.stage = stage


def require(condition: bool, stage: str, detail: str) -> None:
    if not condition:
        raise Reject(stage, detail)


def u32(data: bytes, offset: int, stage: str) -> int:
    require(0 <= offset <= len(data) - 4, stage, "truncated u32")
    return struct.unpack_from("<I", data, offset)[0]


def u64(data: bytes, offset: int, stage: str) -> int:
    require(0 <= offset <= len(data) - 8, stage, "truncated u64")
    return struct.unpack_from("<Q", data, offset)[0]


def align8(value: int) -> int:
    return (value + 7) & ~7


def name(data: bytes, stage: str) -> None:
    require(0 < len(data) <= 1 << 20 and b"\0" not in data, stage, "invalid opaque name")


def selector_arity(data: bytes) -> int:
    name(data, "node")
    colons = data.count(b":")
    if colons:
        return colons
    return 0 if (65 <= data[0] <= 90 or 97 <= data[0] <= 122 or data[0] == 95) else 1


def parse_program(data: bytes) -> dict[str, object]:
    aggregate = [0]
    root = None
    stack = [(data, 1)]
    while stack:
        current, depth = stack.pop()
        admitted = parse_program_one(current, depth, aggregate)
        if root is None:
            root = admitted
        stack.extend((nested, depth + 1) for nested in reversed(admitted["nested"]))
    require(root is not None, "prefix", "missing root program")
    return root


def parse_program_one(data: bytes, depth: int, aggregate: list[int]) -> dict[str, object]:
    require(depth <= 4096, "limits", "nested program depth")
    aggregate[0] += len(data)
    require(aggregate[0] <= 64 << 20, "limits", "transitive program bytes")
    require(len(data) >= 288, "prefix", "minimum length")
    require(data[: len(DOMAIN)] == DOMAIN and data[len(DOMAIN) : 32] == bytes(32 - len(DOMAIN)), "prefix", "domain")
    require(
        u32(data, 32, "prefix") == 1
        and u32(data, 36, "prefix") == 64
        and u64(data, 40, "prefix") == len(data)
        and u32(data, 48, "prefix") == 0
        and u32(data, 52, "prefix") == 6
        and u32(data, 56, "prefix") == 32
        and u32(data, 60, "prefix") == 0,
        "prefix",
        "common prefix",
    )
    require(len(data) <= 64 << 20, "limits", "canonical value bytes")

    strides = (32, 24, 16, 40, 4, 1)
    ceilings = (1, 262_144, 262_144, 1_048_576, 4_294_967_295, 32 << 20)
    sections: list[tuple[int, int, int]] = []
    cursor = 256
    for index, (stride, ceiling) in enumerate(zip(strides, ceilings)):
        base = 64 + 32 * index
        count = u32(data, base + 8, "directory")
        require(count <= ceiling, "limits", f"section {index + 1} count")
        aligned = align8(cursor)
        require(aligned <= len(data) and data[cursor:aligned] == bytes(aligned - cursor), "directory", "nonzero padding")
        length = stride * count
        require(
            u32(data, base, "directory") == index + 1
            and u32(data, base + 4, "directory") == stride
            and u32(data, base + 12, "directory") == 0
            and u64(data, base + 16, "directory") == aligned
            and u64(data, base + 24, "directory") == length,
            "directory",
            f"section {index + 1}",
        )
        end = aligned + length
        require(end <= len(data), "directory", "section bounds")
        sections.append((aligned, end, count))
        cursor = end
    require(cursor == len(data), "directory", "trailing or missing bytes")

    metadata = data[sections[0][0] : sections[0][1]]
    semantic, parameters, captures, bindings, statements, node_count, operand_count, reserved = struct.unpack("<8I", metadata)
    require(
        semantic == 1
        and reserved == 0
        and bindings == sections[1][2]
        and statements == sections[2][2]
        and node_count == sections[3][2]
        and operand_count == sections[4][2]
        and parameters <= 4096
        and captures <= 4096
        and parameters + captures <= bindings,
        "metadata",
        "counts",
    )

    binding_bytes = data[sections[1][0] : sections[1][1]]
    statement_bytes = data[sections[2][0] : sections[2][1]]
    node_bytes = data[sections[3][0] : sections[3][1]]
    operand_bytes = data[sections[4][0] : sections[4][1]]
    arena = data[sections[5][0] : sections[5][1]]
    prefix_bindings = parameters + captures
    let_count = bindings - prefix_bindings
    arena_cursor = 0
    previous_definition = -1
    binding_rows = []
    for index in range(bindings):
        kind, ordinal, length, defining = struct.unpack_from("<4I", binding_bytes, index * 24)
        reserved64 = u64(binding_bytes, index * 24 + 16, "binding")
        expected_kind = 1 if index < parameters else 2 if index < prefix_bindings else 3
        expected_ordinal = index if kind == 1 else index - parameters if kind == 2 else index - prefix_bindings
        end = arena_cursor + length
        require(end <= len(arena), "binding", "name bounds")
        value = arena[arena_cursor:end]
        name(value, "binding")
        require(kind == expected_kind and ordinal == expected_ordinal and reserved64 == 0, "binding", "partition or fixed field")
        if kind == 3:
            require(previous_definition < defining < statements, "binding", "let definition order")
            previous_definition = defining
        else:
            require(defining == ABSENT, "binding", "non-let definition")
        binding_rows.append((kind, defining))
        arena_cursor = end

    statement_rows = []
    next_let = 0
    for index in range(statements):
        kind, root, binding, zero = struct.unpack_from("<4I", statement_bytes, index * 16)
        require(kind in (1, 2) and root < node_count and zero == 0, "statement", "tag, root, or reserved")
        if kind == 1:
            require(binding == ABSENT, "statement", "expression binding")
        else:
            expected_binding = prefix_bindings + next_let
            require(binding == expected_binding and binding_rows[binding][1] == index, "statement", "let relationship")
            next_let += 1
        statement_rows.append((kind, root))
    require(next_let == let_count, "statement", "let coverage")

    operands_cursor = 0
    nodes = []
    nested_values = []
    for index in range(node_count):
        kind, flags, owned, length = struct.unpack_from("<4I", node_bytes, index * 40)
        payload = u64(node_bytes, index * 40 + 16, "node")
        auxiliary_0, auxiliary_1 = struct.unpack_from("<2I", node_bytes, index * 40 + 24)
        reserved64 = u64(node_bytes, index * 40 + 32, "node")
        require(flags == 0 and reserved64 == 0, "node", "fixed field")
        operand_end = operands_cursor + owned
        require(operand_end <= operand_count, "node", "operand consumption")
        owned_operands = tuple(u32(operand_bytes, ordinal * 4, "node") for ordinal in range(operands_cursor, operand_end))
        data_end = arena_cursor + length
        require(data_end <= len(arena), "node", "data consumption")
        value = arena[arena_cursor:data_end]
        if kind in (1, 2):
            require(owned == length == auxiliary_0 == auxiliary_1 == 0, "node", "numeric shape")
        elif kind == 3:
            require(owned == 0 and length <= 16 << 20 and payload == auxiliary_0 == auxiliary_1 == 0, "node", "string shape")
        elif kind in (4, 9):
            name(value, "node")
            require(owned == 0 and payload == auxiliary_0 == auxiliary_1 == 0, "node", "name shape")
        elif kind in (5, 6, 7):
            require(owned == length == payload == auxiliary_0 == auxiliary_1 == 0, "node", "singleton shape")
        elif kind == 8:
            require(owned == length == payload == auxiliary_1 == 0 and auxiliary_0 < bindings, "node", "lexical shape")
        elif kind == 10:
            arity = selector_arity(value)
            require(payload == auxiliary_1 == 0 and auxiliary_0 == arity and owned == arity + 1 <= 4096, "node", "send shape")
            require(all(child < index for child in owned_operands), "node", "forward send child")
        elif kind == 11:
            require(len(value) >= 288, "node", "nested program header")
            nested_parameters = u32(value, 260, "node")
            nested_captures = u32(value, 264, "node")
            require(
                payload == 0
                and auxiliary_0 == nested_parameters
                and auxiliary_1 == nested_captures
                and owned == nested_captures <= 4096
                and all(binding < bindings for binding in owned_operands),
                "node",
                "block shape",
            )
            nested_values.append(value)
        else:
            raise Reject("node", "unknown kind")
        nodes.append((kind, auxiliary_0, owned_operands))
        operands_cursor = operand_end
        arena_cursor = data_end
    require(operands_cursor == operand_count and arena_cursor == len(arena), "node", "arena consumption")

    expected = 0
    completed_lets = 0
    for statement_kind, root in statement_rows:
        stack = [(root, False, 1)]
        while stack:
            index, expanded, expression_depth = stack.pop()
            require(expression_depth <= 4096, "limits", "expression depth")
            kind, auxiliary_0, owned_operands = nodes[index]
            in_scope = prefix_bindings + completed_lets
            if kind == 8:
                require(auxiliary_0 < in_scope, "scope", "lexical read")
            if kind == 11:
                require(all(binding < in_scope for binding in owned_operands), "scope", "block capture")
            if expanded:
                require(index == expected, "tree", "postorder or ownership")
                expected += 1
            else:
                stack.append((index, True, expression_depth))
                if kind == 10:
                    stack.extend((child, False, expression_depth + 1) for child in reversed(owned_operands))
        completed_lets += statement_kind == 2
    require(expected == node_count, "tree", "orphan node")
    return {
        "parameters": parameters,
        "captures": captures,
        "bindings": bindings,
        "nodes": nodes,
        "nested": tuple(nested_values),
    }


def parse_inventory(data: bytes, roots: tuple[bytes, ...] | None = None) -> tuple[bytes, ...]:
    domain = b"birnam.portable-inventory.v1\0"
    require(len(data) >= 128 and len(data) <= 64 << 20, "inventory-prefix", "length")
    require(data[: len(domain)] == domain and data[len(domain) : 32] == bytes(32 - len(domain)), "inventory-prefix", "domain")
    require(
        u32(data, 32, "inventory-prefix") == 1
        and u32(data, 36, "inventory-prefix") == 64
        and u64(data, 40, "inventory-prefix") == len(data)
        and u32(data, 48, "inventory-prefix") == 0
        and u32(data, 52, "inventory-prefix") == 2
        and u32(data, 56, "inventory-prefix") == 32
        and u32(data, 60, "inventory-prefix") == 0,
        "inventory-prefix",
        "common prefix",
    )
    rows = u32(data, 72, "inventory-directory")
    program_bytes = u32(data, 104, "inventory-directory")
    require(rows <= 262_144, "limits", "inventory rows")
    rows_end = 128 + 48 * rows
    programs_offset = align8(rows_end)
    require(
        u32(data, 64, "inventory-directory") == 1
        and u32(data, 68, "inventory-directory") == 48
        and u32(data, 76, "inventory-directory") == 0
        and u64(data, 80, "inventory-directory") == 128
        and u64(data, 88, "inventory-directory") == 48 * rows
        and u32(data, 96, "inventory-directory") == 2
        and u32(data, 100, "inventory-directory") == 1
        and u32(data, 108, "inventory-directory") == 0
        and u64(data, 112, "inventory-directory") == programs_offset
        and u64(data, 120, "inventory-directory") == program_bytes
        and data[rows_end:programs_offset] == bytes(programs_offset - rows_end)
        and programs_offset + program_bytes == len(data),
        "inventory-directory",
        "sections",
    )
    arena = data[programs_offset:]
    cursor = 0
    previous = None
    values = []
    for index in range(rows):
        base = 128 + 48 * index
        digest = data[base : base + 32]
        length, flags, parameters, captures = struct.unpack_from("<4I", data, base + 32)
        end = cursor + length
        require(0 < length and end <= len(arena), "inventory-row", "program bounds")
        value = arena[cursor:end]
        admitted = parse_program(value)
        key = (digest, value)
        require(
            flags == 0
            and parameters == admitted["parameters"]
            and captures == admitted["captures"]
            and hashlib.sha256(value).digest() == digest
            and (previous is None or previous < key),
            "inventory-row",
            "digest, counts, flags, or order",
        )
        previous = key
        values.append(value)
        cursor = end
    require(cursor == len(arena), "inventory-row", "program consumption")
    if roots is not None:
        expected = set()
        pending = list(roots)
        while pending:
            value = pending.pop()
            if value in expected:
                continue
            expected.add(value)
            pending.extend(parse_program(value)["nested"])
        require(set(values) == expected, "inventory-closure", "missing or unreachable program")
    return tuple(values)


def parse_field(data: bytes) -> tuple[int, bytes | None]:
    require(len(data) >= 1, "binding-field", "empty field")
    tag = data[0]
    if tag in (0, 1, 2):
        require(len(data) == 1, "binding-field", "singleton length")
        return tag, None
    if tag == 7:
        require(len(data) == 5, "binding-field", "heap length")
        return tag, None
    require(tag in (5, 9) and len(data) >= 5, "binding-field", "tag")
    length = u32(data, 1, "binding-field")
    value = data[5:]
    require(len(value) == length, "binding-field", "length-delimited field")
    if tag in (5, 9):
        name(value, "binding-field")
    return tag, value


def parse_bindings(data: bytes) -> tuple[bytes, tuple[tuple[bytes, int, bytes], ...]]:
    domain = b"birnam.code-bindings.v2\0"
    require(len(data) >= 128 and data[: len(domain)] == domain, "bindings-prefix", "domain or length")
    require(
        data[len(domain) : 32] == bytes(32 - len(domain))
        and u32(data, 32, "bindings-prefix") == 1
        and u32(data, 36, "bindings-prefix") == 64
        and u64(data, 40, "bindings-prefix") == len(data)
        and u32(data, 48, "bindings-prefix") == 0
        and u32(data, 52, "bindings-prefix") == 2
        and u32(data, 56, "bindings-prefix") == 32
        and u32(data, 60, "bindings-prefix") == 0,
        "bindings-prefix",
        "common prefix",
    )
    count = u32(data, 72, "bindings-directory")
    field_bytes = u32(data, 104, "bindings-directory")
    require(count <= 262_144, "limits", "binding count")
    rows_end = 128 + count * 32
    fields_offset = align8(rows_end)
    require(
        u32(data, 64, "bindings-directory") == 1
        and u32(data, 68, "bindings-directory") == 32
        and u32(data, 76, "bindings-directory") == 0
        and u64(data, 80, "bindings-directory") == 128
        and u64(data, 88, "bindings-directory") == count * 32
        and u32(data, 96, "bindings-directory") == 2
        and u32(data, 100, "bindings-directory") == 1
        and u32(data, 108, "bindings-directory") == 0
        and u64(data, 112, "bindings-directory") == fields_offset
        and u64(data, 120, "bindings-directory") == field_bytes
        and data[rows_end:fields_offset] == bytes(fields_offset - rows_end)
        and fields_offset + field_bytes == len(data),
        "bindings-directory",
        "sections",
    )
    cursor = fields_offset
    previous = None
    digests = []
    result = []
    for index in range(count):
        base = 128 + index * 32
        lengths = struct.unpack_from("<4I", data, base)
        arity, flags = struct.unpack_from("<2I", data, base + 16)
        reserved = u64(data, base + 24, "bindings-row")
        encoded = []
        for length in lengths:
            end = cursor + length
            require(end <= len(data), "bindings-row", "field bounds")
            encoded.append(data[cursor:end])
            cursor = end
        target, selector, implementation, provenance = encoded
        target_tag, _ = parse_field(target)
        selector_tag, selector_value = parse_field(selector)
        implementation_tag, _ = parse_field(implementation)
        provenance_tag, _ = parse_field(provenance)
        require(selector_value is not None, "bindings-row", "selector bytes")
        derived_arity = selector_arity(selector_value)
        key = (target, selector_value)
        require(
            target_tag in (0, 1, 2, 5, 7, 9)
            and selector_tag == 5
            and implementation_tag in (0, 1, 2, 5, 7, 9)
            and provenance_tag in (0, 7, 9)
            and arity == derived_arity
            and flags == reserved == 0
            and (previous is None or previous < key),
            "bindings-row",
            "shape, arity, or order",
        )
        digest = hashlib.sha256(
            b"birnam.code-binding-row.v2\0"
            + struct.pack("<5I", index, len(target), len(selector), len(implementation), arity)
            + target
            + selector
            + implementation
        ).digest()
        previous = key
        digests.append(digest)
        result.append((selector_value, arity, digest))
    require(cursor == len(data), "bindings-row", "field consumption")
    semantics = hashlib.sha256(
        b"birnam.code-binding-semantics.v2\0" + struct.pack("<I", count) + b"".join(digests)
    ).digest()
    return semantics, tuple(result)


def parse_selection(data: bytes, program_inventory: bytes, binding_inventory: bytes) -> None:
    programs = parse_inventory(program_inventory)
    binding_digest, bindings = parse_bindings(binding_inventory)
    domain = b"birnam.code-selection.v1\0"
    require(len(data) >= 224 and data[: len(domain)] == domain, "selection-prefix", "domain or length")
    require(
        data[len(domain) : 32] == bytes(32 - len(domain))
        and u32(data, 32, "selection-prefix") == 1
        and u32(data, 36, "selection-prefix") == 64
        and u64(data, 40, "selection-prefix") == len(data)
        and u32(data, 48, "selection-prefix") == 0
        and u32(data, 52, "selection-prefix") == 3
        and u32(data, 56, "selection-prefix") == 32
        and u32(data, 60, "selection-prefix") == 0,
        "selection-prefix",
        "common prefix",
    )
    count = u32(data, 104, "selection-directory")
    selector_bytes = u32(data, 136, "selection-directory")
    selector_offset = 224 + count * 64
    require(count == len(bindings) + len(programs) and count <= 524_288, "selection-coverage", "row count")
    require(
        u32(data, 64, "selection-directory") == 1
        and u32(data, 68, "selection-directory") == 64
        and u32(data, 72, "selection-directory") == 1
        and u32(data, 76, "selection-directory") == 0
        and u64(data, 80, "selection-directory") == 160
        and u64(data, 88, "selection-directory") == 64
        and u32(data, 96, "selection-directory") == 2
        and u32(data, 100, "selection-directory") == 64
        and u32(data, 108, "selection-directory") == 0
        and u64(data, 112, "selection-directory") == 224
        and u64(data, 120, "selection-directory") == count * 64
        and u32(data, 128, "selection-directory") == 3
        and u32(data, 132, "selection-directory") == 1
        and u32(data, 140, "selection-directory") == 0
        and u64(data, 144, "selection-directory") == selector_offset
        and u64(data, 152, "selection-directory") == selector_bytes
        and selector_offset + selector_bytes == len(data),
        "selection-directory",
        "sections",
    )
    require(
        data[160:192] == hashlib.sha256(program_inventory).digest()
        and data[192:224] == binding_digest,
        "selection-metadata",
        "inventory authority",
    )
    selector_cursor = selector_offset
    for index in range(count):
        base = 224 + index * 64
        kind, disposition, subject, arity, captures, length, flags, reserved = struct.unpack_from("<8I", data, base)
        expected_kind = 1 if index < len(bindings) else 2
        expected_subject = index if expected_kind == 1 else index - len(bindings)
        if expected_kind == 1:
            expected_selector, expected_arity, digest = bindings[expected_subject]
            expected_captures = 0
        else:
            program = parse_program(programs[expected_subject])
            expected_arity = program["parameters"]
            expected_captures = program["captures"]
            expected_selector = b"value" if expected_arity == 0 else b"value:" * expected_arity
            digest = hashlib.sha256(programs[expected_subject]).digest()
        end = selector_cursor + length
        require(
            kind == expected_kind
            and disposition in (1, 2)
            and subject == expected_subject
            and arity == expected_arity
            and captures == expected_captures
            and flags == reserved == 0
            and data[base + 32 : base + 64] == digest
            and data[selector_cursor:end] == expected_selector,
            "selection-row",
            "coverage or row agreement",
        )
        selector_cursor = end
    require(selector_cursor == len(data), "selection-row", "selector consumption")


def image_field(data: bytes, offset: int) -> tuple[tuple[int, int | None, bytes | None], int]:
    require(offset < len(data), "image-field", "tag")
    tag = data[offset]
    offset += 1
    require(tag != 10, "image-field", "legacy tag 10")
    if tag in (0, 1, 2, 8):
        return (tag, None, None), offset
    if tag in (3, 13, 14, 15):
        require(offset + 8 <= len(data), "image-field", "numeric")
        return (tag, None, None), offset + 8
    if tag in (7, 11, 12):
        require(offset + 4 <= len(data), "image-field", "index")
        return (tag, u32(data, offset, "image-field"), None), offset + 4
    require(tag in (5, 9, 16), "image-field", "unknown tag")
    length = u32(data, offset, "image-field")
    offset += 4
    end = offset + length
    require(end <= len(data), "image-field", "bytes")
    value = data[offset:end]
    if tag in (5, 9):
        name(value, "image-field")
    return (tag, None, value), end


def binding_heap_references(data: bytes) -> tuple[int, ...]:
    count = u32(data, 72, "bindings-directory")
    cursor = u64(data, 112, "bindings-directory")
    refs = []
    for index in range(count):
        for length in struct.unpack_from("<4I", data, 128 + index * 32):
            encoded = data[cursor : cursor + length]
            field_value, end = image_field(encoded, 0)
            require(end == len(encoded), "image-field", "binding field suffix")
            if field_value[0] == 7:
                refs.append(field_value[1])
            cursor += length
    return tuple(refs)


def parse_heap(payloads: list[bytes], programs: bytes, bindings: bytes) -> tuple[int, tuple[bool, ...]]:
    program_values = parse_inventory(programs)
    objects = []
    for payload in payloads:
        count = u32(payload, 0, "heap")
        offset = 4
        for _ in range(count):
            locator, offset = image_field(payload, offset)
            require(locator[0] in (9, 12, 13, 14, 15, 16), "heap", "locator")
            words = u32(payload, offset, "heap")
            offset += 4
            require(locator[0] not in (13, 14, 15, 16) or words == 0, "heap", "constructor words")
            slots = []
            for _ in range(words):
                value, offset = image_field(payload, offset)
                slots.append(value)
            objects.append((locator, slots))
        require(offset == len(payload), "heap", "payload consumption")
    require(len(objects) <= 1_048_576, "limits", "heap objects")
    code_programs = [None] * len(objects)
    roots = []
    for index, (locator, slots) in enumerate(objects):
        code_object = locator[0] == 9 and locator[2] == b"CodeObjectVt"
        if code_object:
            require(
                len(slots) == 2
                and slots[0][0] == 11
                and slots[0][1] is not None
                and slots[0][1] < len(program_values)
                and slots[1][0] in (0, 7, 9),
                "heap",
                "CodeObject payload",
            )
            code_programs[index] = slots[0][1]
            roots.append(program_values[slots[0][1]])
        else:
            require(
                all(field_value[0] not in (4, 6, 11, 13, 14, 15, 16) for field_value in slots),
                "heap",
                "field placement",
            )
        for field_value in (locator, *slots):
            if field_value[0] in (7, 12):
                require(field_value[1] is not None and field_value[1] < len(objects), "heap", "reference range")
    parse_inventory(programs, tuple(roots))
    require(all(reference is not None and reference < len(objects) for reference in binding_heap_references(bindings)), "heap", "binding HeapRef")
    for locator, slots in objects:
        for field_value in (locator, *slots):
            if field_value[0] == 12:
                require(
                    field_value[1] is not None and code_programs[field_value[1]] is not None,
                    "heap",
                    "BlockVtableRef target",
                )
    for locator, slots in objects:
        if locator[0] != 12:
            continue
        code_index = locator[1]
        require(code_index is not None and code_programs[code_index] is not None, "heap", "block locator target")
        program = parse_program(program_values[code_programs[code_index]])
        require(
            len(slots) == program["captures"] + 1
            and slots[0][0] == 7
            and slots[0][1] == code_index,
            "heap",
            "portable Block payload",
        )
    return len(objects), tuple(value is not None for value in code_programs)


def parse_facts(payload: bytes, heap_count: int, code_objects: tuple[bool, ...]) -> None:
    require(len(payload) >= 36, "image-facts", "header")
    basis = u64(payload, 0, "image-facts")
    floor = u64(payload, 8, "image-facts")
    policy = payload[16]
    argument = u64(payload, 24, "image-facts")
    count = u32(payload, 32, "image-facts")
    require(
        floor <= basis
        and payload[17:24] == bytes(7)
        and policy in (0, 1, 2)
        and (policy != 0 or argument == 0)
        and (policy != 1 or argument <= basis),
        "image-facts",
        "retention metadata",
    )
    offset = 36
    for _ in range(count):
        require(offset + 10 <= len(payload), "image-facts", "fact prefix")
        require(payload[offset] in (0, 1) and payload[offset + 1] in (0, 1), "image-facts", "fact flags")
        offset += 10
        for _ in range(3):
            field_value, offset = image_field(payload, offset)
            require(field_value[0] in (0, 1, 2, 5, 7, 8, 9, 12), "image-facts", "field placement")
            if field_value[0] in (7, 12):
                require(field_value[1] is not None and field_value[1] < heap_count, "image-facts", "reference")
            if field_value[0] == 12:
                require(code_objects[field_value[1]], "image-facts", "BlockVtableRef target")
    require(offset == len(payload), "image-facts", "payload consumption")


WYHASH_SECRET = (0xA0761D6478BD642F, 0xE7037ED1A0B428DB, 0x8EBC6AF09C88C6E3, 0x589965CC75374CC3)


def wymix(left: int, right: int) -> int:
    product = left * right
    return ((product & ((1 << 64) - 1)) ^ (product >> 64)) & ((1 << 64) - 1)


def wyhash(value: bytes) -> int:
    state = [wymix(WYHASH_SECRET[0], WYHASH_SECRET[1])] * 3
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
    return wymix((product & ((1 << 64) - 1)) ^ WYHASH_SECRET[0] ^ len(value), (product >> 64) ^ WYHASH_SECRET[1])


def parse_image(data: bytes) -> tuple[int, bool]:
    require(
        len(data) >= 28
        and data[:8] == b"BIRNAMIM"
        and u32(data, 8, "image-header") == 8
        and u32(data, 12, "image-header") == 0x01020304
        and u32(data, 16, "image-header") == 1
        and u64(data, 20, "image-header") == 0xCDB641F978540D45,
        "image-header",
        "header",
    )
    offset = 28

    def record(expected: int) -> bytes:
        nonlocal offset
        require(u32(data, offset, "image-record") == expected, "image-record", "kind/order")
        length = u64(data, offset + 4, "image-record")
        start = offset + 12
        end = start + length
        require(end <= len(data), "image-record", "payload bounds")
        offset = end
        return data[start:end]

    authority = record(5)
    require(len(authority) == 64 and all(byte in b"0123456789abcdef" for byte in authority), "image-authority", "spelling")

    def hashed(expected: int) -> tuple[bytes, bytes]:
        payload = record(expected)
        require(len(payload) >= 40, "image-record", "hashed payload")
        digest = payload[:32]
        length = u64(payload, 32, "image-record")
        value = payload[40:]
        require(len(value) == length and hashlib.sha256(value).digest() == digest, "image-record", "hash or length")
        return digest, value

    programs_digest, programs = hashed(7)
    _, bindings = hashed(8)
    selection_digest, selection = hashed(9)
    binding_digest, _ = parse_bindings(bindings)
    parse_selection(selection, programs, bindings)
    expected_authority = hashlib.sha256(
        b"birnam.portable-image-authority.v1\0" + programs_digest + binding_digest + selection_digest
    ).hexdigest().encode("ascii")
    require(authority == expected_authority, "image-authority", "derivation")
    heaps = []
    facts = []
    code = None
    while offset < len(data):
        kind = u32(data, offset, "image-record")
        if kind == 3 and not facts and code is None:
            heaps.append(record(3))
        elif kind == 2 and code is None:
            facts.append(record(2))
        elif kind == 4 and code is None:
            code = record(4)
            require(offset == len(data), "image-record", "Code not final")
        else:
            raise Reject("image-record", "order or unknown kind")
    heap_count, code_objects = parse_heap(heaps, programs, bindings)
    for fact_set in facts:
        parse_facts(fact_set, heap_count, code_objects)
    if code is not None:
        require(
            len(code) >= 73
            and code[8:72] == authority
            and u64(code, 0, "image-code") == wyhash(code[72:]),
            "image-code",
            "checksum or authority",
        )
    return heap_count, code is not None


def generated_boundary_vectors() -> dict[str, bytes]:
    def encode(
        parameters: tuple[bytes, ...],
        statement_roots: tuple[int, ...],
        nodes: tuple[tuple[int, int, int, int, tuple[int, ...], bytes], ...],
    ) -> bytes:
        operands = tuple(child for node in nodes for child in node[4])
        arena = b"".join(parameters) + b"".join(node[5] for node in nodes)
        bindings_offset = 288
        statements_offset = bindings_offset + 24 * len(parameters)
        nodes_offset = statements_offset + 16 * len(statement_roots)
        operands_offset = nodes_offset + 40 * len(nodes)
        bytes_offset = align8(operands_offset + 4 * len(operands))
        total = bytes_offset + len(arena)

        def section(kind: int, stride: int, count: int, offset: int) -> bytes:
            return struct.pack("<IIIIQQ", kind, stride, count, 0, offset, stride * count)

        out = bytearray(DOMAIN)
        out.extend(bytes(32 - len(out)))
        out.extend(struct.pack("<IIQIIII", 1, 64, total, 0, 6, 32, 0))
        for entry in (
            section(1, 32, 1, 256),
            section(2, 24, len(parameters), bindings_offset),
            section(3, 16, len(statement_roots), statements_offset),
            section(4, 40, len(nodes), nodes_offset),
            section(5, 4, len(operands), operands_offset),
            section(6, 1, len(arena), bytes_offset),
        ):
            out.extend(entry)
        out.extend(
            struct.pack(
                "<8I",
                1,
                len(parameters),
                0,
                len(parameters),
                len(statement_roots),
                len(nodes),
                len(operands),
                0,
            )
        )
        for ordinal, value in enumerate(parameters):
            out.extend(struct.pack("<IIIIQ", 1, ordinal, len(value), ABSENT, 0))
        for root in statement_roots:
            out.extend(struct.pack("<IIII", 1, root, ABSENT, 0))
        for kind, payload, auxiliary_0, auxiliary_1, owned, value in nodes:
            out.extend(
                struct.pack(
                    "<IIIIQIIQ",
                    kind,
                    0,
                    len(owned),
                    len(value),
                    payload,
                    auxiliary_0,
                    auxiliary_1,
                    0,
                )
            )
        for child in operands:
            out.extend(struct.pack("<I", child))
        out.extend(bytes(bytes_offset - len(out)))
        out.extend(arena)
        require(len(out) == total, "boundary", "encoder length")
        return bytes(out)

    one_mib = 1 << 20
    sixteen_mib = 16 << 20
    return {
        "program-max-individual-lengths.bin": encode(
            (b"n" * one_mib,),
            (0, 1, 2, 5),
            (
                (3, 0, 0, 0, (), b"\x00" * sixteen_mib),
                (4, 0, 0, 0, (), b"s" * one_mib),
                (9, 0, 0, 0, (), b"g" * one_mib),
                (1, 0, 0, 0, (), b""),
                (1, 1, 0, 0, (), b""),
                (10, 0, 1, 0, (3, 4), b"+" * one_mib),
            ),
        ),
        "program-max-bytes-section.bin": encode(
            (),
            (0, 1),
            (
                (3, 0, 0, 0, (), b"a" * sixteen_mib),
                (3, 0, 0, 0, (), b"b" * sixteen_mib),
            ),
        ),
    }


def boundary_manifest() -> dict[str, tuple[int, str, str]]:
    rows = {}
    for line in (HERE / "BOUNDARY_VECTORS").read_text(encoding="ascii").splitlines():
        if not line or line.startswith("#"):
            continue
        length, digest, filename, recipe = line.split(maxsplit=3)
        rows[filename] = (int(length), digest, recipe)
    return rows


def mutation(source: bytes, offset: int, replacement: bytes) -> bytes:
    return source[:offset] + replacement + source[offset + len(replacement) :]


def rejects_at(label: str, data: bytes, stage: str) -> None:
    try:
        parse_program(data)
    except Reject as error:
        require(error.stage == stage, "test", f"{label}: expected {stage}, got {error.stage}")
        return
    raise AssertionError(f"{label}: mutation accepted")


def rejects_with(label: str, parser, data: bytes, stage: str) -> int:
    try:
        parser(data)
    except Reject as error:
        require(error.stage == stage, "test", f"{label}: expected {stage}, got {error.stage}")
        return 1
    raise AssertionError(f"{label}: mutation accepted")


def reject_all_truncations(data: bytes, parser, valid_prefixes: tuple[bytes, ...] = ()) -> int:
    rejected = 0
    for end in range(len(data)):
        try:
            parser(data[:end])
        except Reject:
            rejected += 1
            continue
        if data[:end] in valid_prefixes:
            continue
        raise AssertionError(f"truncation at {end} of {len(data)} bytes accepted")
    return rejected


def main() -> None:
    loaded = {}
    for filename, (length, digest) in FIXTURES.items():
        data = (HERE / filename).read_bytes()
        require(len(data) == length, "fixture", f"{filename} length")
        require(hashlib.sha256(data).hexdigest() == digest, "fixture", f"{filename} digest")
        loaded[filename] = data
        parse_program(data)

    generated = generated_boundary_vectors()
    generated_manifest = boundary_manifest()
    require(generated.keys() == generated_manifest.keys(), "boundary", "manifest names")
    for filename, data in generated.items():
        length, digest, _ = generated_manifest[filename]
        require(len(data) == length, "boundary", f"{filename} length")
        require(hashlib.sha256(data).hexdigest() == digest, "boundary", f"{filename} digest")
        parse_program(data)
    fake_digest = bytes((0x5A,)) * 32
    require(
        sorted(((fake_digest, b"b"), (fake_digest, b"a")))
        == [(fake_digest, b"a"), (fake_digest, b"b")],
        "boundary",
        "equal-digest byte ordering",
    )

    inventories = {}
    for filename, (length, digest) in INVENTORIES.items():
        data = (HERE / filename).read_bytes()
        require(len(data) == length, "fixture", f"{filename} length")
        require(hashlib.sha256(data).hexdigest() == digest, "fixture", f"{filename} digest")
        inventories[filename] = data
        parse_inventory(data)
    binding_values = {}
    for filename, (length, digest) in BINDINGS.items():
        data = (HERE / filename).read_bytes()
        require(len(data) == length, "fixture", f"{filename} length")
        require(hashlib.sha256(data).hexdigest() == digest, "fixture", f"{filename} digest")
        binding_values[filename] = data
        parse_bindings(data)
    selection_values = {}
    for filename, (length, digest) in SELECTIONS.items():
        data = (HERE / filename).read_bytes()
        require(len(data) == length, "fixture", f"{filename} length")
        require(hashlib.sha256(data).hexdigest() == digest, "fixture", f"{filename} digest")
        selection_values[filename] = data
    image_values = {}
    for filename, (length, digest) in IMAGES.items():
        data = (HERE / filename).read_bytes()
        require(len(data) == length, "fixture", f"{filename} length")
        require(hashlib.sha256(data).hexdigest() == digest, "fixture", f"{filename} digest")
        image_values[filename] = data

    nan = loaded["program-signalling-nan.bin"]
    require(u64(nan, 320, "fixture") == 0x7FF0_0000_0000_0042, "fixture", "signalling NaN bits")
    send = loaded["program-send.bin"]
    padding = loaded["program-padding.bin"]
    future = loaded["program-future-let.bin"]
    nested = loaded["program-nested-captures.bin"]
    auxiliary = loaded["program-auxiliary-values.bin"]
    nonutf8 = parse_program(loaded["program-nonutf8-names-block.bin"])
    require(
        nonutf8["parameters"] == 1
        and nonutf8["captures"] == 1
        and len(nonutf8["nested"]) == 1
        and parse_program(nonutf8["nested"][0])["nodes"][-1][0] == 10,
        "fixture",
        "non-UTF-8 duplicate names and nonempty nested block",
    )
    rejects_at("domain", mutation(send, 0, b"c"), "prefix")
    rejects_at("total length", mutation(send, 40, struct.pack("<Q", len(send) + 1)), "prefix")
    rejects_at("directory reserved", mutation(send, 76, b"\x01"), "directory")
    rejects_at("padding", mutation(padding, 372, b"\x01"), "directory")
    rejects_at("selector arity", mutation(send, 408, struct.pack("<I", 0)), "node")
    rejects_at("shared child", mutation(send, 428, struct.pack("<I", 0)), "tree")
    rejects_at("future let scope", mutation(future, 392, struct.pack("<I", 1)), "scope")
    rejects_at("nested capture count", mutation(nested, 356, struct.pack("<I", 1)), "node")
    rejects_at("embedded NUL name", mutation(auxiliary, 683, b"\0"), "node")
    rejects_at("fixed-zero field", mutation(nan, 308, b"\x01"), "node")
    rejects_at("node count ceiling", mutation(send, 168, struct.pack("<I", 1_048_577)), "limits")
    explicit = loaded["program-explicit-captures.bin"]
    statement_offset = u64(send, 144, "test")
    node_offset = u64(send, 176, "test")
    operand_offset = u64(send, 208, "test")
    binding_offset = u64(explicit, 112, "test")
    nested_data_offset = u64(nested, 240, "test") + 1
    additional_mutations = (
        ("program version", mutation(send, 32, struct.pack("<I", 2)), "prefix"),
        ("program flags", mutation(send, 48, struct.pack("<I", 1)), "prefix"),
        ("directory kind", mutation(send, 64, struct.pack("<I", 7)), "directory"),
        ("directory stride", mutation(send, 68, struct.pack("<I", 31)), "directory"),
        ("metadata row count", mutation(send, 72, struct.pack("<I", 2)), "limits"),
        ("directory offset", mutation(send, 80, struct.pack("<Q", 264)), "directory"),
        ("directory length", mutation(send, 88, struct.pack("<Q", 31)), "directory"),
        ("program suffix", send + b"\0", "prefix"),
        ("statement tag", mutation(send, statement_offset, struct.pack("<I", 3)), "statement"),
        ("statement reserved", mutation(send, statement_offset + 12, b"\x01"), "statement"),
        ("unknown node", mutation(send, node_offset, struct.pack("<I", 12)), "node"),
        ("node flags", mutation(send, node_offset + 4, b"\x01"), "node"),
        ("node reserved", mutation(send, node_offset + 32, b"\x01"), "node"),
        ("forward operand", mutation(send, operand_offset, struct.pack("<I", 2)), "node"),
        ("orphan node", mutation(send, statement_offset + 4, struct.pack("<I", 1)), "tree"),
        ("empty binding name", mutation(explicit, binding_offset + 8, struct.pack("<I", 0)), "binding"),
        ("binding reserved", mutation(explicit, binding_offset + 16, b"\x01"), "binding"),
        ("nested program domain", mutation(nested, nested_data_offset, b"c"), "prefix"),
        ("bytes section ceiling", mutation(send, 232, struct.pack("<I", (32 << 20) + 1)), "limits"),
    )
    additional_mutation_count = sum(
        rejects_with(label, parse_program, changed, stage)
        for label, changed, stage in additional_mutations
    )
    inventory = inventories["inventory-nested-closure.bin"]
    require(
        inventories["inventory-one.bin"] == inventories["inventory-duplicate-elided.bin"]
        and len(parse_inventory(inventories["inventory-one.bin"])) == 1,
        "fixture",
        "duplicate program elision",
    )
    outer = loaded["program-nested-captures.bin"]
    parse_inventory(inventory, (outer,))
    try:
        parse_inventory(inventories["inventory-empty.bin"], (outer,))
    except Reject as error:
        require(error.stage == "inventory-closure", "test", "missing closure stage")
    else:
        raise AssertionError("missing inventory closure accepted")
    corrupted_row = mutation(inventory, 128, bytes([inventory[128] ^ 1]))
    try:
        parse_inventory(corrupted_row)
    except Reject as error:
        require(error.stage == "inventory-row", "test", "inventory digest stage")
    else:
        raise AssertionError("corrupt inventory digest accepted")
    one_inventory = inventories["inventory-one.bin"]
    one_program = parse_inventory(one_inventory)[0]
    duplicate_inventory = bytearray(one_inventory[:128])
    duplicate_total = 224 + 2 * len(one_program)
    struct.pack_into("<Q", duplicate_inventory, 40, duplicate_total)
    struct.pack_into("<I", duplicate_inventory, 72, 2)
    struct.pack_into("<Q", duplicate_inventory, 88, 96)
    struct.pack_into("<I", duplicate_inventory, 104, 2 * len(one_program))
    struct.pack_into("<Q", duplicate_inventory, 112, 224)
    struct.pack_into("<Q", duplicate_inventory, 120, 2 * len(one_program))
    duplicate_inventory.extend(one_inventory[128:176])
    duplicate_inventory.extend(one_inventory[128:176])
    duplicate_inventory.extend(one_program)
    duplicate_inventory.extend(one_program)
    inventory_values = parse_inventory(inventory)
    unsorted_inventory = bytearray(inventory[:128])
    unsorted_inventory.extend(inventory[176:224])
    unsorted_inventory.extend(inventory[128:176])
    unsorted_inventory.extend(inventory_values[1])
    unsorted_inventory.extend(inventory_values[0])
    inventory_mutations = (
        ("inventory version", mutation(inventory, 32, struct.pack("<I", 2)), "inventory-prefix"),
        ("inventory suffix", inventory + b"\0", "inventory-prefix"),
        ("inventory row ceiling", mutation(inventory, 72, struct.pack("<I", 262_145)), "limits"),
        ("inventory directory kind", mutation(inventory, 64, struct.pack("<I", 3)), "inventory-directory"),
        ("inventory directory stride", mutation(inventory, 68, struct.pack("<I", 47)), "inventory-directory"),
        ("inventory row flags", mutation(inventory, 164, b"\x01"), "inventory-row"),
        ("inventory parameter count", mutation(inventory, 168, struct.pack("<I", 99)), "inventory-row"),
        ("inventory zero length", mutation(one_inventory, 160, struct.pack("<I", 0)), "inventory-row"),
        ("inventory exact duplicate", bytes(duplicate_inventory), "inventory-row"),
        ("inventory reordered rows", bytes(unsorted_inventory), "inventory-row"),
    )
    inventory_mutation_count = sum(
        rejects_with(label, parse_inventory, changed, stage)
        for label, changed, stage in inventory_mutations
    )
    mixed_bindings = binding_values["bindings-mixed.bin"]
    block_bindings = binding_values["bindings-block-implementation.bin"]
    changed_bindings = binding_values["bindings-provenance-only-change.bin"]
    require(
        parse_bindings(mixed_bindings)[0] == parse_bindings(changed_bindings)[0],
        "test",
        "provenance changed semantics",
    )
    require(
        1 in binding_heap_references(block_bindings)
        and 7 in binding_heap_references(mixed_bindings)
        and parse_bindings(block_bindings)[0] != parse_bindings(mixed_bindings)[0],
        "fixture",
        "Block and non-Block implementation references",
    )
    try:
        parse_bindings(mutation(mixed_bindings, 144, struct.pack("<I", 1)))
    except Reject as error:
        require(error.stage == "bindings-row", "test", "binding arity stage")
    else:
        raise AssertionError("wrong binding arity accepted")
    fields_offset = u64(mixed_bindings, 112, "test")
    first_field_bytes = sum(struct.unpack_from("<4I", mixed_bindings, 128))
    duplicate_bindings = bytearray(mixed_bindings[:128])
    duplicate_binding_total = 192 + 2 * first_field_bytes
    struct.pack_into("<Q", duplicate_bindings, 40, duplicate_binding_total)
    struct.pack_into("<I", duplicate_bindings, 72, 2)
    struct.pack_into("<Q", duplicate_bindings, 88, 64)
    struct.pack_into("<I", duplicate_bindings, 104, 2 * first_field_bytes)
    struct.pack_into("<Q", duplicate_bindings, 112, 192)
    struct.pack_into("<Q", duplicate_bindings, 120, 2 * first_field_bytes)
    duplicate_bindings.extend(mixed_bindings[128:160])
    duplicate_bindings.extend(mixed_bindings[128:160])
    duplicate_bindings.extend(mixed_bindings[fields_offset : fields_offset + first_field_bytes])
    duplicate_bindings.extend(mixed_bindings[fields_offset : fields_offset + first_field_bytes])
    row_fields = []
    cursor = fields_offset
    for index in range(3):
        length = sum(struct.unpack_from("<4I", mixed_bindings, 128 + 32 * index))
        row_fields.append(mixed_bindings[cursor : cursor + length])
        cursor += length
    reordered_bindings = bytearray(mixed_bindings[:128])
    for index in (1, 0, 2):
        reordered_bindings.extend(mixed_bindings[128 + 32 * index : 160 + 32 * index])
    for index in (1, 0, 2):
        reordered_bindings.extend(row_fields[index])
    binding_mutations = (
        ("bindings version", mutation(mixed_bindings, 32, struct.pack("<I", 2)), "bindings-prefix"),
        ("bindings suffix", mixed_bindings + b"\0", "bindings-prefix"),
        ("bindings row ceiling", mutation(mixed_bindings, 72, struct.pack("<I", 262_145)), "limits"),
        ("bindings directory kind", mutation(mixed_bindings, 64, struct.pack("<I", 3)), "bindings-directory"),
        ("bindings row flags", mutation(mixed_bindings, 148, b"\x01"), "bindings-row"),
        ("bindings row reserved", mutation(mixed_bindings, 152, b"\x01"), "bindings-row"),
        ("bindings unknown target field", mutation(mixed_bindings, fields_offset, b"\x0a"), "binding-field"),
        ("bindings selector field kind", mutation(mixed_bindings, 225, b"\x06"), "binding-field"),
        ("bindings selector embedded NUL", mutation(mixed_bindings, 230, b"\0"), "binding-field"),
        ("bindings forbidden provenance", mutation(mixed_bindings, 300, b"\x05"), "binding-field"),
        ("bindings duplicate key", bytes(duplicate_bindings), "bindings-row"),
        ("bindings reordered rows", bytes(reordered_bindings), "bindings-row"),
    )
    binding_mutation_count = sum(
        rejects_with(label, parse_bindings, changed, stage)
        for label, changed, stage in binding_mutations
    )
    empty_selection = selection_values["selection-empty.bin"]
    mixed_selection = selection_values["selection-mixed.bin"]
    parse_selection(
        empty_selection,
        inventories["inventory-empty.bin"],
        binding_values["bindings-empty.bin"],
    )
    parse_selection(mixed_selection, inventory, mixed_bindings)
    parse_selection(
        selection_values["selection-block-implementation.bin"],
        inventory,
        block_bindings,
    )
    parse_selection(
        selection_values["selection-shared-cyclic.bin"],
        inventories["inventory-shared-cyclic.bin"],
        binding_values["bindings-empty.bin"],
    )
    try:
        parse_selection(mutation(mixed_selection, 228, struct.pack("<I", 3)), inventory, mixed_bindings)
    except Reject as error:
        require(error.stage == "selection-row", "test", "selection disposition stage")
    else:
        raise AssertionError("unknown disposition accepted")
    mixed_selection_parser = lambda data: parse_selection(data, inventory, mixed_bindings)
    mixed_selector_offset = u64(mixed_selection, 144, "test")
    selection_mutations = (
        ("selection version", mutation(mixed_selection, 32, struct.pack("<I", 2)), "selection-prefix"),
        ("selection suffix", mixed_selection + b"\0", "selection-prefix"),
        ("selection row coverage", mutation(mixed_selection, 104, struct.pack("<I", 4)), "selection-coverage"),
        ("selection directory kind", mutation(mixed_selection, 64, struct.pack("<I", 4)), "selection-directory"),
        ("selection program authority", mutation(mixed_selection, 160, bytes((mixed_selection[160] ^ 1,))), "selection-metadata"),
        ("selection binding authority", mutation(mixed_selection, 192, bytes((mixed_selection[192] ^ 1,))), "selection-metadata"),
        ("selection row kind", mutation(mixed_selection, 224, struct.pack("<I", 2)), "selection-row"),
        ("selection row subject", mutation(mixed_selection, 232, struct.pack("<I", 1)), "selection-row"),
        ("selection row arity", mutation(mixed_selection, 236, struct.pack("<I", 99)), "selection-row"),
        ("selection named captures", mutation(mixed_selection, 240, struct.pack("<I", 1)), "selection-row"),
        ("selection selector length", mutation(mixed_selection, 244, struct.pack("<I", 0)), "selection-row"),
        ("selection row flags", mutation(mixed_selection, 248, b"\x01"), "selection-row"),
        ("selection row reserved", mutation(mixed_selection, 252, b"\x01"), "selection-row"),
        ("selection semantic digest", mutation(mixed_selection, 256, bytes((mixed_selection[256] ^ 1,))), "selection-row"),
        ("selection selector bytes", mutation(mixed_selection, mixed_selector_offset, b"x"), "selection-row"),
        ("selection block captures", mutation(mixed_selection, 432, struct.pack("<I", 99)), "selection-row"),
    )
    selection_mutation_count = sum(
        rejects_with(label, mixed_selection_parser, changed, stage)
        for label, changed, stage in selection_mutations
    )
    empty_image = image_values["image-empty-stripped.bin"]
    mixed_image = image_values["image-mixed-stripped.bin"]
    code_image = image_values["image-mixed-code.bin"]
    facts_image = image_values["image-mixed-facts-stripped.bin"]
    shared_image = image_values["image-shared-cyclic-stripped.bin"]
    block_binding_image = image_values["image-block-binding-stripped.bin"]
    require(parse_image(empty_image) == (0, False), "test", "empty image summary")
    require(parse_image(mixed_image) == (12, False), "test", "mixed image summary")
    require(parse_image(code_image) == (12, True), "test", "code image summary")
    require(parse_image(facts_image) == (12, False), "test", "facts image summary")
    require(parse_image(shared_image) == (8, False), "test", "shared cyclic image summary")
    require(parse_image(block_binding_image) == (12, False), "test", "Block binding image summary")
    letter_offset = next(index for index, byte in enumerate(mixed_image[40:104], 40) if byte in b"abcdef")
    try:
        parse_image(mutation(mixed_image, letter_offset, bytes((mixed_image[letter_offset] - 32,))))
    except Reject as error:
        require(error.stage == "image-authority", "test", "uppercase authority stage")
    else:
        raise AssertionError("uppercase authority accepted")
    def image_records(data: bytes) -> list[tuple[int, int, int, int]]:
        rows = []
        cursor = 28
        while cursor < len(data):
            kind = u32(data, cursor, "test")
            length = u64(data, cursor + 4, "test")
            payload = cursor + 12
            end = payload + length
            require(end <= len(data), "test", "valid image record bounds")
            rows.append((kind, cursor, payload, end))
            cursor = end
        require(cursor == len(data), "test", "valid image record consumption")
        return rows

    mixed_records = image_records(mixed_image)
    code_records = image_records(code_image)
    heap_record = next(row for row in mixed_records if row[0] == 3)
    _, first_locator_end = image_field(mixed_image, heap_record[2] + 4)
    first_slot = first_locator_end + 4
    code_record = next(row for row in code_records if row[0] == 4)
    for label, changed, stage in (
        ("legacy record", mutation(mixed_image, heap_record[1], struct.pack("<I", 6)), "image-record"),
        ("legacy field", mutation(mixed_image, first_slot, b"\x0a"), "image-field"),
        ("forged program hash", mutation(mixed_image, 116, bytes((mixed_image[116] ^ 1,))), "image-record"),
        ("code checksum", mutation(code_image, code_record[2], bytes((code_image[code_record[2]] ^ 1,))), "image-code"),
        ("code suffix", code_image + b"\0", "image-record"),
    ):
        try:
            parse_image(changed)
        except Reject as error:
            require(error.stage == stage, "test", f"{label} stage")
        else:
            raise AssertionError(f"{label} accepted")
    facts_records = image_records(facts_image)
    program_record = mixed_records[1]
    binding_record = mixed_records[2]
    selection_record = mixed_records[3]
    heap_record = mixed_records[4]
    facts_record = facts_records[5]
    code_record = code_records[5]
    empty_code = bytearray(code_image[: code_record[2] + 72])
    struct.pack_into("<Q", empty_code, code_record[1] + 4, 72)
    duplicate_authority = (
        mixed_image[: mixed_records[1][1]]
        + mixed_image[mixed_records[0][1] : mixed_records[0][3]]
        + mixed_image[mixed_records[1][1] :]
    )
    reordered_heap_facts = (
        facts_image[: facts_records[4][1]]
        + facts_image[facts_records[5][1] : facts_records[5][3]]
        + facts_image[facts_records[4][1] : facts_records[4][3]]
    )
    heap_payload = heap_record[2]
    facts_payload = facts_record[2]
    image_mutations = (
        ("image magic", mutation(mixed_image, 0, b"C"), "image-header"),
        ("image version", mutation(mixed_image, 8, struct.pack("<I", 7)), "image-header"),
        ("image endian", mutation(mixed_image, 12, struct.pack("<I", 0)), "image-header"),
        ("image architecture", mutation(mixed_image, 16, struct.pack("<I", 2)), "image-header"),
        ("image runtime ABI", mutation(mixed_image, 20, struct.pack("<Q", 0)), "image-header"),
        ("authority record kind", mutation(mixed_image, 28, struct.pack("<I", 7)), "image-record"),
        ("authority record length", mutation(mixed_image, 32, struct.pack("<Q", 63)), "image-authority"),
        ("authority nonhex", mutation(mixed_image, 40, b"z"), "image-authority"),
        ("portable record order", mutation(mixed_image, program_record[1], struct.pack("<I", 8)), "image-record"),
        ("portable value length", mutation(mixed_image, program_record[2] + 32, struct.pack("<Q", 0)), "image-record"),
        ("binding record hash", mutation(mixed_image, binding_record[2], bytes((mixed_image[binding_record[2]] ^ 1,))), "image-record"),
        ("selection record hash", mutation(mixed_image, selection_record[2], bytes((mixed_image[selection_record[2]] ^ 1,))), "image-record"),
        ("duplicate authority record", duplicate_authority, "image-record"),
        ("Code authority mismatch", mutation(code_image, code_record[2] + 8, b"0"), "image-code"),
        ("Code empty unit", bytes(empty_code), "image-code"),
        ("ProgramRef outside CodeObject", mutation(mixed_image, heap_payload + 9, b"Ordinary__Vt"), "heap"),
        ("CodeObject word count", mutation(mixed_image, heap_payload + 21, struct.pack("<I", 1)), "heap"),
        ("CodeObject program index", mutation(mixed_image, heap_payload + 26, struct.pack("<I", 99)), "heap"),
        ("Block locator target", mutation(mixed_image, heap_payload + 32, struct.pack("<I", 7)), "heap"),
        ("Block word-zero mismatch", mutation(mixed_image, heap_payload + 41, struct.pack("<I", 1)), "heap"),
        ("Facts retention floor", mutation(facts_image, facts_payload + 8, struct.pack("<Q", 11)), "image-facts"),
        ("Facts retention policy", mutation(facts_image, facts_payload + 16, b"\x03"), "image-facts"),
        ("Facts ProgramRef", mutation(facts_image, facts_payload + 60, b"\x0b"), "image-facts"),
        ("Facts BlockVtableRef target", mutation(facts_image, facts_payload + 61, struct.pack("<I", 7)), "image-facts"),
        ("Heap after Facts", reordered_heap_facts, "image-record"),
    )
    image_mutation_count = sum(
        rejects_with(label, parse_image, changed, stage)
        for label, changed, stage in image_mutations
    )
    truncations = sum(reject_all_truncations(data, parse_program) for data in loaded.values())
    truncations += sum(
        reject_all_truncations(data, parse_inventory) for data in inventories.values()
    )
    truncations += sum(
        reject_all_truncations(data, parse_bindings) for data in binding_values.values()
    )
    selection_contexts = {
        "selection-block-implementation.bin": (inventory, block_bindings),
        "selection-empty.bin": (
            inventories["inventory-empty.bin"],
            binding_values["bindings-empty.bin"],
        ),
        "selection-mixed.bin": (inventory, mixed_bindings),
        "selection-shared-cyclic.bin": (
            inventories["inventory-shared-cyclic.bin"],
            binding_values["bindings-empty.bin"],
        ),
    }
    for filename, data in selection_values.items():
        programs, bindings = selection_contexts[filename]
        truncations += reject_all_truncations(
            data,
            lambda prefix, programs=programs, bindings=bindings: parse_selection(
                prefix, programs, bindings
            ),
        )
    admitted_images = tuple(image_values.values())
    truncations += sum(
        reject_all_truncations(data, parse_image, admitted_images)
        for data in image_values.values()
    )
    print(
        f"verified {len(FIXTURES)} stored and {len(generated)} generated programs, "
        f"{len(INVENTORIES)} inventories, "
        f"{len(BINDINGS)} binding sets, {len(SELECTIONS)} selections, "
        f"{len(IMAGES)} images, "
        f"{20 + additional_mutation_count + inventory_mutation_count + binding_mutation_count + selection_mutation_count + image_mutation_count} mutations, "
        f"and {truncations} truncations"
    )


if __name__ == "__main__":
    main()
