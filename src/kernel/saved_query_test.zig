const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const siphon = @import("siphon.zig");
const query = @import("query.zig");
const saved_query_mod = @import("saved_query.zig");
const kernel_eids = @import("kernel_eids.zig");

const Ref = ref_mod.Ref;

const A_NAME: u64 = 200;
const E1: u64 = 100;
const E2: u64 = 101;
const M: u64 = 1;

fn setup(arena: *std.heap.ArenaAllocator) !struct {
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
} {
    const a = arena.allocator();
    const store = try a.create(fact_store.FactStore);
    store.* = fact_store.FactStore.init(a);
    const table = try a.create(object_table.ObjectTable);
    table.* = object_table.ObjectTable.init(a);
    const interner = try a.create(interner_mod.Interner);
    interner.* = interner_mod.Interner.init(a);
    _ = try siphon.boot(a, store, table, interner);
    return .{ .store = store, .table = table, .interner = interner };
}

test "save then run returns the same result" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try setup(&arena);

    _ = try s.store.assert(E1, A_NAME, ref_mod.mkSmallInt(7).?, M);
    _ = try s.store.assert(E2, A_NAME, ref_mod.mkSmallInt(8).?, M);

    var q = query.Query.init(a);
    const ve = try q.variable("e");
    try q.addFind(ve);
    try q.addTriple(.{ .variable = ve }, .{ .constant = ref_mod.mkEid(A_NAME) }, .{ .wildcard = {} });

    const name_eid = try s.interner.intern(s.store, s.table, "all-named");
    _ = try saved_query_mod.saveQuery(s.store, s.table, name_eid, &q);

    const q2 = saved_query_mod.lookupQuery(s.store, name_eid).?;
    var r = try query.execute(q2, s.store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 2), r.count());
}

test "shadow semantics: re-save with same name updates the spec" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try setup(&arena);

    _ = try s.store.assert(E1, A_NAME, ref_mod.mkSmallInt(7).?, M);
    _ = try s.store.assert(E2, A_NAME, ref_mod.mkSmallInt(8).?, M);

    var q1 = query.Query.init(a);
    const v1 = try q1.variable("e");
    try q1.addFind(v1);
    try q1.addTriple(.{ .variable = v1 }, .{ .constant = ref_mod.mkEid(A_NAME) }, .{ .constant = ref_mod.mkSmallInt(7).? });

    var q2 = query.Query.init(a);
    const v2 = try q2.variable("e");
    try q2.addFind(v2);
    try q2.addTriple(.{ .variable = v2 }, .{ .constant = ref_mod.mkEid(A_NAME) }, .{ .wildcard = {} });

    const name_eid = try s.interner.intern(s.store, s.table, "my-query");
    const e1 = try saved_query_mod.saveQuery(s.store, s.table, name_eid, &q1);
    const e2 = try saved_query_mod.saveQuery(s.store, s.table, name_eid, &q2);
    try testing.expectEqual(e1, e2); // same saved-query entity
    const looked = saved_query_mod.lookupQuery(s.store, name_eid).?;
    try testing.expect(looked == &q2);
}

test "lookup missing name returns null" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try setup(&arena);
    const name_eid = try s.interner.intern(s.store, s.table, "nonexistent");
    try testing.expectEqual(@as(?*query.Query, null), saved_query_mod.lookupQuery(s.store, name_eid));
}
