//! Phase 11.2 — AST compiler tests (A1: literals + RETURN).

const std = @import("std");
const testing = std.testing;
const compiler = @import("compiler.zig");
const bytecode = @import("bytecode.zig");
const fact_store_mod = @import("fact_store.zig");
const object_table_mod = @import("object_table.zig");
const ref_mod = @import("ref.zig");
const kernel_eids = @import("kernel_eids.zig");
const strings = @import("strings.zig");
const boxed = @import("boxed.zig");

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

test "compile stmt-seq: pops intermediate values, leaves last" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    // Three literal stmts: 1; 2; 3 — bytes should be:
    //   PUSH_SMALLINT_I32 1, POP, PUSH_SMALLINT_I32 2, POP, PUSH_SMALLINT_I32 3, RETURN
    try mkLiteralAst(&store, 6001, ref_mod.mkSmallInt(1).?);
    try mkLiteralAst(&store, 6002, ref_mod.mkSmallInt(2).?);
    try mkLiteralAst(&store, 6003, ref_mod.mkSmallInt(3).?);
    _ = try store.assert(6001, kernel_eids.ATTR_AST_NEXT_SIBLING, ref_mod.mkEid(6002), MODULE_EID);
    _ = try store.assert(6002, kernel_eids.ATTR_AST_NEXT_SIBLING, ref_mod.mkEid(6003), MODULE_EID);

    const seq = 6000;
    _ = try store.assert(seq, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_STMT_SEQ_VT), MODULE_EID);
    _ = try store.assert(seq, kernel_eids.ATTR_AST_STMT_FIRST, ref_mod.mkEid(6001), MODULE_EID);

    var m = try compiler.compileMethod(testing.allocator, &store, seq);
    defer m.deinit(testing.allocator);

    // 5 + 1 + 5 + 1 + 5 + 1 = 18 bytes
    try testing.expectEqual(@as(usize, 18), m.bytecode.len);
    try testing.expectEqual(bytecode.Opcode.PUSH_SMALLINT_I32, bytecode.readOpcode(m.bytecode, 0));
    try testing.expectEqual(@as(i32, 1), bytecode.readI32(m.bytecode, 1));
    try testing.expectEqual(bytecode.Opcode.POP, bytecode.readOpcode(m.bytecode, 5));
    try testing.expectEqual(bytecode.Opcode.PUSH_SMALLINT_I32, bytecode.readOpcode(m.bytecode, 6));
    try testing.expectEqual(@as(i32, 2), bytecode.readI32(m.bytecode, 7));
    try testing.expectEqual(bytecode.Opcode.POP, bytecode.readOpcode(m.bytecode, 11));
    try testing.expectEqual(bytecode.Opcode.PUSH_SMALLINT_I32, bytecode.readOpcode(m.bytecode, 12));
    try testing.expectEqual(@as(i32, 3), bytecode.readI32(m.bytecode, 13));
    try testing.expectEqual(bytecode.Opcode.RETURN, bytecode.readOpcode(m.bytecode, 17));
}

test "compile empty stmt-seq → LOAD_CONST_NIL + RETURN" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    _ = try store.assert(7000, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_STMT_SEQ_VT), MODULE_EID);

    var m = try compiler.compileMethod(testing.allocator, &store, 7000);
    defer m.deinit(testing.allocator);

    try testing.expectEqual(@as(usize, 2), m.bytecode.len);
    try testing.expectEqual(bytecode.Opcode.LOAD_CONST_NIL, bytecode.readOpcode(m.bytecode, 0));
    try testing.expectEqual(bytecode.Opcode.RETURN, bytecode.readOpcode(m.bytecode, 1));
}

