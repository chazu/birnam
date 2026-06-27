const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const object = @import("object.zig");
const closure_mod = @import("closure.zig");
const send_mod = @import("send.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const siphon = @import("siphon.zig");
const kernel_eids = @import("kernel_eids.zig");

const Ref = ref_mod.Ref;
const Closure = closure_mod.Closure;

// -- Test fixture ----------------------------------------------------------

const Fix = struct {
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
    allocator: std.mem.Allocator,
};

fn setup(arena: *std.heap.ArenaAllocator) !Fix {
    const a = arena.allocator();
    const store = try a.create(fact_store.FactStore);
    store.* = fact_store.FactStore.init(a);
    const table = try a.create(object_table.ObjectTable);
    table.* = object_table.ObjectTable.init(a);
    const interner = try a.create(interner_mod.Interner);
    interner.* = interner_mod.Interner.init(a);
    _ = try siphon.boot(a, store, table, interner);
    return .{ .store = store, .table = table, .interner = interner, .allocator = a };
}

/// Build a closure object + EID, return the closure-EID Ref.
fn mkClosureEid(fix: *const Fix, method_fn: *const anyopaque) !u64 {
    const total = 3 * @sizeOf(Ref);
    const buf = try fix.allocator.alignedAlloc(u8, .of(Ref), total);
    @memset(buf, 0);
    const vt_slot: *Ref = @ptrCast(@alignCast(buf.ptr));
    vt_slot.* = ref_mod.mkEid(kernel_eids.CLOSURE_VT);
    const c: *Closure = @ptrCast(@alignCast(buf.ptr + @sizeOf(Ref)));
    c.method = method_fn;
    c.data = ref_mod.NIL;
    const eid = fix.table.alloc();
    try fix.table.put(eid, ref_mod.mkPointer(@ptrCast(c)));
    return eid;
}

// -- Per-test selectors / state --------------------------------------------

const Send = send_mod.Send;

const SENTINEL_PRIM: Ref = ref_mod.mkSmallInt(111).?;
const SENTINEL_OVERRIDE: Ref = ref_mod.mkSmallInt(222).?;

fn primitiveMethod(_: *Send) callconv(.c) Ref {
    return SENTINEL_PRIM;
}

fn overrideTargetMethod(_: *Send) callconv(.c) Ref {
    return SENTINEL_OVERRIDE;
}

/// No-op `:bind` override: always returns NIL → fallthrough to primitive.
fn noopOverride(_: *Send) callconv(.c) Ref {
    return ref_mod.NIL;
}

/// Captured shared state for intercepting override. Globals only —
/// extern fn signature has no userdata channel.
var g_intercept_closure_eid: u64 = 0;
var g_intercept_sel: u64 = 0;
var g_intercept_count: u32 = 0;

fn interceptOverride(s: *Send) callconv(.c) Ref {
    g_intercept_count += 1;
    if (s.selector == g_intercept_sel) {
        return ref_mod.mkEid(g_intercept_closure_eid);
    }
    return ref_mod.NIL;
}

/// Broken override: re-sends the same selector to a fresh receiver, looping
/// through bind until the recursion guard trips and primitive fallback runs.
var g_broken_count: u32 = 0;
var g_broken_loopback_eid: u64 = 0;
fn brokenOverride(s: *Send) callconv(.c) Ref {
    g_broken_count += 1;
    _ = send_mod.send0(ref_mod.mkEid(g_broken_loopback_eid), s.selector);
    return ref_mod.NIL;
}

// -- Helpers ----------------------------------------------------------------

/// Build a user vtable delegating to OBJECT_VT, install one method
/// `(sel → primitiveMethod)` on it, and return the vt EID.
fn makeUserVt(fix: *const Fix, sel: u64) !u64 {
    const vt = fix.table.alloc();
    // Allocate a 0-slot struct so the vt has the proper -1 header.
    const obj_ptr = try object.allocObject(fix.allocator, ref_mod.mkEid(kernel_eids.VTABLE_VT), 0);
    try fix.table.put(vt, object.toRef(obj_ptr));
    _ = try fix.store.assert(vt, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.VTABLE_VT), kernel_eids.M_BOOTSTRAP);
    _ = try fix.store.assert(vt, kernel_eids.ATTR_DELEGATE, ref_mod.mkEid(kernel_eids.OBJECT_VT), kernel_eids.M_BOOTSTRAP);

    const prim_eid = try mkClosureEid(fix, @ptrCast(&primitiveMethod));
    _ = try fix.store.assert(vt, sel, ref_mod.mkEid(prim_eid), kernel_eids.M_BOOTSTRAP);
    return vt;
}

/// Build a receiver EID whose `:vtable` points at `vt`.
fn makeReceiver(fix: *const Fix, vt: u64) !u64 {
    const e = fix.table.alloc();
    _ = try fix.store.assert(e, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(vt), kernel_eids.M_BOOTSTRAP);
    return e;
}

// -- Tests ------------------------------------------------------------------

test "baseline: dispatch hits primitive method when no override installed" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const fix = try setup(&arena);
    const sel = try fix.interner.intern(fix.store, fix.table, "ping");
    const vt = try makeUserVt(&fix, sel);
    const recv = try makeReceiver(&fix, vt);

    const r = send_mod.send0(ref_mod.mkEid(recv), sel);
    try testing.expectEqual(SENTINEL_PRIM, r);
}

