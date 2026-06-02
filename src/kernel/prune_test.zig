//! Phase 13 — reachability prune tests.

const std = @import("std");
const testing = std.testing;
const prune = @import("prune.zig");
const fs = @import("fact_store.zig");
const ot = @import("object_table.zig");
const ref = @import("ref.zig");
const ke = @import("kernel_eids.zig");
const strings = @import("strings.zig");

fn vi(n: i64) ref.Ref {
    return ref.mkSmallInt(n).?;
}

const M_USER = ke.M_USER;
const M_CODE = ke.M_LANG;

// Minimal "bytecode method" entity: just needs a :bytecode-stream fact to be
// recognized as a method root/target.
fn fakeMethod(store: *fs.FactStore, a: std.mem.Allocator, eid: u64) !void {
    _ = try store.assert(eid, ke.ATTR_BYTECODE_STREAM, try strings.mkString(a, "bc"), M_USER);
}

test "prune: orphan AST swept, installed method + its AST kept" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);

    // Live method (eid 500) installed on a kernel vt; carries an AST (510).
    try fakeMethod(&store, a, 500);
    _ = try store.assert(500, ke.ATTR_METHOD_BODY_AST, ref.mkEid(510), M_USER);
    _ = try store.assert(510, ke.ATTR_AST_VALUE, vi(42), M_USER);
    _ = try store.assert(ke.SMALLINT_VT, 9999, ref.mkEid(500), M_USER); // install [vt sel method]

    // Orphan AST (eid 600): parsed, referenced by nobody.
    _ = try store.assert(600, ke.ATTR_AST_VALUE, vi(7), M_USER);

    const stats = try prune.pruneReachable(&store, &table, a);

    try testing.expect(stats.swept_entities >= 1);
    // Live method, its AST, and the install fact survive.
    try testing.expect(store.currentValue(500, ke.ATTR_BYTECODE_STREAM) != null);
    try testing.expectEqual(vi(42), store.currentValue(510, ke.ATTR_AST_VALUE).?);
    try testing.expectEqual(ref.mkEid(500), store.currentValue(ke.SMALLINT_VT, 9999).?);
    // Orphan AST is gone.
    try testing.expect(store.currentValue(600, ke.ATTR_AST_VALUE) == null);
}

test "prune: redefined method drops the old method + clears its object slot" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);

    try fakeMethod(&store, a, 500); // old method
    try fakeMethod(&store, a, 501); // new method
    try table.put(500, ref.mkEid(1)); // pretend old method materialized
    try table.put(501, ref.mkEid(1));

    // Install old, then redefine to new (cardinality-one attr replaces value).
    const sel: u64 = 9999;
    _ = try store.assert(ke.SMALLINT_VT, sel, ref.mkEid(500), M_USER);
    _ = try store.assert(ke.SMALLINT_VT, sel, ref.mkEid(501), M_USER);
    try testing.expectEqual(ref.mkEid(501), store.currentValue(ke.SMALLINT_VT, sel).?);

    _ = try prune.pruneReachable(&store, &table, a);

    // New method kept + still materialized; old method swept + slot cleared.
    try testing.expect(store.currentValue(501, ke.ATTR_BYTECODE_STREAM) != null);
    try testing.expect(table.get(501) != null);
    try testing.expect(store.currentValue(500, ke.ATTR_BYTECODE_STREAM) == null);
    try testing.expect(table.get(500) == null);
}

test "prune: REPL binding chain + its bound method kept" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);

    // TOP_SCOPE -> binding 700 -> bind-value = method 500.
    try fakeMethod(&store, a, 500);
    _ = try store.assert(ke.TOP_SCOPE_EID, ke.ATTR_SCOPE_FIRST_BINDING, ref.mkEid(700), M_USER);
    _ = try store.assert(700, ke.ATTR_BIND_VALUE, ref.mkEid(500), M_USER);
    _ = try store.assert(700, ke.ATTR_BIND_NAME, ref.mkEid(50), M_USER);
    // A dead entity referencing nothing reachable.
    _ = try store.assert(800, ke.ATTR_AST_VALUE, vi(1), M_USER);

    const stats = try prune.pruneReachable(&store, &table, a);

    try testing.expectEqual(ref.mkEid(700), store.currentValue(ke.TOP_SCOPE_EID, ke.ATTR_SCOPE_FIRST_BINDING).?);
    try testing.expectEqual(ref.mkEid(500), store.currentValue(700, ke.ATTR_BIND_VALUE).?);
    try testing.expect(store.currentValue(500, ke.ATTR_BYTECODE_STREAM) != null);
    try testing.expect(store.currentValue(800, ke.ATTR_AST_VALUE) == null);
    try testing.expect(stats.swept_entities >= 1);
}

var g_demote_table: ?*ot.ObjectTable = null;
fn demoteHook(e: u64, a: u64) callconv(.c) void {
    _ = a;
    if (g_demote_table) |t| t.clear(e) catch {};
}

test "prune: kept entity's materialized slot survives compact (demote suppressed)" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);
    // Wire a promotion-style demote-on-mutate hook.
    store.on_entity_mutate = &demoteHook;
    g_demote_table = &table;
    defer g_demote_table = null;

    try fakeMethod(&store, a, 500);
    _ = try store.assert(ke.SMALLINT_VT, 9999, ref.mkEid(500), M_USER); // install (live)
    try table.put(500, ref.mkEid(1)); // materialized closure slot

    // Sanity: a normal mutation demotes (clears the slot).
    _ = try store.assert(500, 9998, vi(0), M_USER);
    try testing.expect(table.get(500) == null);
    try table.put(500, ref.mkEid(1)); // re-materialize

    // Prune re-asserts every kept fact via compact — must NOT demote the
    // kept method, or it'd be undispatchable until re-materialized.
    _ = try prune.pruneReachable(&store, &table, a);
    try testing.expect(table.get(500) != null); // slot survived
}

test "prune: CODE facts are never swept" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);

    // A CODE entity unreachable from any M_USER root — must survive.
    _ = try store.assert(900, ke.ATTR_AST_VALUE, vi(123), M_CODE);
    // An unreachable M_USER entity — must be swept.
    _ = try store.assert(901, ke.ATTR_AST_VALUE, vi(456), M_USER);

    _ = try prune.pruneReachable(&store, &table, a);

    try testing.expectEqual(vi(123), store.currentValue(900, ke.ATTR_AST_VALUE).?); // CODE kept
    try testing.expect(store.currentValue(901, ke.ATTR_AST_VALUE) == null); // M_USER swept
}