test "compile return-expr lowers to expr + RETURN" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    try mkLiteralAst(&store, 8001, ref_mod.mkSmallInt(7).?);
    _ = try store.assert(8000, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_RETURN_VT), MODULE_EID);
    _ = try store.assert(8000, kernel_eids.ATTR_AST_RETURN_VALUE, ref_mod.mkEid(8001), MODULE_EID);

    var m = try compiler.compileMethod(testing.allocator, &store, 8000);
    defer m.deinit(testing.allocator);

    // PUSH_SMALLINT_I32 7, RETURN (explicit), RETURN (implicit). Second
    // RETURN is unreachable but harmless.
    try testing.expectEqual(@as(usize, 7), m.bytecode.len);
    try testing.expectEqual(bytecode.Opcode.PUSH_SMALLINT_I32, bytecode.readOpcode(m.bytecode, 0));
    try testing.expectEqual(@as(i32, 7), bytecode.readI32(m.bytecode, 1));
    try testing.expectEqual(bytecode.Opcode.RETURN, bytecode.readOpcode(m.bytecode, 5));
    try testing.expectEqual(bytecode.Opcode.RETURN, bytecode.readOpcode(m.bytecode, 6));
}

test "compile self → LOAD_SELF + RETURN" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    _ = try store.assert(9000, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_SELF_VT), MODULE_EID);

    var m = try compiler.compileMethod(testing.allocator, &store, 9000);
    defer m.deinit(testing.allocator);

    try testing.expectEqual(@as(usize, 2), m.bytecode.len);
    try testing.expectEqual(bytecode.Opcode.LOAD_SELF, bytecode.readOpcode(m.bytecode, 0));
    try testing.expectEqual(bytecode.Opcode.RETURN, bytecode.readOpcode(m.bytecode, 1));
}

test "compile unary send: receiver + SEND_0" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    try mkLiteralAst(&store, 10001, ref_mod.mkSmallInt(5).?);
    _ = try store.assert(10000, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_UNARY_SEND_VT), MODULE_EID);
    _ = try store.assert(10000, kernel_eids.ATTR_AST_RECEIVER, ref_mod.mkEid(10001), MODULE_EID);
    _ = try store.assert(10000, kernel_eids.ATTR_AST_SELECTOR, ref_mod.mkEid(kernel_eids.SEL_NOT), MODULE_EID);

    var m = try compiler.compileMethod(testing.allocator, &store, 10000);
    defer m.deinit(testing.allocator);

    // PUSH_SMALLINT_I32 5, SEND_0 sel=SEL_NOT site=0, RETURN
    try testing.expectEqual(@as(usize, 13), m.bytecode.len);
    try testing.expectEqual(bytecode.Opcode.PUSH_SMALLINT_I32, bytecode.readOpcode(m.bytecode, 0));
    try testing.expectEqual(bytecode.Opcode.SEND_0, bytecode.readOpcode(m.bytecode, 5));
    try testing.expectEqual(@as(u32, kernel_eids.SEL_NOT), bytecode.readU32(m.bytecode, 6));
    try testing.expectEqual(@as(u16, 0), bytecode.readU16(m.bytecode, 10));
    try testing.expectEqual(bytecode.Opcode.RETURN, bytecode.readOpcode(m.bytecode, 12));
    try testing.expectEqual(@as(u16, 1), m.send_site_count);
}

test "compile binary send: recv + arg + SEND_1" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    try mkLiteralAst(&store, 11001, ref_mod.mkSmallInt(3).?);
    try mkLiteralAst(&store, 11002, ref_mod.mkSmallInt(4).?);
    _ = try store.assert(11000, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_BINARY_SEND_VT), MODULE_EID);
    _ = try store.assert(11000, kernel_eids.ATTR_AST_RECEIVER, ref_mod.mkEid(11001), MODULE_EID);
    _ = try store.assert(11000, kernel_eids.ATTR_AST_SELECTOR, ref_mod.mkEid(kernel_eids.SEL_PLUS), MODULE_EID);
    _ = try store.assert(11000, kernel_eids.ATTR_AST_FIRST_ARG, ref_mod.mkEid(11002), MODULE_EID);

    var m = try compiler.compileMethod(testing.allocator, &store, 11000);
    defer m.deinit(testing.allocator);

    // PUSH 3 (5), PUSH 4 (5), SEND_1 (7), RETURN (1) = 18 bytes
    try testing.expectEqual(@as(usize, 18), m.bytecode.len);
    try testing.expectEqual(bytecode.Opcode.PUSH_SMALLINT_I32, bytecode.readOpcode(m.bytecode, 0));
    try testing.expectEqual(bytecode.Opcode.PUSH_SMALLINT_I32, bytecode.readOpcode(m.bytecode, 5));
    try testing.expectEqual(bytecode.Opcode.SEND_1, bytecode.readOpcode(m.bytecode, 10));
    try testing.expectEqual(@as(u32, kernel_eids.SEL_PLUS), bytecode.readU32(m.bytecode, 11));
    try testing.expectEqual(bytecode.Opcode.RETURN, bytecode.readOpcode(m.bytecode, 17));
}

