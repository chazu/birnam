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

// -- Phase 11.6 reify-stack -----------------------------------------------

/// Installs a method whose Imp captures Vm.frames at execution time and
/// reifies them as transient entities. Verifies the frame chain shape.
var g_reified_eids: []u64 = &[_]u64{};
var g_reify_store: ?*fact_store_mod.FactStore = null;
var g_reify_table: ?*object_table_mod.ObjectTable = null;
var g_reify_allocator: std.mem.Allocator = undefined;

fn reifyAtCallImpl(_: *@import("send.zig").Send, _: [*]const ref_mod.Ref, _: usize) callconv(.c) ref_mod.Ref {
    g_reified_eids = dispatch.reifyStack(g_reify_allocator, g_reify_store.?, g_reify_table.?, MODULE_EID) catch &[_]u64{};
    return ref_mod.NIL;
}

test "reify-stack: captures active frame at SEND-time" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());
    send_mod.initDispatch(&store, &table, arena.allocator());

    g_reify_store = &store;
    g_reify_table = &table;
    g_reify_allocator = arena.allocator();

    // Install reify-at-call as method on test vt.
    const test_vt: u64 = kernel_eids.OBJECT_VT;
    const sel: u64 = kernel_eids.SEL_NOT;
    const closure_eid = table.alloc();
    const c_ptr = try arena.allocator().create(closure_mod.Closure);
    c_ptr.* = .{ .method = @ptrCast(&reifyAtCallImpl), .data = ref_mod.NIL };
    try table.put(closure_eid, ref_mod.mkPointer(@ptrCast(c_ptr)));
    _ = try store.assert(test_vt, sel, ref_mod.mkEid(closure_eid), MODULE_EID);

    // Receiver with :vtable test_vt.
    const recv_eid: u64 = 600;
    _ = try store.assert(recv_eid, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(test_vt), MODULE_EID);

    // Method body: [recv SEL_NOT] — a unary send.
    _ = try store.assert(1, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_LITERAL_VT), MODULE_EID);
    _ = try store.assert(1, kernel_eids.ATTR_AST_VALUE, ref_mod.mkEid(recv_eid), MODULE_EID);
    _ = try store.assert(2, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_UNARY_SEND_VT), MODULE_EID);
    _ = try store.assert(2, kernel_eids.ATTR_AST_RECEIVER, ref_mod.mkEid(1), MODULE_EID);
    _ = try store.assert(2, kernel_eids.ATTR_AST_SELECTOR, ref_mod.mkEid(sel), MODULE_EID);

    var m = try compiler.compileMethod(arena.allocator(), &store, 2);
    defer m.deinit(arena.allocator());
    const method_eid = try compiler.materialize(arena.allocator(), &store, &table, &m, MODULE_EID);

    g_reified_eids = &[_]u64{};
    _ = try dispatch.runMethod(arena.allocator(), &store, ref_mod.NIL, method_eid, &.{});

    // At SEND-time, exactly one frame (the active method) is on the stack.
    try testing.expectEqual(@as(usize, 1), g_reified_eids.len);

    // That frame should reference method_eid + have a non-zero pc.
    const f_eid = g_reified_eids[0];
    const fm = store.currentValue(f_eid, kernel_eids.ATTR_FRAME_METHOD).?;
    try testing.expectEqual(ref_mod.mkEid(method_eid), fm);
    const pc_ref = store.currentValue(f_eid, kernel_eids.ATTR_FRAME_PC).?;
    try testing.expect(ref_mod.untagSmallInt(pc_ref) > 0);
    try testing.expect(store.currentValue(f_eid, kernel_eids.ATTR_FRAME_PARENT) == null);
}

