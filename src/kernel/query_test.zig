const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const fact_store = @import("fact_store.zig");
const query = @import("query.zig");

const Ref = ref_mod.Ref;
const Term = query.Term;

const M: u64 = 1;
const E1: u64 = 100;
const E2: u64 = 101;
const E3: u64 = 102;
const A_NAME: u64 = 200;
const A_AGE: u64 = 201;
const A_KIND: u64 = 202;

fn smallInt(n: i64) Ref {
    return ref_mod.mkSmallInt(n).?;
}

fn setupStore(arena: *std.heap.ArenaAllocator) !*fact_store.FactStore {
    const store = try arena.allocator().create(fact_store.FactStore);
    store.* = fact_store.FactStore.init(arena.allocator());
    return store;
}

// -- Single triple: bind one var --------------------------------------------

test "single triple [?e :name <ref>] finds matching entity" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const store = try setupStore(&arena);
    const point_name = ref_mod.mkEid(999);
    _ = try store.assert(E1, A_NAME, point_name, M);
    _ = try store.assert(E2, A_NAME, ref_mod.mkEid(998), M);

    var q = query.Query.init(arena.allocator());
    defer q.deinit();
    const ve = try q.variable("e");
    try q.addFind(ve);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_NAME) }, .{ .constant = point_name });

    var r = try query.execute(&q, store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 1), r.count());
    try testing.expectEqual(ref_mod.mkEid(E1), r.at(0, 0));
}

// -- Conjunction: multi-clause join ----------------------------------------

test "two-clause join: entities with both name and age" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const store = try setupStore(&arena);
    _ = try store.assert(E1, A_NAME, smallInt(1), M);
    _ = try store.assert(E1, A_AGE, smallInt(30), M);
    _ = try store.assert(E2, A_NAME, smallInt(2), M); // no age

    var q = query.Query.init(arena.allocator());
    defer q.deinit();
    const ve = try q.variable("e");
    const vn = try q.variable("n");
    const va = try q.variable("a");
    try q.addFind(ve);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_NAME) }, .{ .variable = vn });
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_AGE) }, .{ .variable = va });

    var r = try query.execute(&q, store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 1), r.count());
    try testing.expectEqual(ref_mod.mkEid(E1), r.at(0, 0));
}

// -- Multi-var find ---------------------------------------------------------

test "multi-var find returns tuples" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const store = try setupStore(&arena);
    _ = try store.assert(E1, A_NAME, smallInt(100), M);
    _ = try store.assert(E2, A_NAME, smallInt(200), M);

    var q = query.Query.init(arena.allocator());
    defer q.deinit();
    const ve = try q.variable("e");
    const vn = try q.variable("n");
    try q.addFind(ve);
    try q.addFind(vn);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_NAME) }, .{ .variable = vn });

    var r = try query.execute(&q, store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 2), r.count());
    try testing.expectEqual(@as(u32, 2), r.arity);
}

// -- Predicate -------------------------------------------------------------

test "predicate filters by inequality on SmallInts" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const store = try setupStore(&arena);
    _ = try store.assert(E1, A_AGE, smallInt(25), M);
    _ = try store.assert(E2, A_AGE, smallInt(35), M);
    _ = try store.assert(E3, A_AGE, smallInt(30), M);

    var q = query.Query.init(arena.allocator());
    defer q.deinit();
    const ve = try q.variable("e");
    const va = try q.variable("a");
    try q.addFind(ve);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_AGE) }, .{ .variable = va });
    try q.addPredicate(.lt, .{ .variable = va }, .{ .constant = smallInt(30) });

    var r = try query.execute(&q, store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 1), r.count());
    try testing.expectEqual(ref_mod.mkEid(E1), r.at(0, 0));
}

// -- Negation --------------------------------------------------------------

