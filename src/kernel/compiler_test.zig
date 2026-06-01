//! Phase 11.2 — AST compiler tests (A1: literals + RETURN).

const std = @import("std");
const testing = std.testing;
const compiler = @import("compiler.zig");
const bytecode = @import("bytecode.zig");
const fact_store_mod = @import("fact_store.zig");
const ref_mod = @import("ref.zig");
const kernel_eids = @import("kernel_eids.zig");

const MODULE_EID: u64 = kernel_eids.M_BOOTSTRAP;

fn mkLiteralAst(store: *fact_store_mod.FactStore, ast_eid: u64, value: ref_mod.Ref) !void {
    _ = try store.assert(ast_eid, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_LITERAL_VT), MODULE_EID);
    _ = try store.assert(ast_eid, kernel_eids.ATTR_AST_VALUE, value, MODULE_EID);
}

test "compile smallint literal → PUSH_SMALLINT_I32 + RETURN" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    const ast = 1000;
    try mkLiteralAst(&store, ast, ref_mod.mkSmallInt(42).?);

    var m = try compiler.compileMethod(testing.allocator, &store, ast);
    defer m.deinit(testing.allocator);

    try testing.expectEqual(@as(usize, 6), m.bytecode.len);
    try testing.expectEqual(bytecode.Opcode.PUSH_SMALLINT_I32, bytecode.readOpcode(m.bytecode, 0));
    try testing.expectEqual(@as(i32, 42), bytecode.readI32(m.bytecode, 1));
    try testing.expectEqual(bytecode.Opcode.RETURN, bytecode.readOpcode(m.bytecode, 5));
    try testing.expectEqual(@as(usize, 0), m.constant_pool.len);
    try testing.expectEqual(@as(u8, 0), m.arg_count);
    try testing.expectEqual(@as(u8, 0), m.local_count);
    try testing.expectEqual(@as(u16, 0), m.send_site_count);
    try testing.expectEqual(@as(u64, 1000), m.method_body_ast);
}

test "compile true/false literals → dedicated opcodes" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    const cases = [_]struct { ast: u64, value: ref_mod.Ref, op: bytecode.Opcode }{
        .{ .ast = 2001, .value = ref_mod.mkEid(kernel_eids.TRUE_EID), .op = .LOAD_CONST_TRUE },
        .{ .ast = 2002, .value = ref_mod.mkEid(kernel_eids.FALSE_EID), .op = .LOAD_CONST_FALSE },
    };

    for (cases) |c| {
        try mkLiteralAst(&store, c.ast, c.value);
        var m = try compiler.compileMethod(testing.allocator, &store, c.ast);
        defer m.deinit(testing.allocator);

        try testing.expectEqual(@as(usize, 2), m.bytecode.len);
        try testing.expectEqual(c.op, bytecode.readOpcode(m.bytecode, 0));
        try testing.expectEqual(bytecode.Opcode.RETURN, bytecode.readOpcode(m.bytecode, 1));
        try testing.expectEqual(@as(usize, 0), m.constant_pool.len);
    }
}

test "compile nil literal: absence of :ast-value → LOAD_CONST_NIL" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    // Date discipline forbids NIL as a fact value, so nil-literal AST
    // omits :ast-value. Compiler interprets absence as nil.
    _ = try store.assert(5000, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_LITERAL_VT), MODULE_EID);

    var m = try compiler.compileMethod(testing.allocator, &store, 5000);
    defer m.deinit(testing.allocator);

    try testing.expectEqual(@as(usize, 2), m.bytecode.len);
    try testing.expectEqual(bytecode.Opcode.LOAD_CONST_NIL, bytecode.readOpcode(m.bytecode, 0));
    try testing.expectEqual(bytecode.Opcode.RETURN, bytecode.readOpcode(m.bytecode, 1));
}

test "compile big int literal → PUSH_CONST through pool" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    // Smallint that fits i64 but not i32 → forced through constant pool.
    const big = ref_mod.mkSmallInt(@as(i64, std.math.maxInt(i32)) + 1).?;
    try mkLiteralAst(&store, 3000, big);

    var m = try compiler.compileMethod(testing.allocator, &store, 3000);
    defer m.deinit(testing.allocator);

    try testing.expectEqual(@as(usize, 4), m.bytecode.len);
    try testing.expectEqual(bytecode.Opcode.PUSH_CONST, bytecode.readOpcode(m.bytecode, 0));
    try testing.expectEqual(@as(u16, 0), bytecode.readU16(m.bytecode, 1));
    try testing.expectEqual(bytecode.Opcode.RETURN, bytecode.readOpcode(m.bytecode, 3));
    try testing.expectEqual(@as(usize, 1), m.constant_pool.len);
    try testing.expectEqual(big, m.constant_pool[0]);
}

test "compile rejects unknown AST kind" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    // AST_VAR_REF_VT not yet supported in A1.
    _ = try store.assert(4000, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_VAR_REF_VT), MODULE_EID);
    const r = compiler.compileMethod(testing.allocator, &store, 4000);
    try testing.expectError(error.UnknownAstKind, r);
}