test "dispatch: compile + run block with params" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());

    // [: x | x] body uses param x — slot 0.
    const x_name: u64 = 900;
    _ = try store.assert(100, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(x_name), MODULE_EID);

    // body: x (var-ref)
    _ = try store.assert(101, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_VAR_REF_VT), MODULE_EID);
    _ = try store.assert(101, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(x_name), MODULE_EID);

    _ = try store.assert(102, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_BLOCK_VT), MODULE_EID);
    _ = try store.assert(102, kernel_eids.ATTR_AST_BLOCK_BODY, ref_mod.mkEid(101), MODULE_EID);
    _ = try store.assert(102, kernel_eids.ATTR_AST_BLOCK_PARAMS, ref_mod.mkEid(100), MODULE_EID);

    // Compile block as method-body so MAKE_BLOCK emits.
    var m = try compiler.compileMethod(arena.allocator(), &store, 102);
    defer m.deinit(arena.allocator());
    const outer_eid = try compiler.materialize(arena.allocator(), &store, &table, &m, MODULE_EID);

    // Run outer → top of stack should be the block-method-eid Ref.
    const block_ref = try dispatch.runMethod(arena.allocator(), &store, ref_mod.NIL, outer_eid, &.{});
    try testing.expect(ref_mod.isEid(block_ref));
    const block_eid = ref_mod.untagEid(block_ref);

    // Verify block's :method-arg-count = 1.
    const arg_ref = store.currentValue(block_eid, kernel_eids.ATTR_METHOD_ARG_COUNT).?;
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(arg_ref));

    // Invoke block with arg 55 → expect 55.
    const result = try dispatch.runMethod(arena.allocator(), &store, ref_mod.NIL, block_eid, &.{ref_mod.mkSmallInt(55).?});
    try testing.expectEqual(ref_mod.mkSmallInt(55).?, result);
}

test "dispatch: compile + run let-binding stmt-seq" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());

    // let x = 5; x
    const name_eid: u64 = 800;
    try mkLiteralAst(&store, 1, ref_mod.mkSmallInt(5).?);
    _ = try store.assert(2, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_LET_VT), MODULE_EID);
    _ = try store.assert(2, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(name_eid), MODULE_EID);
    _ = try store.assert(2, kernel_eids.ATTR_AST_LET_INIT, ref_mod.mkEid(1), MODULE_EID);

    _ = try store.assert(3, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_VAR_REF_VT), MODULE_EID);
    _ = try store.assert(3, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(name_eid), MODULE_EID);

    _ = try store.assert(2, kernel_eids.ATTR_AST_NEXT_SIBLING, ref_mod.mkEid(3), MODULE_EID);
    _ = try store.assert(4, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_STMT_SEQ_VT), MODULE_EID);
    _ = try store.assert(4, kernel_eids.ATTR_AST_STMT_FIRST, ref_mod.mkEid(2), MODULE_EID);

    try testing.expectEqual(ref_mod.mkSmallInt(5).?, try compileMaterializeRun(arena.allocator(), &store, &table, 4));
}

test "dispatch: compile + run assign updates binding" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());

    // let y = 1; y := 42; y
    const name_eid: u64 = 850;
    try mkLiteralAst(&store, 1, ref_mod.mkSmallInt(1).?);
    _ = try store.assert(2, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_LET_VT), MODULE_EID);
    _ = try store.assert(2, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(name_eid), MODULE_EID);
    _ = try store.assert(2, kernel_eids.ATTR_AST_LET_INIT, ref_mod.mkEid(1), MODULE_EID);

    try mkLiteralAst(&store, 3, ref_mod.mkSmallInt(42).?);
    _ = try store.assert(4, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_ASSIGN_VT), MODULE_EID);
    _ = try store.assert(4, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(name_eid), MODULE_EID);
    _ = try store.assert(4, kernel_eids.ATTR_AST_ASSIGN_VALUE, ref_mod.mkEid(3), MODULE_EID);

    _ = try store.assert(5, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_VAR_REF_VT), MODULE_EID);
    _ = try store.assert(5, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(name_eid), MODULE_EID);

    _ = try store.assert(2, kernel_eids.ATTR_AST_NEXT_SIBLING, ref_mod.mkEid(4), MODULE_EID);
    _ = try store.assert(4, kernel_eids.ATTR_AST_NEXT_SIBLING, ref_mod.mkEid(5), MODULE_EID);

    _ = try store.assert(6, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_STMT_SEQ_VT), MODULE_EID);
    _ = try store.assert(6, kernel_eids.ATTR_AST_STMT_FIRST, ref_mod.mkEid(2), MODULE_EID);

    try testing.expectEqual(ref_mod.mkSmallInt(42).?, try compileMaterializeRun(arena.allocator(), &store, &table, 6));
}