test "negation excludes entities matching inner pattern" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const store = try setupStore(&arena);
    _ = try store.assert(E1, A_NAME, smallInt(1), M);
    _ = try store.assert(E2, A_NAME, smallInt(2), M);
    _ = try store.assert(E1, A_AGE, smallInt(30), M); // E1 has age; E2 does not.

    var q = query.Query.init(arena.allocator());
    defer q.deinit();
    const ve = try q.variable("e");
    const va = try q.variable("a");
    try q.addFind(ve);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_NAME) }, .{ .wildcard = {} });
    const inner = [_]query.Clause{
        .{ .triple = .{
            .e = .{ .variable = ve },
            .a = .{ .constant = ref_mod.mkEid(A_AGE) },
            .v = .{ .variable = va },
        } },
    };
    try q.addNegation(&inner);

    var r = try query.execute(&q, store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 1), r.count());
    try testing.expectEqual(ref_mod.mkEid(E2), r.at(0, 0));
}

// -- Disjunction -----------------------------------------------------------

test "disjunction unions branch results" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const store = try setupStore(&arena);
    const k_point = smallInt(1);
    const k_vector = smallInt(2);
    const k_circle = smallInt(3);
    _ = try store.assert(E1, A_KIND, k_point, M);
    _ = try store.assert(E2, A_KIND, k_vector, M);
    _ = try store.assert(E3, A_KIND, k_circle, M);

    var q = query.Query.init(arena.allocator());
    defer q.deinit();
    const ve = try q.variable("e");
    try q.addFind(ve);

    const branch_a = [_]query.Clause{
        .{ .triple = .{ .e = .{ .variable = ve }, .a = .{ .constant = ref_mod.mkEid(A_KIND) }, .v = .{ .constant = k_point } } },
    };
    const branch_b = [_]query.Clause{
        .{ .triple = .{ .e = .{ .variable = ve }, .a = .{ .constant = ref_mod.mkEid(A_KIND) }, .v = .{ .constant = k_vector } } },
    };
    const branches = [_][]const query.Clause{ &branch_a, &branch_b };
    try q.addDisjunction(&branches);

    var r = try query.execute(&q, store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 2), r.count());
}

// -- Set semantics ---------------------------------------------------------

test "duplicate-producing clauses are deduplicated by set semantics" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const store = try setupStore(&arena);
    _ = try store.assert(E1, A_NAME, smallInt(7), M);
    _ = try store.assert(E1, A_AGE, smallInt(30), M);

    // Two clauses that bind ?e to the same thing — naive join would produce
    // duplicates that the projection step should collapse.
    var q = query.Query.init(arena.allocator());
    defer q.deinit();
    const ve = try q.variable("e");
    try q.addFind(ve);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_NAME) }, .{ .wildcard = {} });
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_AGE) }, .{ .wildcard = {} });

    var r = try query.execute(&q, store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 1), r.count());
    try testing.expectEqual(ref_mod.mkEid(E1), r.at(0, 0));
}

// -- History (asOf) --------------------------------------------------------

test "asOf modifier filters to a historical view" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const store = try setupStore(&arena);
    const tx1 = try store.assert(E1, A_AGE, smallInt(20), M);
    _ = try store.assert(E1, A_AGE, smallInt(30), M);

    var q = query.Query.init(arena.allocator());
    defer q.deinit();
    const ve = try q.variable("e");
    const va = try q.variable("a");
    try q.addFind(va);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_AGE) }, .{ .variable = va });
    q.setAsOf(tx1);

    var r = try query.execute(&q, store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 1), r.count());
    try testing.expectEqual(smallInt(20), r.at(0, 0));
}

// -- Empty result ----------------------------------------------------------

test "no match returns empty result" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const store = try setupStore(&arena);

    var q = query.Query.init(arena.allocator());
    defer q.deinit();
    const ve = try q.variable("e");
    try q.addFind(ve);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_NAME) }, .{ .constant = smallInt(999) });

    var r = try query.execute(&q, store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 0), r.count());
}

// -- Wildcard --------------------------------------------------------------

test "wildcard does not bind, but matches" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const store = try setupStore(&arena);
    _ = try store.assert(E1, A_NAME, smallInt(1), M);
    _ = try store.assert(E2, A_NAME, smallInt(2), M);

    var q = query.Query.init(arena.allocator());
    defer q.deinit();
    const ve = try q.variable("e");
    try q.addFind(ve);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_NAME) }, .{ .wildcard = {} });

    var r = try query.execute(&q, store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 2), r.count());
}
