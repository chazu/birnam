const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const object = @import("object.zig");
const boxed = @import("boxed.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const bootstrap = @import("bootstrap.zig");
const kernel_eids = @import("kernel_eids.zig");

const Ref = ref_mod.Ref;

const Fix = struct {
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
    many_ref: Ref,
};

fn setup(arena: *std.heap.ArenaAllocator) !Fix {
    const a = arena.allocator();
    const store = try a.create(fact_store.FactStore);
    store.* = fact_store.FactStore.init(a);
    const table = try a.create(object_table.ObjectTable);
    table.* = object_table.ObjectTable.init(a);
    const interner = try a.create(interner_mod.Interner);
    interner.* = interner_mod.Interner.init(a);
    _ = try bootstrap.boot(a, store, table, interner);
    const many_eid = interner.str_to_eid.get("many").?;
    return .{ .store = store, .table = table, .interner = interner, .many_ref = ref_mod.mkEid(many_eid) };
}

// User-range entity + attr EIDs for tests.
const E: u64 = 5_000;
const A_NAME: u64 = 5_100; // single-value test attr
const A_CHILDREN: u64 = 5_101; // many-value test attr
const V1: u64 = 5_200;
const V2: u64 = 5_201;
const V3: u64 = 5_202;

// -- Default cardinality (:one) --------------------------------------------

test "cardinality default: assert/retract preserves Phase 0-9 semantics" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const fix = try setup(&arena);

    _ = try fix.store.assert(E, A_NAME, ref_mod.mkEid(V1), kernel_eids.M_BOOTSTRAP);
    try testing.expectEqual(@as(?Ref, ref_mod.mkEid(V1)), fix.store.currentValue(E, A_NAME));

    // Implicit retract on overwrite (single-cardinality behavior).
    _ = try fix.store.assert(E, A_NAME, ref_mod.mkEid(V2), kernel_eids.M_BOOTSTRAP);
    try testing.expectEqual(@as(?Ref, ref_mod.mkEid(V2)), fix.store.currentValue(E, A_NAME));
    try testing.expect(!fix.store.hasFact(E, A_NAME, ref_mod.mkEid(V1)));

    try testing.expectEqual(fact_store.Cardinality.one, fix.store.cardinalityOfAttr(A_NAME));
}

// -- Many cardinality: append, no implicit retract -------------------------

test "cardinality :many: multiple asserts accumulate; currentValue returns RRB" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const fix = try setup(&arena);

    _ = try fix.store.assert(A_CHILDREN, kernel_eids.ATTR_CARDINALITY, fix.many_ref, kernel_eids.M_BOOTSTRAP);
    try testing.expectEqual(fact_store.Cardinality.many, fix.store.cardinalityOfAttr(A_CHILDREN));

    _ = try fix.store.assert(E, A_CHILDREN, ref_mod.mkEid(V1), kernel_eids.M_BOOTSTRAP);
    _ = try fix.store.assert(E, A_CHILDREN, ref_mod.mkEid(V2), kernel_eids.M_BOOTSTRAP);
    _ = try fix.store.assert(E, A_CHILDREN, ref_mod.mkEid(V3), kernel_eids.M_BOOTSTRAP);

    // No implicit retract — all three values currently present.
    try testing.expect(fix.store.hasFact(E, A_CHILDREN, ref_mod.mkEid(V1)));
    try testing.expect(fix.store.hasFact(E, A_CHILDREN, ref_mod.mkEid(V2)));
    try testing.expect(fix.store.hasFact(E, A_CHILDREN, ref_mod.mkEid(V3)));

    // currentValue returns an RRB-Ref polymorphically.
    const r = fix.store.currentValue(E, A_CHILDREN).?;
    try testing.expect(boxed.isRrbRef(r));
    const rrb = boxed.untagRrbRef(r);
    try testing.expectEqual(@as(usize, 3), rrb.count());
}

test "cardinality :many: retract removes only the named value" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const fix = try setup(&arena);

    _ = try fix.store.assert(A_CHILDREN, kernel_eids.ATTR_CARDINALITY, fix.many_ref, kernel_eids.M_BOOTSTRAP);
    _ = try fix.store.assert(E, A_CHILDREN, ref_mod.mkEid(V1), kernel_eids.M_BOOTSTRAP);
    _ = try fix.store.assert(E, A_CHILDREN, ref_mod.mkEid(V2), kernel_eids.M_BOOTSTRAP);
    _ = try fix.store.assert(E, A_CHILDREN, ref_mod.mkEid(V3), kernel_eids.M_BOOTSTRAP);

    _ = try fix.store.retract(E, A_CHILDREN, ref_mod.mkEid(V2), kernel_eids.M_BOOTSTRAP);

    try testing.expect(fix.store.hasFact(E, A_CHILDREN, ref_mod.mkEid(V1)));
    try testing.expect(!fix.store.hasFact(E, A_CHILDREN, ref_mod.mkEid(V2)));
    try testing.expect(fix.store.hasFact(E, A_CHILDREN, ref_mod.mkEid(V3)));

    const r = fix.store.currentValue(E, A_CHILDREN).?;
    try testing.expect(boxed.isRrbRef(r));
    try testing.expectEqual(@as(usize, 2), boxed.untagRrbRef(r).count());
}

