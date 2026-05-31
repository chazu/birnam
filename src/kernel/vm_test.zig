//! Phase 11.7.i + 11.1 — Vm struct + opcode set tests.

const std = @import("std");
const testing = std.testing;
const vm = @import("vm.zig");
const ref_mod = @import("ref.zig");

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

// -- mcache tests -----------------------------------------------------------

fn stubMethodA(_: u32) callconv(.c) u32 {
    return 1;
}
fn stubMethodB(_: u32) callconv(.c) u32 {
    return 2;
}

test "mcache: empty lookup returns null" {
    var mc = try vm.MethodMCache.alloc(testing.allocator, 4);
    defer mc.deinit(testing.allocator);
    try testing.expect(mc.lookup(0, 100) == null);
    try testing.expect(mc.lookup(3, 999) == null);
}

test "mcache: store + lookup hit" {
    var mc = try vm.MethodMCache.alloc(testing.allocator, 4);
    defer mc.deinit(testing.allocator);
    const impl: *const anyopaque = @ptrCast(&stubMethodA);
    mc.store(2, 42, impl);
    const got = mc.lookup(2, 42);
    try testing.expect(got != null);
    try testing.expectEqual(impl, got.?);
}

test "mcache: vt mismatch returns null" {
    var mc = try vm.MethodMCache.alloc(testing.allocator, 4);
    defer mc.deinit(testing.allocator);
    mc.store(1, 42, @ptrCast(&stubMethodA));
    try testing.expect(mc.lookup(1, 99) == null);
}

test "mcache: out-of-range site_idx safe" {
    var mc = try vm.MethodMCache.alloc(testing.allocator, 2);
    defer mc.deinit(testing.allocator);
    mc.store(5, 42, @ptrCast(&stubMethodA));
    try testing.expect(mc.lookup(5, 42) == null);
}

test "mcache: invalidateImpl walks and zeros matching entries" {
    var mc = try vm.MethodMCache.alloc(testing.allocator, 4);
    defer mc.deinit(testing.allocator);
    const impl_a: *const anyopaque = @ptrCast(&stubMethodA);
    const impl_b: *const anyopaque = @ptrCast(&stubMethodB);
    mc.store(0, 10, impl_a);
    mc.store(1, 20, impl_b);
    mc.store(2, 30, impl_a);
    mc.store(3, 40, impl_b);

    mc.invalidateImpl(impl_a);

    try testing.expect(mc.lookup(0, 10) == null);
    try testing.expect(mc.lookup(2, 30) == null);
    // B entries survive.
    const got1 = mc.lookup(1, 20);
    const got3 = mc.lookup(3, 40);
    try testing.expect(got1 != null);
    try testing.expect(got3 != null);
    try testing.expectEqual(impl_b, got1.?);
    try testing.expectEqual(impl_b, got3.?);
}

test "mcache: 16-byte alignment + size invariants" {
    try testing.expectEqual(@as(usize, 16), @sizeOf(vm.MCEntry));
    try testing.expect(@alignOf(vm.MCEntry) >= 16);
}

// -- Frame tests ------------------------------------------------------------

test "Frame: defaults" {
    const f = vm.Frame{
        .bytecode = null,
        .pc = 0,
        .method_eid = 0,
        .self_ref = ref_mod.NIL,
        .bp = 0,
        .local_count = 0,
        .parent_method_frame = null,
    };
    try testing.expectEqual(@as(usize, 0), f.pc);
    try testing.expectEqual(@as(u64, 0), f.method_eid);
    try testing.expect(f.parent_method_frame == null);
}