test "compile keyword send with 2 args: SEND_N arg_count=2" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    try mkLiteralAst(&store, 12001, ref_mod.mkSmallInt(1).?); // receiver
    try mkLiteralAst(&store, 12002, ref_mod.mkSmallInt(2).?); // arg1
    try mkLiteralAst(&store, 12003, ref_mod.mkSmallInt(3).?); // arg2
    _ = try store.assert(12002, kernel_eids.ATTR_AST_NEXT_SIBLING, ref_mod.mkEid(12003), MODULE_EID);

    _ = try store.assert(12000, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_KEYWORD_SEND_VT), MODULE_EID);
    _ = try store.assert(12000, kernel_eids.ATTR_AST_RECEIVER, ref_mod.mkEid(12001), MODULE_EID);
    _ = try store.assert(12000, kernel_eids.ATTR_AST_SELECTOR, ref_mod.mkEid(kernel_eids.SEL_IF_TRUE_IF_FALSE), MODULE_EID);
    _ = try store.assert(12000, kernel_eids.ATTR_AST_FIRST_ARG, ref_mod.mkEid(12002), MODULE_EID);

    var m = try compiler.compileMethod(testing.allocator, &store, 12000);
    defer m.deinit(testing.allocator);

    // PUSH 1, PUSH 2, PUSH 3, SEND_N sel=SEL_IF_TRUE_IF_FALSE arg=2 site=0, RETURN
    try testing.expectEqual(@as(usize, 24), m.bytecode.len);
    try testing.expectEqual(bytecode.Opcode.SEND_N, bytecode.readOpcode(m.bytecode, 15));
    try testing.expectEqual(@as(u32, kernel_eids.SEL_IF_TRUE_IF_FALSE), bytecode.readU32(m.bytecode, 16));
    try testing.expectEqual(@as(u8, 2), bytecode.readU8(m.bytecode, 20));
    try testing.expectEqual(@as(u16, 0), bytecode.readU16(m.bytecode, 21));
    try testing.expectEqual(bytecode.Opcode.RETURN, bytecode.readOpcode(m.bytecode, 23));
}

test "compile nested sends: site_idx increments per send" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    // (1 + 2) * 3 — two binary sends, expect site_idx 0 and 1.
    try mkLiteralAst(&store, 13001, ref_mod.mkSmallInt(1).?);
    try mkLiteralAst(&store, 13002, ref_mod.mkSmallInt(2).?);
    try mkLiteralAst(&store, 13003, ref_mod.mkSmallInt(3).?);

    // inner: 1 + 2
    const inner = 13100;
    _ = try store.assert(inner, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_BINARY_SEND_VT), MODULE_EID);
    _ = try store.assert(inner, kernel_eids.ATTR_AST_RECEIVER, ref_mod.mkEid(13001), MODULE_EID);
    _ = try store.assert(inner, kernel_eids.ATTR_AST_SELECTOR, ref_mod.mkEid(kernel_eids.SEL_PLUS), MODULE_EID);
    _ = try store.assert(inner, kernel_eids.ATTR_AST_FIRST_ARG, ref_mod.mkEid(13002), MODULE_EID);

    // outer: inner * 3
    const outer = 13101;
    _ = try store.assert(outer, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_BINARY_SEND_VT), MODULE_EID);
    _ = try store.assert(outer, kernel_eids.ATTR_AST_RECEIVER, ref_mod.mkEid(inner), MODULE_EID);
    _ = try store.assert(outer, kernel_eids.ATTR_AST_SELECTOR, ref_mod.mkEid(kernel_eids.SEL_TIMES), MODULE_EID);
    _ = try store.assert(outer, kernel_eids.ATTR_AST_FIRST_ARG, ref_mod.mkEid(13003), MODULE_EID);

    var m = try compiler.compileMethod(testing.allocator, &store, outer);
    defer m.deinit(testing.allocator);

    try testing.expectEqual(@as(u16, 2), m.send_site_count);
    // First send (inner) site = 0; second send (outer) site = 1.
    // inner SEND_1 at offset 10; outer SEND_1 after another PUSH (5 bytes) at 22.
    try testing.expectEqual(bytecode.Opcode.SEND_1, bytecode.readOpcode(m.bytecode, 10));
    try testing.expectEqual(@as(u16, 0), bytecode.readU16(m.bytecode, 15));
    try testing.expectEqual(bytecode.Opcode.SEND_1, bytecode.readOpcode(m.bytecode, 22));
    try testing.expectEqual(@as(u16, 1), bytecode.readU16(m.bytecode, 27));
}

