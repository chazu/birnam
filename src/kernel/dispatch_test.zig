//! Phase 11.3 — VM dispatch tests (A6 scope: non-SEND opcodes).

const std = @import("std");
const testing = std.testing;
const compiler = @import("compiler.zig");
const dispatch = @import("dispatch.zig");
const bytecode = @import("bytecode.zig");
const fact_store_mod = @import("fact_store.zig");
const object_table_mod = @import("object_table.zig");
const ref_mod = @import("ref.zig");
const kernel_eids = @import("kernel_eids.zig");

const MODULE_EID: u64 = kernel_eids.M_BOOTSTRAP;

fn mkLiteralAst(store: *fact_store_mod.FactStore, ast_eid: u64, value: ref_mod.Ref) !void {
    _ = try store.assert(ast_eid, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_LITERAL_VT), MODULE_EID);
    _ = try store.assert(ast_eid, kernel_eids.ATTR_AST_VALUE, value, MODULE_EID);
}

/// End-to-end helper: parse-free → AST → bytecode → fact-store → run.
fn compileMaterializeRun(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
    ast_eid: u64,
) !ref_mod.Ref {
    var m = try compiler.compileMethod(allocator, store, ast_eid);
    defer m.deinit(allocator);
    const method_eid = try compiler.materialize(allocator, store, table, &m, MODULE_EID);
    return dispatch.runMethod(allocator, store, ref_mod.NIL, method_eid, &.{});
}

test "dispatch: smallint literal" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());

    try mkLiteralAst(&store, 1, ref_mod.mkSmallInt(42).?);
    const result = try compileMaterializeRun(arena.allocator(), &store, &table, 1);
    try testing.expectEqual(ref_mod.mkSmallInt(42).?, result);
}

test "dispatch: true / false / nil literals" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());

    try mkLiteralAst(&store, 1, ref_mod.mkEid(kernel_eids.TRUE_EID));
    try testing.expectEqual(ref_mod.mkEid(kernel_eids.TRUE_EID), try compileMaterializeRun(arena.allocator(), &store, &table, 1));

    try mkLiteralAst(&store, 2, ref_mod.mkEid(kernel_eids.FALSE_EID));
    try testing.expectEqual(ref_mod.mkEid(kernel_eids.FALSE_EID), try compileMaterializeRun(arena.allocator(), &store, &table, 2));

    // Nil literal: AST_LITERAL_VT with no :ast-value.
    _ = try store.assert(3, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_LITERAL_VT), MODULE_EID);
    try testing.expectEqual(ref_mod.NIL, try compileMaterializeRun(arena.allocator(), &store, &table, 3));
}

test "dispatch: stmt-seq returns last value" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());

    try mkLiteralAst(&store, 1, ref_mod.mkSmallInt(10).?);
    try mkLiteralAst(&store, 2, ref_mod.mkSmallInt(20).?);
    try mkLiteralAst(&store, 3, ref_mod.mkSmallInt(30).?);
    _ = try store.assert(1, kernel_eids.ATTR_AST_NEXT_SIBLING, ref_mod.mkEid(2), MODULE_EID);
    _ = try store.assert(2, kernel_eids.ATTR_AST_NEXT_SIBLING, ref_mod.mkEid(3), MODULE_EID);

    _ = try store.assert(4, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_STMT_SEQ_VT), MODULE_EID);
    _ = try store.assert(4, kernel_eids.ATTR_AST_STMT_FIRST, ref_mod.mkEid(1), MODULE_EID);

    try testing.expectEqual(ref_mod.mkSmallInt(30).?, try compileMaterializeRun(arena.allocator(), &store, &table, 4));
}

test "dispatch: explicit return short-circuits" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());

    try mkLiteralAst(&store, 1, ref_mod.mkSmallInt(7).?);
    _ = try store.assert(2, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_RETURN_VT), MODULE_EID);
    _ = try store.assert(2, kernel_eids.ATTR_AST_RETURN_VALUE, ref_mod.mkEid(1), MODULE_EID);

    try testing.expectEqual(ref_mod.mkSmallInt(7).?, try compileMaterializeRun(arena.allocator(), &store, &table, 2));
}

test "dispatch: LOAD_SELF returns receiver" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());

    _ = try store.assert(1, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_SELF_VT), MODULE_EID);

    var m = try compiler.compileMethod(arena.allocator(), &store, 1);
    defer m.deinit(arena.allocator());
    const method_eid = try compiler.materialize(arena.allocator(), &store, &table, &m, MODULE_EID);

    const receiver = ref_mod.mkSmallInt(99).?;
    const result = try dispatch.runMethod(arena.allocator(), &store, receiver, method_eid, &.{});
    try testing.expectEqual(receiver, result);
}

test "dispatch: big int literal via constant pool" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());

    const big = ref_mod.mkSmallInt(@as(i64, std.math.maxInt(i32)) + 1).?;
    try mkLiteralAst(&store, 1, big);
    try testing.expectEqual(big, try compileMaterializeRun(arena.allocator(), &store, &table, 1));
}
