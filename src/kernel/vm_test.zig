//! Phase 11.7.i + 11.1 — Vm struct + opcode set tests.

const std = @import("std");
const testing = std.testing;
const vm = @import("vm.zig");

test "Vm: per-thread state defaults" {
    vm.reset();
    const v = vm.current();
    try testing.expectEqual(@as(u32, 0), v.recursion_guard);
    try testing.expectEqual(@as(u32, 0), v.ffi_pause);
    try testing.expectEqual(false, v.unwinding);
    try testing.expectEqual(@as(usize, 0), v.pc);
}

test "Vm: reset clears mutated fields" {
    const v = vm.current();
    v.recursion_guard = 7;
    v.ffi_pause = 3;
    v.unwinding = true;
    v.pc = 42;
    vm.reset();
    try testing.expectEqual(@as(u32, 0), v.recursion_guard);
    try testing.expectEqual(@as(u32, 0), v.ffi_pause);
    try testing.expectEqual(false, v.unwinding);
    try testing.expectEqual(@as(usize, 0), v.pc);
}

test "Vm: opcode sizes match ADR-11 addendum table" {
    // ADR-11 addendum locked opcode-size table. Edit here = breaking
    // bytecode-format-version bump in image manifest.
    try testing.expectEqual(@as(usize, 3), vm.instructionSize(.PUSH_CONST));
    try testing.expectEqual(@as(usize, 5), vm.instructionSize(.PUSH_SMALLINT_I32));
    try testing.expectEqual(@as(usize, 2), vm.instructionSize(.PUSH_LOCAL));
    try testing.expectEqual(@as(usize, 2), vm.instructionSize(.STORE_LOCAL));
    try testing.expectEqual(@as(usize, 1), vm.instructionSize(.POP));
    try testing.expectEqual(@as(usize, 1), vm.instructionSize(.LOAD_SELF));
    try testing.expectEqual(@as(usize, 1), vm.instructionSize(.LOAD_CONST_TRUE));
    try testing.expectEqual(@as(usize, 1), vm.instructionSize(.LOAD_CONST_FALSE));
    try testing.expectEqual(@as(usize, 1), vm.instructionSize(.LOAD_CONST_NIL));
    try testing.expectEqual(@as(usize, 7), vm.instructionSize(.SEND_0));
    try testing.expectEqual(@as(usize, 7), vm.instructionSize(.SEND_1));
    try testing.expectEqual(@as(usize, 8), vm.instructionSize(.SEND_N));
    try testing.expectEqual(@as(usize, 1), vm.instructionSize(.RETURN));
    try testing.expectEqual(@as(usize, 2), vm.instructionSize(.RETURN_FROM_METHOD));
    try testing.expectEqual(@as(usize, 3), vm.instructionSize(.JUMP));
    try testing.expectEqual(@as(usize, 3), vm.instructionSize(.BRANCH_IF_FALSE));
    try testing.expectEqual(@as(usize, 3), vm.instructionSize(.MAKE_BLOCK));
    try testing.expectEqual(@as(usize, 4), vm.instructionSize(.INVOKE_BLOCK));
}

test "Vm: opcode byte values stable (bytecode-format-version anchor)" {
    // Renumbering the Opcode enum = breaking format change. Bump
    // bytecode-format-version in manifest.txt per ADR-11 addendum.
    try testing.expectEqual(@as(u8, 0), @intFromEnum(vm.Opcode.PUSH_CONST));
    try testing.expectEqual(@as(u8, 17), @intFromEnum(vm.Opcode.INVOKE_BLOCK));
}
