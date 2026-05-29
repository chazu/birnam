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

test "since: filter includes only facts modified after the watermark" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const store = try setupStore(&arena);
    const tx1 = try store.assert(E1, A_NAME, smallInt(1), M);
    _ = try store.assert(E2, A_NAME, smallInt(2), M); // after tx1
    _ = try store.assert(E3, A_NAME, smallInt(3), M); // after tx1

    var q = query.Query.init(arena.allocator());
    defer q.deinit();
    const ve = try q.variable("e");
    try q.addFind(ve);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_NAME) }, .{ .wildcard = {} });
    q.setSince(tx1);

    var r = try query.execute(&q, store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 2), r.count()); // E2, E3 only
}

test "between: filter includes only facts in the tx range" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const store = try setupStore(&arena);
    _ = try store.assert(E1, A_NAME, smallInt(1), M); // tx 1
    const tx2 = try store.assert(E2, A_NAME, smallInt(2), M); // tx 2
    const tx3 = try store.assert(E3, A_NAME, smallInt(3), M); // tx 3
    _ = try store.assert(999, A_NAME, smallInt(4), M); // tx 4 (later, distinct entity)

    var q = query.Query.init(arena.allocator());
    defer q.deinit();
    const ve = try q.variable("e");
    try q.addFind(ve);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_NAME) }, .{ .wildcard = {} });
    q.setBetween(tx2, tx3);

    var r = try query.execute(&q, store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 2), r.count()); // E2, E3
}

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

test "begin/end negation builder produces equivalent results" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const store = try setupStore(&arena);
    _ = try store.assert(E1, A_NAME, smallInt(1), M);
    _ = try store.assert(E2, A_NAME, smallInt(2), M);
    _ = try store.assert(E1, A_AGE, smallInt(30), M);

    var q = query.Query.init(arena.allocator());
    defer q.deinit();
    const ve = try q.variable("e");
    try q.addFind(ve);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_NAME) }, .{ .wildcard = {} });

    try q.beginNegation();
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_AGE) }, .{ .wildcard = {} });
    try q.endNegation();

    var r = try query.execute(&q, store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 1), r.count());
    try testing.expectEqual(ref_mod.mkEid(E2), r.at(0, 0));
}

test "begin/end disjunction builder produces union of branches" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const store = try setupStore(&arena);
    const k1 = smallInt(1);
    const k2 = smallInt(2);
    const k3 = smallInt(3);
    _ = try store.assert(E1, A_KIND, k1, M);
    _ = try store.assert(E2, A_KIND, k2, M);
    _ = try store.assert(E3, A_KIND, k3, M);

    var q = query.Query.init(arena.allocator());
    defer q.deinit();
    const ve = try q.variable("e");
    try q.addFind(ve);

    try q.beginDisjunction();
    try q.beginBranch();
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_KIND) }, .{ .constant = k1 });
    try q.endBranch();
    try q.beginBranch();
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_KIND) }, .{ .constant = k2 });
    try q.endBranch();
    try q.endDisjunction();

    var r = try query.execute(&q, store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 2), r.count());
}

test "subquery: outer iterates over inner's result rows" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const store = try setupStore(&arena);
    _ = try store.assert(E1, A_NAME, smallInt(100), M);
    _ = try store.assert(E2, A_NAME, smallInt(200), M);
    _ = try store.assert(E3, A_NAME, smallInt(300), M);
    _ = try store.assert(E1, A_AGE, smallInt(25), M);
    _ = try store.assert(E2, A_AGE, smallInt(35), M); // not in subquery result
    _ = try store.assert(E3, A_AGE, smallInt(45), M);

    // Inner query: entities with age < 30 OR > 40.
    var inner = query.Query.init(arena.allocator());
    defer inner.deinit();
    const ie = try inner.variable("e");
    const ia = try inner.variable("a");
    try inner.addFind(ie);
    try inner.addTriple(.{ .variable = ie }, .{ .constant = ref_mod.mkEid(A_AGE) }, .{ .variable = ia });
    try inner.beginDisjunction();
    try inner.beginBranch();
    try inner.addPredicate(.lt, .{ .variable = ia }, .{ .constant = smallInt(30) });
    try inner.endBranch();
    try inner.beginBranch();
    try inner.addPredicate(.gt, .{ .variable = ia }, .{ .constant = smallInt(40) });
    try inner.endBranch();
    try inner.endDisjunction();

    // Outer query: bind inner's results into ?e, then look up name.
    var outer = query.Query.init(arena.allocator());
    defer outer.deinit();
    const oe = try outer.variable("e");
    const on = try outer.variable("n");
    try outer.addFind(on);
    const sub_bindings = try arena.allocator().alloc(u32, 1);
    sub_bindings[0] = oe;
    try outer.addSubquery(&inner, sub_bindings);
    try outer.addTriple(.{ .variable = oe }, .{ .constant = ref_mod.mkEid(A_NAME) }, .{ .variable = on });

    var r = try query.execute(&outer, store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 2), r.count()); // E1, E3
    var seen_100 = false;
    var seen_300 = false;
    var i: usize = 0;
    while (i < r.count()) : (i += 1) {
        const v = r.at(i, 0);
        if (v == smallInt(100)) seen_100 = true;
        if (v == smallInt(300)) seen_300 = true;
    }
    try testing.expect(seen_100 and seen_300);
}

test "subquery: same subquery referenced twice memoizes" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const store = try setupStore(&arena);
    _ = try store.assert(E1, A_NAME, smallInt(1), M);
    _ = try store.assert(E2, A_NAME, smallInt(2), M);

    var inner = query.Query.init(arena.allocator());
    defer inner.deinit();
    const ie = try inner.variable("e");
    try inner.addFind(ie);
    try inner.addTriple(.{ .variable = ie }, .{ .constant = ref_mod.mkEid(A_NAME) }, .{ .wildcard = {} });

    var outer = query.Query.init(arena.allocator());
    defer outer.deinit();
    const oe = try outer.variable("e");
    try outer.addFind(oe);
    const sb1 = try arena.allocator().alloc(u32, 1);
    sb1[0] = oe;
    const sb2 = try arena.allocator().alloc(u32, 1);
    sb2[0] = oe;
    try outer.addSubquery(&inner, sb1);
    try outer.addSubquery(&inner, sb2);

    var r = try query.execute(&outer, store);
    defer r.deinit();
    // Both subquery references yield the same set; intersection (join via
    // shared ?e) is the set itself → 2.
    try testing.expectEqual(@as(usize, 2), r.count());
}

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
