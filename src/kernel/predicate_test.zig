const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const predicate = @import("predicate.zig");
const query = @import("query.zig");

const Ref = ref_mod.Ref;
const Term = query.Term;
const Clause = query.Clause;

const M: u64 = 1;
const A_VALUE: u64 = 300;

const E1: u64 = 400;
const E2: u64 = 401;
const E3: u64 = 402;

fn boot(
    a: std.mem.Allocator,
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
) !void {
    store.* = fact_store.FactStore.init(a);
    table.* = object_table.ObjectTable.init(a);
    interner.* = interner_mod.Interner.init(a);
    try predicate.registerBuiltins(a, store, table, interner);
}

// -- Builtin arithmetic ------------------------------------------------------

test "builtin +: bind ?b = ?a + 1" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store: fact_store.FactStore = undefined;
    var table: object_table.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    try boot(a, &store, &table, &interner);

    _ = try store.assert(E1, A_VALUE, ref_mod.mkSmallInt(10).?, M);
    _ = try store.assert(E2, A_VALUE, ref_mod.mkSmallInt(20).?, M);

    var q = query.Query.init(a);
    defer q.deinit();
    const vn = try q.variable("n");
    const vb = try q.variable("b");
    try q.addFind(vb);
    try q.addTriple(.wildcard, .{ .constant = ref_mod.mkEid(A_VALUE) }, .{ .variable = vn });

    const fn_eid = interner.str_to_eid.get("+").?;
    const fn_args = try a.alloc(Term, 2);
    fn_args[0] = .{ .variable = vn };
    fn_args[1] = .{ .constant = ref_mod.mkSmallInt(1).? };
    try q.addFnCall(fn_eid, fn_args, vb);

    var r = try query.execute(&q, &store);
    defer r.deinit();

    try testing.expectEqual(@as(usize, 2), r.count());
    var seen_11 = false;
    var seen_21 = false;
    var i: usize = 0;
    while (i < r.count()) : (i += 1) {
        const v = r.at(i, 0);
        if (ref_mod.isSmallInt(v)) {
            const n = ref_mod.untagSmallInt(v);
            if (n == 11) seen_11 = true;
            if (n == 21) seen_21 = true;
        }
    }
    try testing.expect(seen_11 and seen_21);
}

test "builtin function chain: ?b = ?a + 1; ?c = ?b * 2" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store: fact_store.FactStore = undefined;
    var table: object_table.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    try boot(a, &store, &table, &interner);

    _ = try store.assert(E1, A_VALUE, ref_mod.mkSmallInt(5).?, M);

    var q = query.Query.init(a);
    defer q.deinit();
    const vn = try q.variable("n");
    const vb = try q.variable("b");
    const vc = try q.variable("c");
    try q.addFind(vc);
    try q.addTriple(.wildcard, .{ .constant = ref_mod.mkEid(A_VALUE) }, .{ .variable = vn });

    const add_eid = interner.str_to_eid.get("+").?;
    const add_args = try a.alloc(Term, 2);
    add_args[0] = .{ .variable = vn };
    add_args[1] = .{ .constant = ref_mod.mkSmallInt(1).? };
    try q.addFnCall(add_eid, add_args, vb);

    const mul_eid = interner.str_to_eid.get("*").?;
    const mul_args = try a.alloc(Term, 2);
    mul_args[0] = .{ .variable = vb };
    mul_args[1] = .{ .constant = ref_mod.mkSmallInt(2).? };
    try q.addFnCall(mul_eid, mul_args, vc);

    var r = try query.execute(&q, &store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 1), r.count());
    try testing.expectEqual(@as(i64, 12), ref_mod.untagSmallInt(r.at(0, 0)));
}

test "predicate over function result: filter ?b > 10" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store: fact_store.FactStore = undefined;
    var table: object_table.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    try boot(a, &store, &table, &interner);

    _ = try store.assert(E1, A_VALUE, ref_mod.mkSmallInt(5).?, M);
    _ = try store.assert(E2, A_VALUE, ref_mod.mkSmallInt(15).?, M);
    _ = try store.assert(E3, A_VALUE, ref_mod.mkSmallInt(25).?, M);

    var q = query.Query.init(a);
    defer q.deinit();
    const vn = try q.variable("n");
    const vb = try q.variable("b");
    try q.addFind(vb);
    try q.addTriple(.wildcard, .{ .constant = ref_mod.mkEid(A_VALUE) }, .{ .variable = vn });

    const add_eid = interner.str_to_eid.get("+").?;
    const add_args = try a.alloc(Term, 2);
    add_args[0] = .{ .variable = vn };
    add_args[1] = .{ .constant = ref_mod.mkSmallInt(1).? };
    try q.addFnCall(add_eid, add_args, vb);
    try q.addPredicate(.gt, .{ .variable = vb }, .{ .constant = ref_mod.mkSmallInt(10).? });

    var r = try query.execute(&q, &store);
    defer r.deinit();
    // 5+1=6 drop, 15+1=16 keep, 25+1=26 keep → 2
    try testing.expectEqual(@as(usize, 2), r.count());
}

