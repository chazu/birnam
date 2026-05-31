//! Phase 11 — bytecode encoder/decoder tests.

const std = @import("std");
const testing = std.testing;
const bc = @import("bytecode.zig");

test "bytecode: literal + return round-trip" {
    var e = bc.Encoder.init(testing.allocator);
    defer e.deinit(testing.allocator);

    try e.pushSmallInt(testing.allocator, 42);
    try e.ret(testing.allocator);

    const bytes = e.buf.items;
    try testing.expectEqual(@as(usize, 6), bytes.len);
    try testing.expectEqual(bc.Opcode.PUSH_SMALLINT_I32, bc.readOpcode(bytes, 0));
    try testing.expectEqual(@as(i32, 42), bc.readI32(bytes, 1));
    try testing.expectEqual(bc.Opcode.RETURN, bc.readOpcode(bytes, 5));
}

test "bytecode: send1 layout" {
    var e = bc.Encoder.init(testing.allocator);
    defer e.deinit(testing.allocator);

    try e.send1(testing.allocator, 0xCAFEBABE, 7);

    const bytes = e.buf.items;
    try testing.expectEqual(@as(usize, 7), bytes.len);
    try testing.expectEqual(bc.Opcode.SEND_1, bc.readOpcode(bytes, 0));
    try testing.expectEqual(@as(u32, 0xCAFEBABE), bc.readU32(bytes, 1));
    try testing.expectEqual(@as(u16, 7), bc.readU16(bytes, 5));
}

test "bytecode: sendN layout" {
    var e = bc.Encoder.init(testing.allocator);
    defer e.deinit(testing.allocator);

    try e.sendN(testing.allocator, 0x11223344, 3, 9);

    const bytes = e.buf.items;
    try testing.expectEqual(@as(usize, 8), bytes.len);
    try testing.expectEqual(bc.Opcode.SEND_N, bc.readOpcode(bytes, 0));
    try testing.expectEqual(@as(u32, 0x11223344), bc.readU32(bytes, 1));
    try testing.expectEqual(@as(u8, 3), bc.readU8(bytes, 5));
    try testing.expectEqual(@as(u16, 9), bc.readU16(bytes, 6));
}

test "bytecode: forward jump patch" {
    var e = bc.Encoder.init(testing.allocator);
    defer e.deinit(testing.allocator);

    try e.loadNil(testing.allocator); // pc=0
    const patch = try e.jump_forward(testing.allocator); // pc=1, patch=2, after_operand=4
    try e.loadFalse(testing.allocator); // pc=4 (skipped at runtime)
    const target = e.pos(); // pc=5
    try e.loadTrue(testing.allocator);
    try e.ret(testing.allocator);

    try e.patchJump(patch, target);

    const bytes = e.buf.items;
    // JUMP opcode at 1, offset at 2..3. Offset = target - (patch + 2) = 5 - 4 = 1.
    try testing.expectEqual(bc.Opcode.JUMP, bc.readOpcode(bytes, 1));
    try testing.expectEqual(@as(i16, 1), bc.readI16(bytes, 2));
}

test "bytecode: all 16 opcodes encode + decode" {
    var e = bc.Encoder.init(testing.allocator);
    defer e.deinit(testing.allocator);

    try e.pushConst(testing.allocator, 100);
    try e.pushSmallInt(testing.allocator, -7);
    try e.pushLocal(testing.allocator, 3);
    try e.storeLocal(testing.allocator, 4);
    try e.pop(testing.allocator);
    try e.loadSelf(testing.allocator);
    try e.loadTrue(testing.allocator);
    try e.loadFalse(testing.allocator);
    try e.loadNil(testing.allocator);
    try e.send0(testing.allocator, 200, 0);
    try e.send1(testing.allocator, 201, 1);
    try e.sendN(testing.allocator, 202, 2, 2);
    try e.ret(testing.allocator);
    try e.returnFromMethod(testing.allocator, 1);
    try e.jump(testing.allocator, -4);
    try e.branchIfFalse(testing.allocator, 8);
    try e.makeBlock(testing.allocator, 50);
    try e.invokeBlock(testing.allocator, 1, 3);

    // Walk and decode every opcode. Asserts each Opcode value is reachable
    // by encoder API + readable by decoder. Catches enum drift.
    const bytes = e.buf.items;
    var pc: usize = 0;
    while (pc < bytes.len) {
        const op = bc.readOpcode(bytes, pc);
        pc += @import("vm.zig").instructionSize(op);
    }
    try testing.expectEqual(bytes.len, pc);
}