test "dispatch: compileBlockAsMethod handles [:x | [x succ]] shape" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());
    send_mod.initDispatch(&store, &table, arena.allocator());

    // Install succ as a stub on SMALLINT_VT that returns receiver + 1.
    const closure_eid = table.alloc();
    const c_ptr = try arena.allocator().create(closure_mod.Closure);
    c_ptr.* = .{ .method = @ptrCast(&stubSuccImpl), .data = ref_mod.NIL };
    try table.put(closure_eid, ref_mod.mkPointer(@ptrCast(c_ptr)));
    const succ_sel: u64 = 12345; // arbitrary EID; tests own it
    _ = try store.assert(kernel_eids.SMALLINT_VT, succ_sel, ref_mod.mkEid(closure_eid), MODULE_EID);

    const x_name: u64 = 999;

    // param entity: :ast-name = mkEid(x_name)
    _ = try store.assert(100, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(x_name), MODULE_EID);

    // var-ref x: :vtable AST_VAR_REF_VT, :ast-name = mkEid(x_name)
    _ = try store.assert(101, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_VAR_REF_VT), MODULE_EID);
    _ = try store.assert(101, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(x_name), MODULE_EID);

    // bracket-unary [x succ]: AST_UNARY_SEND_VT, recv=101, sel=succ_sel
    _ = try store.assert(102, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_UNARY_SEND_VT), MODULE_EID);
    _ = try store.assert(102, kernel_eids.ATTR_AST_RECEIVER, ref_mod.mkEid(101), MODULE_EID);
    _ = try store.assert(102, kernel_eids.ATTR_AST_SELECTOR, ref_mod.mkEid(succ_sel), MODULE_EID);

    // Wrap body in STMT_SEQ — lang parser desugars block body to a stmt-list.
    _ = try store.assert(110, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_STMT_SEQ_VT), MODULE_EID);
    _ = try store.assert(110, kernel_eids.ATTR_AST_STMT_FIRST, ref_mod.mkEid(102), MODULE_EID);

    // outer block [:x | [x succ]]: AST_BLOCK_VT, params=100, body=stmt-seq(102)
    _ = try store.assert(103, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_BLOCK_VT), MODULE_EID);
    _ = try store.assert(103, kernel_eids.ATTR_AST_BLOCK_PARAMS, ref_mod.mkEid(100), MODULE_EID);
    _ = try store.assert(103, kernel_eids.ATTR_AST_BLOCK_BODY, ref_mod.mkEid(110), MODULE_EID);

    // Use the same entry as the IdApi switchover path.
    var m = try compiler.compileBlockAsMethod(arena.allocator(), &store, 103);
    defer m.deinit(arena.allocator());
    const method_eid = try compiler.materialize(arena.allocator(), &store, &table, &m, MODULE_EID);

    try testing.expectEqual(@as(u8, 1), m.arg_count);

    // Invoke as if dispatched: receiver=3, args=[7] → expect 8.
    const result = try dispatch.runMethod(arena.allocator(), &store, ref_mod.mkSmallInt(3).?, method_eid, &.{ref_mod.mkSmallInt(7).?});
    try testing.expectEqual(ref_mod.mkSmallInt(8).?, result);
}

fn stubSuccImpl(s: *send_mod.Send, _: [*]const ref_mod.Ref, _: usize) callconv(.c) ref_mod.Ref {
    return ref_mod.mkSmallInt(ref_mod.untagSmallInt(s.receiver) + 1).?;
}

test "dispatch: SEND into compiled bytecode method via bytecode dispatcher" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());
    send_mod.initDispatch(&store, &table, arena.allocator());

    // Compile a "callee" method: literal 88 + RETURN.
    try mkLiteralAst(&store, 1, ref_mod.mkSmallInt(88).?);
    var callee_m = try compiler.compileMethod(arena.allocator(), &store, 1);
    defer callee_m.deinit(arena.allocator());
    const callee_eid = try compiler.materialize(arena.allocator(), &store, &table, &callee_m, MODULE_EID);

    // Install [test_vt :foo callee_eid].
    const test_vt: u64 = kernel_eids.OBJECT_VT;
    const sel_foo: u64 = kernel_eids.SEL_NOT; // reuse any sel EID
    _ = try store.assert(test_vt, sel_foo, ref_mod.mkEid(callee_eid), MODULE_EID);

    // Receiver entity with :vtable test_vt.
    const recv_eid: u64 = 700;
    _ = try store.assert(recv_eid, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(test_vt), MODULE_EID);

    // Outer method: [recv foo] — should dispatch into callee via bytecode dispatcher.
    _ = try store.assert(2, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_LITERAL_VT), MODULE_EID);
    _ = try store.assert(2, kernel_eids.ATTR_AST_VALUE, ref_mod.mkEid(recv_eid), MODULE_EID);
    _ = try store.assert(3, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_UNARY_SEND_VT), MODULE_EID);
    _ = try store.assert(3, kernel_eids.ATTR_AST_RECEIVER, ref_mod.mkEid(2), MODULE_EID);
    _ = try store.assert(3, kernel_eids.ATTR_AST_SELECTOR, ref_mod.mkEid(sel_foo), MODULE_EID);

    var outer_m = try compiler.compileMethod(arena.allocator(), &store, 3);
    defer outer_m.deinit(arena.allocator());
    const outer_eid = try compiler.materialize(arena.allocator(), &store, &table, &outer_m, MODULE_EID);

    const result = try dispatch.runMethod(arena.allocator(), &store, ref_mod.NIL, outer_eid, &.{});
    try testing.expectEqual(ref_mod.mkSmallInt(88).?, result);
}