test "materialize: persists method entity with bytecode + annotations" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());

    try mkLiteralAst(&store, 20001, ref_mod.mkSmallInt(42).?);
    var m = try compiler.compileMethod(arena.allocator(), &store, 20001);
    // Don't deinit — arena owns it.

    const method_eid = try compiler.materialize(arena.allocator(), &store, &table, &m, MODULE_EID);
    try testing.expect(method_eid >= kernel_eids.USER_EID_START);

    // :vtable CLOSURE_VT
    const vt = store.currentValue(method_eid, kernel_eids.ATTR_VTABLE).?;
    try testing.expectEqual(ref_mod.mkEid(kernel_eids.CLOSURE_VT), vt);

    // :bytecode-stream — string Ref carrying the emitted bytes
    const bc_ref = store.currentValue(method_eid, kernel_eids.ATTR_BYTECODE_STREAM).?;
    try testing.expect(strings.isString(bc_ref));
    try testing.expectEqualSlices(u8, m.bytecode, strings.bytes(bc_ref));

    // :method-body-ast → ast eid
    const body = store.currentValue(method_eid, kernel_eids.ATTR_METHOD_BODY_AST).?;
    try testing.expectEqual(ref_mod.mkEid(20001), body);

    // sizing annotations
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(store.currentValue(method_eid, kernel_eids.ATTR_METHOD_ARG_COUNT).?));
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(store.currentValue(method_eid, kernel_eids.ATTR_METHOD_LOCAL_COUNT).?));
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(store.currentValue(method_eid, kernel_eids.ATTR_METHOD_SEND_SITE_COUNT).?));

    // No :constant-pool (empty pool)
    try testing.expect(store.currentValue(method_eid, kernel_eids.ATTR_CONSTANT_POOL) == null);
}

test "materialize: pool emitted when non-empty" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());

    // Big int forces pool entry.
    const big = ref_mod.mkSmallInt(@as(i64, std.math.maxInt(i32)) + 1).?;
    try mkLiteralAst(&store, 21001, big);
    var m = try compiler.compileMethod(arena.allocator(), &store, 21001);

    const method_eid = try compiler.materialize(arena.allocator(), &store, &table, &m, MODULE_EID);

    const pool_ref = store.currentValue(method_eid, kernel_eids.ATTR_CONSTANT_POOL).?;
    try testing.expect(boxed.isRrbRef(pool_ref));
    const rrb = boxed.untagRrbRef(pool_ref);
    try testing.expectEqual(@as(usize, 1), rrb.count());
    try testing.expectEqual(big, rrb.nth(0));
}

