const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const fact_store = @import("fact_store.zig");
const query = @import("query.zig");
const aggregate = @import("aggregate.zig");

const Ref = ref_mod.Ref;

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

// -- count -------------------------------------------------------------------

test "count over single attribute returns total distinct values" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    _ = try store.assert(E1, A_NAME, smallInt(10), M);
    _ = try store.assert(E2, A_NAME, smallInt(20), M);
    _ = try store.assert(E3, A_NAME, smallInt(30), M);

    var q = query.Query.init(a);
    defer q.deinit();
    const ve = try q.variable("e");
    try q.addFindAggregate(.count, ve);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_NAME) }, .{ .wildcard = {} });

    var r = try query.execute(&q, &store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 1), r.count()); // universal group
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(r.at(0, 0)));
}

test "count is distinct (Date discipline)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    // E1 and E2 both have name=7 (same value).
    _ = try store.assert(E1, A_NAME, smallInt(7), M);
    _ = try store.assert(E2, A_NAME, smallInt(7), M);
    _ = try store.assert(E3, A_NAME, smallInt(8), M);

    var q = query.Query.init(a);
    defer q.deinit();
    const vn = try q.variable("n");
    const ve = try q.variable("e");
    try q.addFindAggregate(.count, vn);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_NAME) }, .{ .variable = vn });

    var r = try query.execute(&q, &store);
    defer r.deinit();
    // Distinct ?n values: {7, 8} → 2.
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(r.at(0, 0)));
}

// -- group-by ---------------------------------------------------------------

test "implicit group-by on the non-aggregate find var" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    // Model: each method m has a :vtable attribute pointing at its vtable.
    // Count distinct methods per vtable.
    const A_VT: u64 = 700;
    const VT1: u64 = 500;
    const VT2: u64 = 501;
    const M1: u64 = 800;
    const M2: u64 = 801;
    const M3: u64 = 802;
    _ = try store.assert(M1, A_VT, ref_mod.mkEid(VT1), M);
    _ = try store.assert(M2, A_VT, ref_mod.mkEid(VT1), M);
    _ = try store.assert(M3, A_VT, ref_mod.mkEid(VT2), M);

    var q = query.Query.init(a);
    defer q.deinit();
    const vt = try q.variable("vt");
    const m = try q.variable("m");
    try q.addFind(vt);
    try q.addFindAggregate(.count, m);
    try q.addTriple(.{ .variable = m }, .{ .constant = ref_mod.mkEid(A_VT) }, .{ .variable = vt });

    var r = try query.execute(&q, &store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 2), r.count()); // 2 groups: VT1, VT2
    try testing.expectEqual(@as(u32, 2), r.arity);

    var total: i64 = 0;
    var i: usize = 0;
    while (i < r.count()) : (i += 1) {
        total += ref_mod.untagSmallInt(r.at(i, 1));
    }
    try testing.expectEqual(@as(i64, 3), total);
}

// -- sum / max / min / avg --------------------------------------------------

test "sum over distinct values" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    _ = try store.assert(E1, A_AGE, smallInt(10), M);
    _ = try store.assert(E2, A_AGE, smallInt(20), M);
    _ = try store.assert(E3, A_AGE, smallInt(30), M);

    var q = query.Query.init(a);
    defer q.deinit();
    const ve = try q.variable("e");
    const va = try q.variable("a");
    try q.addFindAggregate(.sum, va);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_AGE) }, .{ .variable = va });

    var r = try query.execute(&q, &store);
    defer r.deinit();
    try testing.expectEqual(@as(i64, 60), ref_mod.untagSmallInt(r.at(0, 0)));
}

test "max / min over distinct values" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    _ = try store.assert(E1, A_AGE, smallInt(-5), M);
    _ = try store.assert(E2, A_AGE, smallInt(42), M);
    _ = try store.assert(E3, A_AGE, smallInt(17), M);

    var q_max = query.Query.init(a);
    defer q_max.deinit();
    const e1 = try q_max.variable("e");
    const v1 = try q_max.variable("a");
    try q_max.addFindAggregate(.max, v1);
    try q_max.addTriple(.{ .variable = e1 }, .{ .constant = ref_mod.mkEid(A_AGE) }, .{ .variable = v1 });
    var r_max = try query.execute(&q_max, &store);
    defer r_max.deinit();
    try testing.expectEqual(@as(i64, 42), ref_mod.untagSmallInt(r_max.at(0, 0)));

    var q_min = query.Query.init(a);
    defer q_min.deinit();
    const e2 = try q_min.variable("e");
    const v2 = try q_min.variable("a");
    try q_min.addFindAggregate(.min, v2);
    try q_min.addTriple(.{ .variable = e2 }, .{ .constant = ref_mod.mkEid(A_AGE) }, .{ .variable = v2 });
    var r_min = try query.execute(&q_min, &store);
    defer r_min.deinit();
    try testing.expectEqual(@as(i64, -5), ref_mod.untagSmallInt(r_min.at(0, 0)));
}

test "avg returns truncated mean" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    _ = try store.assert(E1, A_AGE, smallInt(10), M);
    _ = try store.assert(E2, A_AGE, smallInt(20), M);
    _ = try store.assert(E3, A_AGE, smallInt(33), M);

    var q = query.Query.init(a);
    defer q.deinit();
    const ve = try q.variable("e");
    const va = try q.variable("a");
    try q.addFindAggregate(.avg, va);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_AGE) }, .{ .variable = va });

    var r = try query.execute(&q, &store);
    defer r.deinit();
    // (10+20+33)/3 = 21
    try testing.expectEqual(@as(i64, 21), ref_mod.untagSmallInt(r.at(0, 0)));
}

// -- type strictness --------------------------------------------------------

test "sum over non-numeric raises NonNumericInAggregate" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    _ = try store.assert(E1, A_NAME, ref_mod.mkEid(999), M);

    var q = query.Query.init(a);
    defer q.deinit();
    const ve = try q.variable("e");
    const vn = try q.variable("n");
    try q.addFindAggregate(.sum, vn);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_NAME) }, .{ .variable = vn });

    const result = query.execute(&q, &store);
    try testing.expectError(error.NonNumericInAggregate, result);
}

// -- multi-aggregate --------------------------------------------------------

test "multiple aggregates over the same group" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    _ = try store.assert(E1, A_AGE, smallInt(10), M);
    _ = try store.assert(E2, A_AGE, smallInt(20), M);
    _ = try store.assert(E3, A_AGE, smallInt(30), M);

    var q = query.Query.init(a);
    defer q.deinit();
    const ve = try q.variable("e");
    const va = try q.variable("a");
    try q.addFindAggregate(.count, va);
    try q.addFindAggregate(.sum, va);
    try q.addFindAggregate(.min, va);
    try q.addFindAggregate(.max, va);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_AGE) }, .{ .variable = va });

    var r = try query.execute(&q, &store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 1), r.count());
    try testing.expectEqual(@as(u32, 4), r.arity);
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(r.at(0, 0))); // count
    try testing.expectEqual(@as(i64, 60), ref_mod.untagSmallInt(r.at(0, 1))); // sum
    try testing.expectEqual(@as(i64, 10), ref_mod.untagSmallInt(r.at(0, 2))); // min
    try testing.expectEqual(@as(i64, 30), ref_mod.untagSmallInt(r.at(0, 3))); // max
}