fn stubSmallintPlusImpl(s: *send_mod.Send, args: [*]const ref_mod.Ref, _: usize) callconv(.c) ref_mod.Ref {
    const a = ref_mod.untagSmallInt(s.receiver);
    const b = ref_mod.untagSmallInt(args[0]);
    return ref_mod.mkSmallInt(a + b).?;
}

test "dispatch: composite let + binary send (1 + 2 + 3 = 6)" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());
    send_mod.initDispatch(&store, &table, arena.allocator());

    // Install stub + on SMALLINT_VT.
    const closure_eid = table.alloc();
    const c_ptr = try arena.allocator().create(closure_mod.Closure);
    c_ptr.* = .{ .method = @ptrCast(&stubSmallintPlusImpl), .data = ref_mod.NIL };
    try table.put(closure_eid, ref_mod.mkPointer(@ptrCast(c_ptr)));
    _ = try store.assert(kernel_eids.SMALLINT_VT, kernel_eids.SEL_PLUS, ref_mod.mkEid(closure_eid), MODULE_EID);

    // AST: let x = 1; let y = 2; (x + y) + 3
    const x_name: u64 = 700;
    const y_name: u64 = 701;

    try mkLiteralAst(&store, 1, ref_mod.mkSmallInt(1).?);
    _ = try store.assert(2, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_LET_VT), MODULE_EID);
    _ = try store.assert(2, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(x_name), MODULE_EID);
    _ = try store.assert(2, kernel_eids.ATTR_AST_LET_INIT, ref_mod.mkEid(1), MODULE_EID);

    try mkLiteralAst(&store, 3, ref_mod.mkSmallInt(2).?);
    _ = try store.assert(4, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_LET_VT), MODULE_EID);
    _ = try store.assert(4, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(y_name), MODULE_EID);
    _ = try store.assert(4, kernel_eids.ATTR_AST_LET_INIT, ref_mod.mkEid(3), MODULE_EID);

    // x var-ref
    _ = try store.assert(5, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_VAR_REF_VT), MODULE_EID);
    _ = try store.assert(5, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(x_name), MODULE_EID);
    // y var-ref
    _ = try store.assert(6, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_VAR_REF_VT), MODULE_EID);
    _ = try store.assert(6, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(y_name), MODULE_EID);
    // x + y
    _ = try store.assert(7, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_BINARY_SEND_VT), MODULE_EID);
    _ = try store.assert(7, kernel_eids.ATTR_AST_RECEIVER, ref_mod.mkEid(5), MODULE_EID);
    _ = try store.assert(7, kernel_eids.ATTR_AST_SELECTOR, ref_mod.mkEid(kernel_eids.SEL_PLUS), MODULE_EID);
    _ = try store.assert(7, kernel_eids.ATTR_AST_FIRST_ARG, ref_mod.mkEid(6), MODULE_EID);

    // (x + y) + 3
    try mkLiteralAst(&store, 8, ref_mod.mkSmallInt(3).?);
    _ = try store.assert(9, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_BINARY_SEND_VT), MODULE_EID);
    _ = try store.assert(9, kernel_eids.ATTR_AST_RECEIVER, ref_mod.mkEid(7), MODULE_EID);
    _ = try store.assert(9, kernel_eids.ATTR_AST_SELECTOR, ref_mod.mkEid(kernel_eids.SEL_PLUS), MODULE_EID);
    _ = try store.assert(9, kernel_eids.ATTR_AST_FIRST_ARG, ref_mod.mkEid(8), MODULE_EID);

    _ = try store.assert(2, kernel_eids.ATTR_AST_NEXT_SIBLING, ref_mod.mkEid(4), MODULE_EID);
    _ = try store.assert(4, kernel_eids.ATTR_AST_NEXT_SIBLING, ref_mod.mkEid(9), MODULE_EID);

    _ = try store.assert(10, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_STMT_SEQ_VT), MODULE_EID);
    _ = try store.assert(10, kernel_eids.ATTR_AST_STMT_FIRST, ref_mod.mkEid(2), MODULE_EID);

    try testing.expectEqual(ref_mod.mkSmallInt(6).?, try compileMaterializeRun(arena.allocator(), &store, &table, 10));
}

