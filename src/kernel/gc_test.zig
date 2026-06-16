//! Phase 15.2/15.3/15.5 — heap collector tests (boxes + closures).

const std = @import("std");
const testing = std.testing;
const gc = @import("gc.zig");
const boxed = @import("boxed.zig");
const arenas = @import("arenas.zig");
const vm = @import("vm.zig");
const fs = @import("fact_store.zig");
const ot = @import("object_table.zig");
const ke = @import("kernel_eids.zig");
const ref = @import("ref.zig");
const ffi = @import("ffi.zig");

const FAIL = testing.failing_allocator; // boxes/closures route to arenas; param unused
const REF_SZ = @sizeOf(ref.Ref);

fn freshArenas() !void {
    if (arenas.ready()) arenas.deinit();
    try arenas.init(testing.allocator);
}

/// Build a CLOSURE_VT wrapper `[vt][method][data]` in the closures_vt arena,
/// mirroring `cMakeClosure`/`allocClosureEntity`. Returns the wrapper Ref
/// (points at the payload = base+8).
fn makeWrapper(data: ref.Ref) !ref.Ref {
    const a = arenas.closuresVt();
    const buf = try a.alignedAlloc(u8, .of(ref.Ref), 3 * REF_SZ);
    @memset(buf, 0);
    const vt_slot: *ref.Ref = @ptrCast(@alignCast(buf.ptr));
    vt_slot.* = ref.mkEid(ke.CLOSURE_VT);
    const payload = buf.ptr + REF_SZ;
    const method_slot: *usize = @ptrCast(@alignCast(payload));
    method_slot.* = 0xdead; // dummy fn ptr — never called in the mark
    const data_slot: *ref.Ref = @ptrCast(@alignCast(payload + REF_SZ));
    data_slot.* = data;
    return ref.mkPointer(@ptrCast(payload));
}

test "gc: frees orphan boxes, keeps fact-referenced ones" {
    try freshArenas();
    defer arenas.deinit();
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);

    const kept = try boxed.mkFloat(FAIL, 1.5);
    const orphan = try boxed.mkFloat(FAIL, 9.0);
    try testing.expectEqual(@as(usize, 2), arenas.arena(.boxes).?.liveCount());

    _ = try store.assert(100, ke.ATTR_AST_VALUE, kept, ke.M_USER);

    const st = gc.collectBoxes(&store, &table, a);
    try testing.expect(!st.skipped);
    try testing.expectEqual(@as(usize, 1), st.boxes_freed);
    try testing.expectEqual(@as(usize, 1), st.boxes_kept);
    try testing.expectEqual(@as(f64, 1.5), boxed.untagFloat(kept));
    _ = orphan;
}

test "gc: keeps a box reachable only through a constant-pool RRB" {
    try freshArenas();
    defer arenas.deinit();
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);

    const in_pool = try boxed.mkFloat(FAIL, 2.5);
    const loose = try boxed.mkFloat(FAIL, 7.0);
    const pool = try boxed.mkRrb(a, &.{in_pool}); // RRB nodes on the test arena
    _ = try store.assert(200, ke.ATTR_CONSTANT_POOL, pool, ke.M_USER);

    const st = gc.collectBoxes(&store, &table, a);
    try testing.expectEqual(@as(usize, 1), st.boxes_freed); // loose
    try testing.expectEqual(@as(f64, 2.5), boxed.untagFloat(in_pool));
    _ = loose;
}

test "gc: sweeps orphan closures, keeps table-referenced; live captures pin boxes" {
    try freshArenas();
    defer arenas.deinit();
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);

    // Live closure: referenced from a table slot, captures a float box.
    const cap_box = try boxed.mkFloat(FAIL, 4.5);
    const live = try vm.allocBytecodeBlock(FAIL, 500, ref.NIL, 0, 1);
    live.captures_tail[0] = cap_box;
    const live_eid = table.alloc();
    try table.put(live_eid, live.ref);

    // Orphan closure: referenced by nothing, captures its own box.
    const dead_box = try boxed.mkFloat(FAIL, 8.0);
    const dead = try vm.allocBytecodeBlock(FAIL, 501, ref.NIL, 0, 1);
    dead.captures_tail[0] = dead_box;

    try testing.expectEqual(@as(usize, 2), arenas.arena(.boxes).?.liveCount());
    try testing.expectEqual(@as(usize, 2), arenas.arena(.captures).?.liveCount());

    const st = gc.collectBoxes(&store, &table, a);
    // Dead closure + its box reclaimed; live closure + its captured box kept.
    try testing.expectEqual(@as(usize, 1), st.closures_freed);
    try testing.expectEqual(@as(usize, 1), st.closures_kept);
    try testing.expectEqual(@as(usize, 1), st.boxes_freed); // dead_box
    try testing.expectEqual(@as(usize, 1), st.boxes_kept); // cap_box via live closure
    try testing.expectEqual(@as(f64, 4.5), boxed.untagFloat(cap_box)); // still valid
}