test "cardinality :many: empty after all-retracted returns empty RRB-Ref (not NIL)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const fix = try setup(&arena);

    _ = try fix.store.assert(A_CHILDREN, kernel_eids.ATTR_CARDINALITY, fix.many_ref, kernel_eids.M_BOOTSTRAP);
    _ = try fix.store.assert(E, A_CHILDREN, ref_mod.mkEid(V1), kernel_eids.M_BOOTSTRAP);
    _ = try fix.store.retract(E, A_CHILDREN, ref_mod.mkEid(V1), kernel_eids.M_BOOTSTRAP);

    // Per ADR-9.5-D2: distinguishable from never-asserted (which is NIL).
    const r = fix.store.currentValue(E, A_CHILDREN).?;
    try testing.expect(boxed.isRrbRef(r));
    try testing.expectEqual(@as(usize, 0), boxed.untagRrbRef(r).count());
}

test "cardinality :many: never-asserted attribute on entity returns NIL" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const fix = try setup(&arena);

    _ = try fix.store.assert(A_CHILDREN, kernel_eids.ATTR_CARDINALITY, fix.many_ref, kernel_eids.M_BOOTSTRAP);
    // E has never been touched on A_CHILDREN.
    try testing.expectEqual(@as(?Ref, null), fix.store.currentValue(E, A_CHILDREN));
}

// -- currentValuesIter ------------------------------------------------------

test "currentValuesIter: yields every current value for :many attr" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const fix = try setup(&arena);

    _ = try fix.store.assert(A_CHILDREN, kernel_eids.ATTR_CARDINALITY, fix.many_ref, kernel_eids.M_BOOTSTRAP);
    _ = try fix.store.assert(E, A_CHILDREN, ref_mod.mkEid(V1), kernel_eids.M_BOOTSTRAP);
    _ = try fix.store.assert(E, A_CHILDREN, ref_mod.mkEid(V2), kernel_eids.M_BOOTSTRAP);

    var it = fix.store.currentValuesIter(E, A_CHILDREN);
    var seen_v1 = false;
    var seen_v2 = false;
    var count: usize = 0;
    while (it.next()) |v| {
        count += 1;
        if (v == ref_mod.mkEid(V1)) seen_v1 = true;
        if (v == ref_mod.mkEid(V2)) seen_v2 = true;
    }
    try testing.expectEqual(@as(usize, 2), count);
    try testing.expect(seen_v1);
    try testing.expect(seen_v2);
}

test "currentValuesIter: works on :one attr too (0 or 1 results)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const fix = try setup(&arena);

    _ = try fix.store.assert(E, A_NAME, ref_mod.mkEid(V1), kernel_eids.M_BOOTSTRAP);
    var it = fix.store.currentValuesIter(E, A_NAME);
    const first = it.next().?;
    try testing.expectEqual(ref_mod.mkEid(V1), first);
    try testing.expectEqual(@as(?Ref, null), it.next());
}

// -- Forward-only cardinality flip (D3) ------------------------------------

test "cardinality flip mid-life: pre-flip :one history survives, post-flip uses :many semantics" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const fix = try setup(&arena);

    // Phase A: A_CHILDREN is :one. Assert v1 then v2 → v1 implicit-retracted, v2 current.
    _ = try fix.store.assert(E, A_CHILDREN, ref_mod.mkEid(V1), kernel_eids.M_BOOTSTRAP);
    _ = try fix.store.assert(E, A_CHILDREN, ref_mod.mkEid(V2), kernel_eids.M_BOOTSTRAP);
    try testing.expect(!fix.store.hasFact(E, A_CHILDREN, ref_mod.mkEid(V1)));
    try testing.expect(fix.store.hasFact(E, A_CHILDREN, ref_mod.mkEid(V2)));

    // Phase B: flip to :many. Past history unchanged. v2 still current; assert v3 appends.
    _ = try fix.store.assert(A_CHILDREN, kernel_eids.ATTR_CARDINALITY, fix.many_ref, kernel_eids.M_BOOTSTRAP);
    _ = try fix.store.assert(E, A_CHILDREN, ref_mod.mkEid(V3), kernel_eids.M_BOOTSTRAP);

    // v2 (pre-flip) and v3 (post-flip) both currently present. v1 stays retracted.
    try testing.expect(!fix.store.hasFact(E, A_CHILDREN, ref_mod.mkEid(V1)));
    try testing.expect(fix.store.hasFact(E, A_CHILDREN, ref_mod.mkEid(V2)));
    try testing.expect(fix.store.hasFact(E, A_CHILDREN, ref_mod.mkEid(V3)));

    const r = fix.store.currentValue(E, A_CHILDREN).?;
    try testing.expectEqual(@as(usize, 2), boxed.untagRrbRef(r).count());
}

// -- Cardinality attr itself stays :one (recursion base case) --------------

test "cardinality recursion base: ATTR_CARDINALITY itself is always :one" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const fix = try setup(&arena);

    try testing.expectEqual(fact_store.Cardinality.one, fix.store.cardinalityOfAttr(kernel_eids.ATTR_CARDINALITY));
}