// -- MAKE_BLOCK + INVOKE_BLOCK ---------------------------------------------

const strings = @import("strings.zig");
const boxed = @import("boxed.zig");

fn installBytecodeMethod(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
    bytes: []const u8,
    pool: []const ref_mod.Ref,
) !u64 {
    const eid = table.alloc();
    const bc_ref = try strings.mkString(allocator, bytes);
    _ = try store.assert(eid, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.CLOSURE_VT), MODULE_EID);
    _ = try store.assert(eid, kernel_eids.ATTR_BYTECODE_STREAM, bc_ref, MODULE_EID);
    if (pool.len > 0) {
        const pool_ref = try boxed.mkRrb(allocator, pool);
        _ = try store.assert(eid, kernel_eids.ATTR_CONSTANT_POOL, pool_ref, MODULE_EID);
    }
    return eid;
}

test "dispatch: MAKE_BLOCK + INVOKE_BLOCK runs block and returns value" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());

    // Block method body: PUSH_SMALLINT_I32 99, RETURN.
    const block_body = [_]u8{
        @intFromEnum(bytecode.Opcode.PUSH_SMALLINT_I32), 99, 0, 0, 0,
        @intFromEnum(bytecode.Opcode.RETURN),
    };
    const block_eid = try installBytecodeMethod(arena.allocator(), &store, &table, &block_body, &.{});

    // Outer method: MAKE_BLOCK 0, INVOKE_BLOCK args=0 site=0, RETURN.
    const outer_body = [_]u8{
        @intFromEnum(bytecode.Opcode.MAKE_BLOCK), 0, 0,
        @intFromEnum(bytecode.Opcode.INVOKE_BLOCK), 0, 0, 0,
        @intFromEnum(bytecode.Opcode.RETURN),
    };
    const pool = [_]ref_mod.Ref{ref_mod.mkEid(block_eid)};
    const outer_eid = try installBytecodeMethod(arena.allocator(), &store, &table, &outer_body, &pool);

    const result = try dispatch.runMethod(arena.allocator(), &store, ref_mod.NIL, outer_eid, &.{});
    try testing.expectEqual(ref_mod.mkSmallInt(99).?, result);
}

test "dispatch: INVOKE_BLOCK passes args as locals" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());

    // Block body: PUSH_LOCAL 0, RETURN. Expects 1 arg, returns it.
    const block_body = [_]u8{
        @intFromEnum(bytecode.Opcode.PUSH_LOCAL), 0,
        @intFromEnum(bytecode.Opcode.RETURN),
    };
    const block_eid = try installBytecodeMethod(arena.allocator(), &store, &table, &block_body, &.{});

    // Outer: MAKE_BLOCK 0, PUSH_SMALLINT_I32 77, INVOKE_BLOCK args=1, RETURN.
    const outer_body = [_]u8{
        @intFromEnum(bytecode.Opcode.MAKE_BLOCK), 0, 0,
        @intFromEnum(bytecode.Opcode.PUSH_SMALLINT_I32), 77, 0, 0, 0,
        @intFromEnum(bytecode.Opcode.INVOKE_BLOCK), 1, 0, 0,
        @intFromEnum(bytecode.Opcode.RETURN),
    };
    const pool = [_]ref_mod.Ref{ref_mod.mkEid(block_eid)};
    const outer_eid = try installBytecodeMethod(arena.allocator(), &store, &table, &outer_body, &pool);

    const result = try dispatch.runMethod(arena.allocator(), &store, ref_mod.NIL, outer_eid, &.{});
    try testing.expectEqual(ref_mod.mkSmallInt(77).?, result);
}

