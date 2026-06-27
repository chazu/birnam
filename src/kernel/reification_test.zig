const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const siphon = @import("siphon.zig");
const query = @import("query.zig");
const kernel_eids = @import("kernel_eids.zig");

const Ref = ref_mod.Ref;
const Term = query.Term;

const A_NAME: u64 = 1200;
const E1: u64 = 1100;
const E2: u64 = 1101;

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

// -- Basic :as binding -------------------------------------------------------

test ":as binds a stable fact-EID per matched triple" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try setup(&arena);

    const M_FOO: u64 = 1000;
    _ = try s.store.assert(E1, A_NAME, ref_mod.mkSmallInt(7).?, M_FOO);

    var q = query.Query.init(a);
    defer q.deinit();
    const vf = try q.variable("fact");
    try q.addFind(vf);
    try q.addTripleAs(
        .{ .constant = ref_mod.mkEid(E1) },
        .{ .constant = ref_mod.mkEid(A_NAME) },
        .{ .constant = ref_mod.mkSmallInt(7).? },
        vf,
    );

    var r = try query.execute(&q, s.store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 1), r.count());
    const fact_ref = r.at(0, 0);
    try testing.expect(ref_mod.isEid(fact_ref));
    const fact_eid = ref_mod.untagEid(fact_ref);
    // Should be in user-range (>= USER_EID_START).
    try testing.expect(fact_eid >= kernel_eids.USER_EID_START);
}

test "two queries asking for same fact's EID return the same value" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try setup(&arena);

    const M_FOO: u64 = 1000;
    _ = try s.store.assert(E1, A_NAME, ref_mod.mkSmallInt(7).?, M_FOO);

    var q1 = query.Query.init(a);
    defer q1.deinit();
    const f1 = try q1.variable("fact");
    try q1.addFind(f1);
    try q1.addTripleAs(
        .{ .constant = ref_mod.mkEid(E1) },
        .{ .constant = ref_mod.mkEid(A_NAME) },
        .{ .constant = ref_mod.mkSmallInt(7).? },
        f1,
    );
    var r1 = try query.execute(&q1, s.store);
    defer r1.deinit();

    var q2 = query.Query.init(a);
    defer q2.deinit();
    const f2 = try q2.variable("fact");
    try q2.addFind(f2);
    try q2.addTripleAs(
        .{ .constant = ref_mod.mkEid(E1) },
        .{ .constant = ref_mod.mkEid(A_NAME) },
        .{ .constant = ref_mod.mkSmallInt(7).? },
        f2,
    );
    var r2 = try query.execute(&q2, s.store);
    defer r2.deinit();

    try testing.expectEqual(r1.at(0, 0), r2.at(0, 0));
}

// -- Reverse :module query ---------------------------------------------------

test "[?fact :module ?m] resolves to the asserting module" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try setup(&arena);

    const M_FOO: u64 = 1000;
    _ = try s.store.assert(E1, A_NAME, ref_mod.mkSmallInt(7).?, M_FOO);

    // First query: bind ?fact via :as.
    var q = query.Query.init(a);
    defer q.deinit();
    const vf = try q.variable("fact");
    const vm = try q.variable("m");
    try q.addFind(vm);
    try q.addTripleAs(
        .{ .constant = ref_mod.mkEid(E1) },
        .{ .constant = ref_mod.mkEid(A_NAME) },
        .{ .constant = ref_mod.mkSmallInt(7).? },
        vf,
    );
    // Now query [?fact :module ?m].
    try q.addTriple(.{ .variable = vf }, .{ .constant = ref_mod.mkEid(kernel_eids.ATTR_MODULE) }, .{ .variable = vm });

    var r = try query.execute(&q, s.store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 1), r.count());
    try testing.expectEqual(ref_mod.mkEid(M_FOO), r.at(0, 0));
}

test ":as binds during attributesOf walk (Case B)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const s = try setup(&arena);

    const M_FOO: u64 = 1000;
    _ = try s.store.assert(E1, A_NAME, ref_mod.mkSmallInt(5).?, M_FOO);

    var q = query.Query.init(a);
    defer q.deinit();
    const va = try q.variable("a");
    const vv = try q.variable("v");
    const vf = try q.variable("fact");
    const vm = try q.variable("m");
    try q.addFind(vm);
    // [E1 ?a ?v :as ?fact] — Case B (e bound, a/v unbound).
    try q.addTripleAs(
        .{ .constant = ref_mod.mkEid(E1) },
        .{ .variable = va },
        .{ .variable = vv },
        vf,
    );
    try q.addTriple(.{ .variable = vf }, .{ .constant = ref_mod.mkEid(kernel_eids.ATTR_MODULE) }, .{ .variable = vm });

    var r = try query.execute(&q, s.store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 1), r.count());
    try testing.expectEqual(ref_mod.mkEid(M_FOO), r.at(0, 0));
}