test "gc: live wrapper pins its inner block; orphan wrapper + inner freed" {
    try freshArenas();
    defer arenas.deinit();
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);

    // Live: CLOSURE_VT wrapper in the table, data = inner BytecodeClosure.
    // The inner block is reachable ONLY through the wrapper's `data`.
    const live_inner = try vm.allocBytecodeBlock(FAIL, 600, ref.NIL, 0, 0);
    const live_wrap = try makeWrapper(live_inner.ref);
    const wrap_eid = table.alloc();
    try table.put(wrap_eid, live_wrap);

    // Orphan: wrapper referenced by nothing, wrapping its own block.
    const dead_inner = try vm.allocBytecodeBlock(FAIL, 601, ref.NIL, 0, 0);
    _ = try makeWrapper(dead_inner.ref);

    try testing.expectEqual(@as(usize, 2), arenas.arena(.captures).?.liveCount());
    try testing.expectEqual(@as(usize, 2), arenas.arena(.closures_vt).?.liveCount());

    const st = gc.collectBoxes(&store, &table, a);
    try testing.expect(!st.skipped);
    // Live wrapper + its inner block kept; orphan wrapper + its inner freed.
    try testing.expectEqual(@as(usize, 1), st.wrappers_freed);
    try testing.expectEqual(@as(usize, 1), st.wrappers_kept);
    try testing.expectEqual(@as(usize, 1), st.closures_freed); // dead_inner
    try testing.expectEqual(@as(usize, 1), st.closures_kept); // live_inner via wrapper
}

test "gc: frees orphan string boxes, keeps fact-referenced ones" {
    try freshArenas();
    defer arenas.deinit();
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);

    const strs = arenas.strings();
    const kept = try @import("strings.zig").mkString(strs, "live");
    const orphan = try @import("strings.zig").mkString(strs, "dead");
    try testing.expectEqual(@as(usize, 2), arenas.arena(.strings).?.liveCount());

    // Only `kept` is referenced by a current fact (mirrors interner :name).
    _ = try store.assert(300, ke.ATTR_NAME, kept, ke.M_BOOTSTRAP);

    const st = gc.collectBoxes(&store, &table, a);
    try testing.expect(!st.skipped);
    try testing.expectEqual(@as(usize, 1), st.strings_freed); // orphan
    try testing.expectEqual(@as(usize, 1), st.strings_kept); // kept
    try testing.expectEqualStrings("live", @import("strings.zig").bytes(kept));
    _ = orphan;
}

test "gc: maybeCollect stays null below the growth threshold" {
    try freshArenas();
    defer arenas.deinit();
    gc.resetAutoTrigger();
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);

    // A handful of boxes — well under GC_GROWTH_THRESHOLD — must not arm a sweep.
    var i: usize = 0;
    while (i < 8) : (i += 1) _ = try boxed.mkFloat(FAIL, @floatFromInt(i));
    try testing.expect(gc.maybeCollect(&store, &table, a) == null);
}

test "gc: live callback box roots its inner closure (Phase 19.2 follow-up)" {
    try freshArenas();
    defer arenas.deinit();
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);

    // Inner closure: reachable ONLY through the callback's CallbackCtx, a raw
    // heap pointer the GC cannot see without the callback-marking branch.
    const inner = try vm.allocBytecodeBlock(FAIL, 700, ref.NIL, 0, 0);
    // A truly-orphan closure proves the sweep still runs.
    _ = try vm.allocBytecodeBlock(FAIL, 701, ref.NIL, 0, 0);

    // Minimal CallbackCtx — the GC only reads `.closure`; other fields are inert.
    var cbctx: ffi.CallbackCtx = undefined;
    cbctx.closure = inner.ref;
    const cb = try boxed.mkCallback(FAIL, 0xc0de, @intFromPtr(&cbctx));
    const cb_eid = table.alloc();
    try table.put(cb_eid, cb);

    try testing.expectEqual(@as(usize, 1), arenas.arena(.boxes).?.liveCount());
    try testing.expectEqual(@as(usize, 2), arenas.arena(.captures).?.liveCount());

    const st = gc.collectBoxes(&store, &table, a);
    try testing.expectEqual(@as(usize, 0), st.boxes_freed); // callback box kept
    try testing.expectEqual(@as(usize, 1), st.boxes_kept);
    try testing.expectEqual(@as(usize, 1), st.closures_freed); // orphan
    try testing.expectEqual(@as(usize, 1), st.closures_kept); // inner via callback
}

test "gc: a freed callback roots nothing (ctx cleared)" {
    try freshArenas();
    defer arenas.deinit();
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);

    const inner = try vm.allocBytecodeBlock(FAIL, 800, ref.NIL, 0, 0);
    var cbctx: ffi.CallbackCtx = undefined;
    cbctx.closure = inner.ref;
    const cb = try boxed.mkCallback(FAIL, 0xc0de, @intFromPtr(&cbctx));
    boxed.clearCallbackCtx(cb); // simulate `[cb free]` — slot 1 → 0
    const cb_eid = table.alloc();
    try table.put(cb_eid, cb);

    const st = gc.collectBoxes(&store, &table, a);
    try testing.expectEqual(@as(usize, 1), st.boxes_kept); // box still slot-held
    try testing.expectEqual(@as(usize, 1), st.closures_freed); // inner no longer rooted
    try testing.expectEqual(@as(usize, 0), st.closures_kept);
}

test "gc: no-op when boxes arena registry is absent" {
    if (arenas.ready()) arenas.deinit();
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    var store = fs.FactStore.init(a);
    var table = ot.ObjectTable.init(a);
    const st = gc.collectBoxes(&store, &table, a);
    try testing.expect(st.skipped);
    try testing.expectEqual(@as(usize, 0), st.boxes_freed);
}