test "compile block: emits MAKE_BLOCK + child compiled method" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    // Block body = literal 99.
    try mkLiteralAst(&store, 30001, ref_mod.mkSmallInt(99).?);

    // [ 99 ] block — AST_BLOCK_VT with :ast-block-body → 30001.
    _ = try store.assert(30000, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_BLOCK_VT), MODULE_EID);
    _ = try store.assert(30000, kernel_eids.ATTR_AST_BLOCK_BODY, ref_mod.mkEid(30001), MODULE_EID);

    var m = try compiler.compileMethod(testing.allocator, &store, 30000);
    defer m.deinit(testing.allocator);

    // Outer method: MAKE_BLOCK 0, RETURN.
    try testing.expectEqual(@as(usize, 4), m.bytecode.len);
    try testing.expectEqual(bytecode.Opcode.MAKE_BLOCK, bytecode.readOpcode(m.bytecode, 0));
    try testing.expectEqual(@as(u16, 0), bytecode.readU16(m.bytecode, 1));
    try testing.expectEqual(bytecode.Opcode.RETURN, bytecode.readOpcode(m.bytecode, 3));

    // One block-child queued; pool reserved slot 0.
    try testing.expectEqual(@as(usize, 1), m.block_children.len);
    try testing.expectEqual(@as(u16, 0), m.block_children[0].pool_idx);
    try testing.expectEqual(@as(usize, 1), m.constant_pool.len);
    try testing.expectEqual(ref_mod.NIL, m.constant_pool[0]); // placeholder

    // Child bytecode = PUSH_SMALLINT_I32 99, RETURN.
    const child = m.block_children[0].compiled;
    try testing.expectEqual(@as(usize, 6), child.bytecode.len);
    try testing.expectEqual(bytecode.Opcode.PUSH_SMALLINT_I32, bytecode.readOpcode(child.bytecode, 0));
    try testing.expectEqual(@as(i32, 99), bytecode.readI32(child.bytecode, 1));
}

test "materialize block: patches pool + asserts :block-parent-method" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());
    var table = object_table_mod.ObjectTable.init(arena.allocator());

    try mkLiteralAst(&store, 31001, ref_mod.mkSmallInt(7).?);
    _ = try store.assert(31000, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_BLOCK_VT), MODULE_EID);
    _ = try store.assert(31000, kernel_eids.ATTR_AST_BLOCK_BODY, ref_mod.mkEid(31001), MODULE_EID);

    var m = try compiler.compileMethod(arena.allocator(), &store, 31000);
    const parent_eid = try compiler.materialize(arena.allocator(), &store, &table, &m, MODULE_EID);

    // Parent's constant_pool[0] should now hold mkEid(child-method-eid).
    const pool_ref = store.currentValue(parent_eid, kernel_eids.ATTR_CONSTANT_POOL).?;
    const rrb = boxed.untagRrbRef(pool_ref);
    try testing.expectEqual(@as(usize, 1), rrb.count());
    const child_ref = rrb.nth(0);
    try testing.expect(ref_mod.isEid(child_ref));
    const child_eid = ref_mod.untagEid(child_ref);

    // Child carries :block-parent-method back to parent.
    const parent_back = store.currentValue(child_eid, kernel_eids.ATTR_BLOCK_PARENT_METHOD).?;
    try testing.expectEqual(ref_mod.mkEid(parent_eid), parent_back);

    // Child has its own bytecode entity.
    const child_bc = store.currentValue(child_eid, kernel_eids.ATTR_BYTECODE_STREAM).?;
    try testing.expect(strings.isString(child_bc));
}

test "compile rejects unknown AST kind" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    // AST_SUPER_SEND_VT not yet supported.
    _ = try store.assert(4000, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_SUPER_SEND_VT), MODULE_EID);
    const r = compiler.compileMethod(testing.allocator, &store, 4000);
    try testing.expectError(error.UnknownAstKind, r);
}