// -- User-registered native predicate ---------------------------------------

fn isEvenPred(args: [*]const Ref, n: usize) callconv(.c) bool {
    if (n != 1) return false;
    if (!ref_mod.isSmallInt(args[0])) return false;
    return @mod(ref_mod.untagSmallInt(args[0]), 2) == 0;
}

test "user predicate: even? filters odd numbers out" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store: fact_store.FactStore = undefined;
    var table: object_table.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    try boot(a, &store, &table, &interner);

    _ = try store.assert(E1, A_VALUE, ref_mod.mkSmallInt(2).?, M);
    _ = try store.assert(E2, A_VALUE, ref_mod.mkSmallInt(3).?, M);
    _ = try store.assert(E3, A_VALUE, ref_mod.mkSmallInt(4).?, M);

    const name_eid = try interner.intern(&store, &table, "even?");
    _ = try predicate.definePredNative(a, &store, &table, name_eid, &isEvenPred);

    var q = query.Query.init(a);
    defer q.deinit();
    const vn = try q.variable("n");
    try q.addFind(vn);
    try q.addTriple(.wildcard, .{ .constant = ref_mod.mkEid(A_VALUE) }, .{ .variable = vn });

    const pred_args = try a.alloc(Term, 1);
    pred_args[0] = .{ .variable = vn };
    try q.addPredCall(name_eid, pred_args);

    var r = try query.execute(&q, &store);
    defer r.deinit();

    try testing.expectEqual(@as(usize, 2), r.count());
    var saw_2 = false;
    var saw_4 = false;
    var i: usize = 0;
    while (i < r.count()) : (i += 1) {
        const v = r.at(i, 0);
        if (ref_mod.untagSmallInt(v) == 2) saw_2 = true;
        if (ref_mod.untagSmallInt(v) == 4) saw_4 = true;
    }
    try testing.expect(saw_2 and saw_4);
}

test "user predicate: missing impl → clause drops binding" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store: fact_store.FactStore = undefined;
    var table: object_table.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    try boot(a, &store, &table, &interner);

    _ = try store.assert(E1, A_VALUE, ref_mod.mkSmallInt(2).?, M);

    var q = query.Query.init(a);
    defer q.deinit();
    const vn = try q.variable("n");
    try q.addFind(vn);
    try q.addTriple(.wildcard, .{ .constant = ref_mod.mkEid(A_VALUE) }, .{ .variable = vn });

    // Unregistered name-EID → invokePred returns null → binding drops.
    const pred_args = try a.alloc(Term, 1);
    pred_args[0] = .{ .variable = vn };
    try q.addPredCall(999_999, pred_args);

    var r = try query.execute(&q, &store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 0), r.count());
}

// -- min2 / max2 -------------------------------------------------------------

test "builtin min2 / max2" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store: fact_store.FactStore = undefined;
    var table: object_table.ObjectTable = undefined;
    var interner: interner_mod.Interner = undefined;
    try boot(a, &store, &table, &interner);

    _ = try store.assert(E1, A_VALUE, ref_mod.mkSmallInt(7).?, M);

    var q = query.Query.init(a);
    defer q.deinit();
    const vn = try q.variable("n");
    const vmin = try q.variable("mn");
    const vmax = try q.variable("mx");
    try q.addFind(vmin);
    try q.addFind(vmax);
    try q.addTriple(.wildcard, .{ .constant = ref_mod.mkEid(A_VALUE) }, .{ .variable = vn });

    const min_eid = interner.str_to_eid.get("min2").?;
    const max_eid = interner.str_to_eid.get("max2").?;
    const mn_args = try a.alloc(Term, 2);
    mn_args[0] = .{ .variable = vn };
    mn_args[1] = .{ .constant = ref_mod.mkSmallInt(5).? };
    try q.addFnCall(min_eid, mn_args, vmin);

    const mx_args = try a.alloc(Term, 2);
    mx_args[0] = .{ .variable = vn };
    mx_args[1] = .{ .constant = ref_mod.mkSmallInt(5).? };
    try q.addFnCall(max_eid, mx_args, vmax);

    var r = try query.execute(&q, &store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 1), r.count());
    try testing.expectEqual(@as(i64, 5), ref_mod.untagSmallInt(r.at(0, 0)));
    try testing.expectEqual(@as(i64, 7), ref_mod.untagSmallInt(r.at(0, 1)));
}
