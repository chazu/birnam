const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const id_api_mod = @import("id_api.zig");
const module = @import("module.zig");
const kernel_eids = @import("kernel_eids.zig");
const siphon = @import("siphon.zig");
const send_mod = @import("send.zig");

const Ref = ref_mod.Ref;

fn shellLog(_: [*:0]const u8) callconv(.c) void {}
fn shellSetReplHandler(_: ?*const fn ([*:0]const u8) callconv(.c) void) callconv(.c) void {}

var shell_ctx = id_api_mod.ShellContext{
    .log = &shellLog,
    .set_repl_handler = &shellSetReplHandler,
};

fn fullSetup(arena: *std.heap.ArenaAllocator) !struct {
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
    manager: *module.Manager,
    api: *const id_api_mod.IdApi,
} {
    const allocator = arena.allocator();
    const store = try allocator.create(fact_store.FactStore);
    store.* = fact_store.FactStore.init(allocator);
    const table = try allocator.create(object_table.ObjectTable);
    table.* = object_table.ObjectTable.init(allocator);
    const interner = try allocator.create(interner_mod.Interner);
    interner.* = interner_mod.Interner.init(allocator);
    _ = try siphon.boot(allocator, store, table, interner);
    id_api_mod.setInterner(interner);

    const api_struct = try allocator.create(id_api_mod.IdApi);
    api_struct.* = id_api_mod.populate();

    const manager = try allocator.create(module.Manager);
    manager.* = module.Manager.init(allocator, &shell_ctx, api_struct.*);
    return .{ .store = store, .table = table, .interner = interner, .manager = manager, .api = api_struct };
}

fn loadQuery(s: anytype) !u64 {
    return (try s.manager.load("./zig-out/lib/libbirnam_query.dylib")) orelse error.LoadFailed;
}

// -- Tests ------------------------------------------------------------------

test "load query dylib: db is exported" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try fullSetup(&arena);
    const mod_eid = try loadQuery(s);

    const sel_db = try s.interner.intern(s.store, s.table, "db");
    const db_ref = s.store.currentValue(mod_eid, sel_db).?;
    try testing.expect(ref_mod.isEid(db_ref));
}

test "query: build via IdApi, execute via [db execute: q], iterate via result methods" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try fullSetup(&arena);
    const mod_eid = try loadQuery(s);

    // Seed some facts.
    const M_TEST: u64 = 999;
    const A_NAME: u64 = 9001;
    const A_AGE: u64 = 9002;
    const E1: u64 = 9100;
    const E2: u64 = 9101;
    const E3: u64 = 9102;
    _ = try s.store.assert(E1, A_NAME, ref_mod.mkSmallInt(1).?, M_TEST);
    _ = try s.store.assert(E1, A_AGE, ref_mod.mkSmallInt(25).?, M_TEST);
    _ = try s.store.assert(E2, A_NAME, ref_mod.mkSmallInt(2).?, M_TEST);
    _ = try s.store.assert(E2, A_AGE, ref_mod.mkSmallInt(35).?, M_TEST);
    _ = try s.store.assert(E3, A_NAME, ref_mod.mkSmallInt(3).?, M_TEST);
    _ = try s.store.assert(E3, A_AGE, ref_mod.mkSmallInt(30).?, M_TEST);

    // Build query via IdApi: find ?e where [?e :age ?a][?a < 30].
    const api = s.api;
    const q = api.make_query().?;
    defer api.free_query(q);
    const ve = api.query_variable(q, "e", 1);
    const va = api.query_variable(q, "a", 1);
    api.query_add_find(q, ve);
    // [?e A_AGE ?a]
    api.query_add_triple(q,
        1, ve, // var
        0, ref_mod.mkEid(A_AGE), // const
        1, va, // var
    );
    // [?a < 30]
    api.query_add_predicate(q,
        2, // lt
        1, va,
        0, ref_mod.mkSmallInt(30).?,
    );

    // Dispatch through send.
    const sel_db = try s.interner.intern(s.store, s.table, "db");
    const db_ref = s.store.currentValue(mod_eid, sel_db).?;
    const sel_execute = try s.interner.intern(s.store, s.table, "execute:");
    const q_ref: Ref = @intFromPtr(q);
    const result_ref = send_mod.send1(db_ref, sel_execute, q_ref);
    try testing.expect(ref_mod.isPointer(result_ref));

    // [result count] -> 1
    const sel_count = try s.interner.intern(s.store, s.table, "count");
    const count_ref = send_mod.send0(result_ref, sel_count);
    try testing.expect(ref_mod.isSmallInt(count_ref));
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(count_ref));

    // [result at: 0] -> mkEid(E1)
    const sel_at = try s.interner.intern(s.store, s.table, "at:");
    const at_ref = send_mod.send1(result_ref, sel_at, ref_mod.mkSmallInt(0).?);
    try testing.expectEqual(ref_mod.mkEid(E1), at_ref);
}