test "dispatch: RETURN_FROM_METHOD from block unwinds to enclosing method" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());

    // Block body: PUSH_SMALLINT_I32 7, RETURN_FROM_METHOD depth=0.
    const block_body = [_]u8{
        @intFromEnum(bytecode.Opcode.PUSH_SMALLINT_I32), 7, 0, 0, 0,
        @intFromEnum(bytecode.Opcode.RETURN_FROM_METHOD), 0,
    };
    const block_eid = try installBytecodeMethod(arena.allocator(), &store, &table, &block_body, &.{});

    // Outer: MAKE_BLOCK 0, INVOKE_BLOCK 0 0, PUSH_SMALLINT_I32 99, RETURN.
    // If non-local return catches at INVOKE_BLOCK, outer returns 7.
    // If it falls through, outer would return 99 — test catches the
    // regression.
    const outer_body = [_]u8{
        @intFromEnum(bytecode.Opcode.MAKE_BLOCK), 0, 0,
        @intFromEnum(bytecode.Opcode.INVOKE_BLOCK), 0, 0, 0,
        @intFromEnum(bytecode.Opcode.PUSH_SMALLINT_I32), 99, 0, 0, 0,
        @intFromEnum(bytecode.Opcode.RETURN),
    };
    const pool = [_]ref_mod.Ref{ref_mod.mkEid(block_eid)};
    const outer_eid = try installBytecodeMethod(arena.allocator(), &store, &table, &outer_body, &pool);

    const vm = @import("vm.zig").current();
    vm.unwinding = false;
    vm.unwind_target_remaining = 0;
    vm.unwind_value = ref_mod.NIL;

    const result = try dispatch.runMethod(arena.allocator(), &store, ref_mod.NIL, outer_eid, &.{});
    try testing.expectEqual(ref_mod.mkSmallInt(7).?, result);
    try testing.expectEqual(false, vm.unwinding);
}

// -- SEND tests: install a stub method, dispatch through send.zig --------

const send_mod = @import("send.zig");
const closure_mod = @import("closure.zig");

fn stubAdd1Impl(_: *send_mod.Send, args: [*]const ref_mod.Ref, n: usize) callconv(.c) ref_mod.Ref {
    _ = n;
    // recv + arg as smallint. Test harness drives values; assume both smallints.
    const recv = args[0]; // n=1 → args[0]=arg; receiver came via state in Send. For simplicity, treat arg as the only input.
    return recv;
}

fn stubEchoImpl(s: *send_mod.Send, _: [*]const ref_mod.Ref, _: usize) callconv(.c) ref_mod.Ref {
    // Unary send returns receiver verbatim.
    return s.receiver;
}

test "dispatch: SEND_0 unary message returns receiver via stub method" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());

    // Wire send.zig globals so bind works inside this test.
    send_mod.initDispatch(&store, &table, arena.allocator());

    // Install :vtable on receiver (use a small test EID with a vt set).
    const test_vt: u64 = kernel_eids.OBJECT_VT;
    const recv_eid: u64 = 500;
    _ = try store.assert(recv_eid, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(test_vt), MODULE_EID);

    // Install stub method [test_vt sel_echo closure_ref].
    const sel_echo: u64 = kernel_eids.SEL_NOT; // pick any sel EID; the stub ignores it
    const closure_eid = table.alloc();
    const c_ptr = try arena.allocator().create(closure_mod.Closure);
    c_ptr.* = .{ .method = @ptrCast(&stubEchoImpl), .data = ref_mod.NIL };
    try table.put(closure_eid, ref_mod.mkPointer(@ptrCast(c_ptr)));
    _ = try store.assert(test_vt, sel_echo, ref_mod.mkEid(closure_eid), MODULE_EID);

    // AST: [recv_eid SEL_NOT] — receiver loaded as literal AST containing
    // mkEid(recv_eid). For now we hand-build the AST.
    _ = try store.assert(1, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_LITERAL_VT), MODULE_EID);
    _ = try store.assert(1, kernel_eids.ATTR_AST_VALUE, ref_mod.mkEid(recv_eid), MODULE_EID);

    _ = try store.assert(2, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_UNARY_SEND_VT), MODULE_EID);
    _ = try store.assert(2, kernel_eids.ATTR_AST_RECEIVER, ref_mod.mkEid(1), MODULE_EID);
    _ = try store.assert(2, kernel_eids.ATTR_AST_SELECTOR, ref_mod.mkEid(sel_echo), MODULE_EID);

    var m = try compiler.compileMethod(arena.allocator(), &store, 2);
    defer m.deinit(arena.allocator());
    const method_eid = try compiler.materialize(arena.allocator(), &store, &table, &m, MODULE_EID);

    const result = try dispatch.runMethod(arena.allocator(), &store, ref_mod.NIL, method_eid, &.{});
    try testing.expectEqual(ref_mod.mkEid(recv_eid), result);
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