test "no-op override returns NIL → fallthrough to primitive" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const fix = try setup(&arena);
    const sel = try fix.interner.intern(fix.store, fix.table, "ping");
    const vt = try makeUserVt(&fix, sel);
    const recv = try makeReceiver(&fix, vt);

    const override_eid = try mkClosureEid(&fix, @ptrCast(&noopOverride));
    _ = try fix.store.assert(vt, kernel_eids.ATTR_BIND, ref_mod.mkEid(override_eid), kernel_eids.M_BOOTSTRAP);

    const r = send_mod.send0(ref_mod.mkEid(recv), sel);
    try testing.expectEqual(SENTINEL_PRIM, r);
}

test "intercepting override returns alternate closure → dispatch hits it" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const fix = try setup(&arena);
    const sel = try fix.interner.intern(fix.store, fix.table, "ping");
    const vt = try makeUserVt(&fix, sel);
    const recv = try makeReceiver(&fix, vt);

    const target_eid = try mkClosureEid(&fix, @ptrCast(&overrideTargetMethod));
    g_intercept_closure_eid = target_eid;
    g_intercept_sel = sel;
    g_intercept_count = 0;
    const override_eid = try mkClosureEid(&fix, @ptrCast(&interceptOverride));
    _ = try fix.store.assert(vt, kernel_eids.ATTR_BIND, ref_mod.mkEid(override_eid), kernel_eids.M_BOOTSTRAP);

    const r = send_mod.send0(ref_mod.mkEid(recv), sel);
    try testing.expectEqual(SENTINEL_OVERRIDE, r);
    try testing.expect(g_intercept_count >= 1);
}

test "override walks delegation chain: install on OBJECT_VT, child vt sees it" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const fix = try setup(&arena);
    const sel = try fix.interner.intern(fix.store, fix.table, "ping");
    const vt = try makeUserVt(&fix, sel);
    const recv = try makeReceiver(&fix, vt);

    const target_eid = try mkClosureEid(&fix, @ptrCast(&overrideTargetMethod));
    g_intercept_closure_eid = target_eid;
    g_intercept_sel = sel;
    g_intercept_count = 0;
    const override_eid = try mkClosureEid(&fix, @ptrCast(&interceptOverride));
    // Install on OBJECT_VT, NOT on the user vt. Delegation chain walk
    // should find it.
    _ = try fix.store.assert(kernel_eids.OBJECT_VT, kernel_eids.ATTR_BIND, ref_mod.mkEid(override_eid), kernel_eids.M_BOOTSTRAP);

    const r = send_mod.send0(ref_mod.mkEid(recv), sel);
    try testing.expectEqual(SENTINEL_OVERRIDE, r);
}

test "retract `:bind` restores primitive dispatch" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const fix = try setup(&arena);
    const sel = try fix.interner.intern(fix.store, fix.table, "ping");
    const vt = try makeUserVt(&fix, sel);
    const recv = try makeReceiver(&fix, vt);

    const target_eid = try mkClosureEid(&fix, @ptrCast(&overrideTargetMethod));
    g_intercept_closure_eid = target_eid;
    g_intercept_sel = sel;
    const override_eid = try mkClosureEid(&fix, @ptrCast(&interceptOverride));
    const override_ref = ref_mod.mkEid(override_eid);
    _ = try fix.store.assert(vt, kernel_eids.ATTR_BIND, override_ref, kernel_eids.M_BOOTSTRAP);

    try testing.expectEqual(SENTINEL_OVERRIDE, send_mod.send0(ref_mod.mkEid(recv), sel));

    _ = try fix.store.retract(vt, kernel_eids.ATTR_BIND, override_ref, kernel_eids.M_BOOTSTRAP);
    try testing.expectEqual(SENTINEL_PRIM, send_mod.send0(ref_mod.mkEid(recv), sel));
}

test "recursion guard: override that loops back through bind hits primitive after depth limit" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const fix = try setup(&arena);
    const sel = try fix.interner.intern(fix.store, fix.table, "ping");
    const vt = try makeUserVt(&fix, sel);
    const recv = try makeReceiver(&fix, vt);

    // Loopback receiver = a different entity sharing the same vt. Sending
    // `sel` to it triggers the override again, recursing.
    g_broken_loopback_eid = try makeReceiver(&fix, vt);
    g_broken_count = 0;

    const override_eid = try mkClosureEid(&fix, @ptrCast(&brokenOverride));
    _ = try fix.store.assert(vt, kernel_eids.ATTR_BIND, ref_mod.mkEid(override_eid), kernel_eids.M_BOOTSTRAP);

    const r = send_mod.send0(ref_mod.mkEid(recv), sel);
    // Eventually guard trips, primitive runs, recursion unwinds. Top-level
    // call returns SENTINEL_PRIM because brokenOverride returns NIL after
    // recursing, so the OUTER bind falls through to primitive.
    try testing.expectEqual(SENTINEL_PRIM, r);
    // Override invoked at least once but bounded — must NOT have run forever.
    try testing.expect(g_broken_count >= 1);
    try testing.expect(g_broken_count <= send_mod.BIND_RECURSION_LIMIT + 1);
}