test "query: aggregate count via send-dispatched [db execute: q] -> [result at:0]" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try fullSetup(&arena);
    const mod_eid = try loadQuery(s);

    const M_TEST: u64 = 999;
    const A_AGE: u64 = 9020;
    _ = try s.store.assert(9300, A_AGE, ref_mod.mkSmallInt(10).?, M_TEST);
    _ = try s.store.assert(9301, A_AGE, ref_mod.mkSmallInt(20).?, M_TEST);
    _ = try s.store.assert(9302, A_AGE, ref_mod.mkSmallInt(30).?, M_TEST);

    const api = s.api;
    const q = api.make_query().?;
    defer api.free_query(q);
    const ve = api.query_variable(q, "e", 1);
    const va = api.query_variable(q, "a", 1);
    api.query_add_find_aggregate(q, 0, va); // 0 = count
    api.query_add_triple(q,
        1, ve,
        0, ref_mod.mkEid(A_AGE),
        1, va,
    );

    const sel_db = try s.interner.intern(s.store, s.table, "db");
    const db_ref = s.store.currentValue(mod_eid, sel_db).?;
    const sel_execute = try s.interner.intern(s.store, s.table, "execute:");
    const sel_at = try s.interner.intern(s.store, s.table, "at:");

    const result_ref = send_mod.send1(db_ref, sel_execute, @as(Ref, @intFromPtr(q)));
    const cell_ref = send_mod.send1(result_ref, sel_at, ref_mod.mkSmallInt(0).?);
    try testing.expect(ref_mod.isSmallInt(cell_ref));
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(cell_ref));
}

test "query: collect aggregate -> [result at:0] returns RRB; [rrb count]/[rrb at:i]" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try fullSetup(&arena);
    const mod_eid = try loadQuery(s);

    const M_TEST: u64 = 999;
    const A_TAG: u64 = 9030;
    _ = try s.store.assert(9400, A_TAG, ref_mod.mkSmallInt(100).?, M_TEST);
    _ = try s.store.assert(9401, A_TAG, ref_mod.mkSmallInt(200).?, M_TEST);
    _ = try s.store.assert(9402, A_TAG, ref_mod.mkSmallInt(300).?, M_TEST);

    const api = s.api;
    const q = api.make_query().?;
    defer api.free_query(q);
    const ve = api.query_variable(q, "e", 1);
    const va = api.query_variable(q, "a", 1);
    api.query_add_find_aggregate(q, 5, va); // 5 = collect
    api.query_add_triple(q,
        1, ve,
        0, ref_mod.mkEid(A_TAG),
        1, va,
    );

    const sel_db = try s.interner.intern(s.store, s.table, "db");
    const db_ref = s.store.currentValue(mod_eid, sel_db).?;
    const sel_execute = try s.interner.intern(s.store, s.table, "execute:");
    const sel_at = try s.interner.intern(s.store, s.table, "at:");
    const sel_count = try s.interner.intern(s.store, s.table, "count");

    const result_ref = send_mod.send1(db_ref, sel_execute, @as(Ref, @intFromPtr(q)));
    const rrb_ref = send_mod.send1(result_ref, sel_at, ref_mod.mkSmallInt(0).?);
    // RRB wrapper is a pointer-tagged Ref.
    try testing.expect(ref_mod.isPointer(rrb_ref));

    // [rrb count] -> 3
    const rrb_count_ref = send_mod.send0(rrb_ref, sel_count);
    try testing.expect(ref_mod.isSmallInt(rrb_count_ref));
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(rrb_count_ref));

    // [rrb at: 0/1/2] returns the collected SmallInts in some order; sum must be 600.
    var sum: i64 = 0;
    var i: i64 = 0;
    while (i < 3) : (i += 1) {
        const v = send_mod.send1(rrb_ref, sel_at, ref_mod.mkSmallInt(i).?);
        try testing.expect(ref_mod.isSmallInt(v));
        sum += ref_mod.untagSmallInt(v);
    }
    try testing.expectEqual(@as(i64, 600), sum);
}

test "query: multi-result findAll, full iteration via send-dispatched methods" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try fullSetup(&arena);
    const mod_eid = try loadQuery(s);

    const M_TEST: u64 = 999;
    const A_KIND: u64 = 9010;
    const K_LEAF: Ref = ref_mod.mkSmallInt(1).?;
    const E1: u64 = 9200;
    const E2: u64 = 9201;
    const E3: u64 = 9202;
    _ = try s.store.assert(E1, A_KIND, K_LEAF, M_TEST);
    _ = try s.store.assert(E2, A_KIND, K_LEAF, M_TEST);
    _ = try s.store.assert(E3, A_KIND, K_LEAF, M_TEST);

    const api = s.api;
    const q = api.make_query().?;
    defer api.free_query(q);
    const ve = api.query_variable(q, "e", 1);
    api.query_add_find(q, ve);
    api.query_add_triple(q, 1, ve, 0, ref_mod.mkEid(A_KIND), 0, K_LEAF);

    const sel_db = try s.interner.intern(s.store, s.table, "db");
    const db_ref = s.store.currentValue(mod_eid, sel_db).?;
    const sel_execute = try s.interner.intern(s.store, s.table, "execute:");
    const sel_count = try s.interner.intern(s.store, s.table, "count");
    const sel_at = try s.interner.intern(s.store, s.table, "at:");

    const result_ref = send_mod.send1(db_ref, sel_execute, @as(Ref, @intFromPtr(q)));
    const count_ref = send_mod.send0(result_ref, sel_count);
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(count_ref));

    // Walk via send-dispatched at:.
    var seen_e1 = false;
    var seen_e2 = false;
    var seen_e3 = false;
    var i: i64 = 0;
    while (i < 3) : (i += 1) {
        const row = send_mod.send1(result_ref, sel_at, ref_mod.mkSmallInt(i).?);
        if (row == ref_mod.mkEid(E1)) seen_e1 = true;
        if (row == ref_mod.mkEid(E2)) seen_e2 = true;
        if (row == ref_mod.mkEid(E3)) seen_e3 = true;
    }
    try testing.expect(seen_e1 and seen_e2 and seen_e3);
}