test "compile let + var-ref: stores then reloads from slot 0" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    const name_eid: u64 = 800; // arbitrary; treated as opaque name EID
    // let x = 5
    try mkLiteralAst(&store, 40001, ref_mod.mkSmallInt(5).?);
    _ = try store.assert(40002, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_LET_VT), MODULE_EID);
    _ = try store.assert(40002, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(name_eid), MODULE_EID);
    _ = try store.assert(40002, kernel_eids.ATTR_AST_LET_INIT, ref_mod.mkEid(40001), MODULE_EID);

    // x (var-ref)
    _ = try store.assert(40003, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_VAR_REF_VT), MODULE_EID);
    _ = try store.assert(40003, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(name_eid), MODULE_EID);

    // stmt-seq: let x = 5; x
    _ = try store.assert(40002, kernel_eids.ATTR_AST_NEXT_SIBLING, ref_mod.mkEid(40003), MODULE_EID);
    _ = try store.assert(40000, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_STMT_SEQ_VT), MODULE_EID);
    _ = try store.assert(40000, kernel_eids.ATTR_AST_STMT_FIRST, ref_mod.mkEid(40002), MODULE_EID);

    var m = try compiler.compileMethod(testing.allocator, &store, 40000);
    defer m.deinit(testing.allocator);

    try testing.expectEqual(@as(u8, 1), m.local_count); // one local
    // Bytecode: PUSH 5 (5), STORE_LOCAL 0 (2), PUSH_LOCAL 0 (2),
    //           POP (1), PUSH_LOCAL 0 (2), RETURN (1) = 13.
    try testing.expectEqual(@as(usize, 13), m.bytecode.len);
    try testing.expectEqual(bytecode.Opcode.PUSH_SMALLINT_I32, bytecode.readOpcode(m.bytecode, 0));
    try testing.expectEqual(bytecode.Opcode.STORE_LOCAL, bytecode.readOpcode(m.bytecode, 5));
    try testing.expectEqual(@as(u8, 0), bytecode.readU8(m.bytecode, 6));
    try testing.expectEqual(bytecode.Opcode.PUSH_LOCAL, bytecode.readOpcode(m.bytecode, 7));
    try testing.expectEqual(bytecode.Opcode.POP, bytecode.readOpcode(m.bytecode, 9));
    try testing.expectEqual(bytecode.Opcode.PUSH_LOCAL, bytecode.readOpcode(m.bytecode, 10));
    try testing.expectEqual(@as(u8, 0), bytecode.readU8(m.bytecode, 11));
    try testing.expectEqual(bytecode.Opcode.RETURN, bytecode.readOpcode(m.bytecode, 12));
}

test "compile var-ref out of scope → VarNotInScope" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    _ = try store.assert(41000, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_VAR_REF_VT), MODULE_EID);
    _ = try store.assert(41000, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(900), MODULE_EID);

    const r = compiler.compileMethod(testing.allocator, &store, 41000);
    try testing.expectError(error.VarNotInScope, r);
}

test "compile assign: stores + reloads bound slot" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store_mod.FactStore.init(arena.allocator());

    const name_eid: u64 = 850;
    // let y = 1; y := 99; y
    try mkLiteralAst(&store, 42001, ref_mod.mkSmallInt(1).?);
    _ = try store.assert(42002, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_LET_VT), MODULE_EID);
    _ = try store.assert(42002, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(name_eid), MODULE_EID);
    _ = try store.assert(42002, kernel_eids.ATTR_AST_LET_INIT, ref_mod.mkEid(42001), MODULE_EID);

    try mkLiteralAst(&store, 42003, ref_mod.mkSmallInt(99).?);
    _ = try store.assert(42004, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_ASSIGN_VT), MODULE_EID);
    _ = try store.assert(42004, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(name_eid), MODULE_EID);
    _ = try store.assert(42004, kernel_eids.ATTR_AST_ASSIGN_VALUE, ref_mod.mkEid(42003), MODULE_EID);

    _ = try store.assert(42005, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_VAR_REF_VT), MODULE_EID);
    _ = try store.assert(42005, kernel_eids.ATTR_AST_NAME, ref_mod.mkEid(name_eid), MODULE_EID);

    _ = try store.assert(42002, kernel_eids.ATTR_AST_NEXT_SIBLING, ref_mod.mkEid(42004), MODULE_EID);
    _ = try store.assert(42004, kernel_eids.ATTR_AST_NEXT_SIBLING, ref_mod.mkEid(42005), MODULE_EID);

    _ = try store.assert(42000, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.AST_STMT_SEQ_VT), MODULE_EID);
    _ = try store.assert(42000, kernel_eids.ATTR_AST_STMT_FIRST, ref_mod.mkEid(42002), MODULE_EID);

    var m = try compiler.compileMethod(testing.allocator, &store, 42000);
    defer m.deinit(testing.allocator);

    try testing.expectEqual(@as(u8, 1), m.local_count);
}
