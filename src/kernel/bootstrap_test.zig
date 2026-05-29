const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const object = @import("object.zig");
const send_mod = @import("send.zig");
const mcache = @import("mcache.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const kernel_eids = @import("kernel_eids.zig");
const bootstrap = @import("bootstrap.zig");
const interner_mod = @import("interner.zig");

const Ref = ref_mod.Ref;

fn setup(arena: *std.heap.ArenaAllocator) !struct {
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
    kernel: bootstrap.Kernel,
} {
    const allocator = arena.allocator();
    const store = try allocator.create(fact_store.FactStore);
    store.* = fact_store.FactStore.init(allocator);
    const table = try allocator.create(object_table.ObjectTable);
    table.* = object_table.ObjectTable.init(allocator);
    const interner = try allocator.create(interner_mod.Interner);
    interner.* = interner_mod.Interner.init(allocator);
    const k = try bootstrap.boot(allocator, store, table, interner);
    return .{ .store = store, .table = table, .interner = interner, .kernel = k };
}

// -- Bootstrap returns valid kernel handles ----------------------------------

test "boot completes; kernel refs are pointer-tagged" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try setup(&arena);
    try testing.expect(ref_mod.isPointer(s.kernel.vtable_vt_ref));
    try testing.expect(ref_mod.isPointer(s.kernel.object_vt_ref));
    try testing.expect(ref_mod.isPointer(s.kernel.nil_vt_ref));
}

test "kernel vtables registered in object table at well-known EIDs" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try setup(&arena);
    try testing.expectEqual(s.kernel.vtable_vt_ref, s.table.get(kernel_eids.VTABLE_VT).?);
    try testing.expectEqual(s.kernel.object_vt_ref, s.table.get(kernel_eids.OBJECT_VT).?);
    try testing.expectEqual(s.kernel.nil_vt_ref, s.table.get(kernel_eids.NIL_VT).?);
}

test "vtable structs carry mkEid(VTABLE_VT) at offset -1 (metavtable fixed-point)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try setup(&arena);

    const expected = ref_mod.mkEid(kernel_eids.VTABLE_VT);
    try testing.expectEqual(expected, object.vtableOf(object.fromRef(s.kernel.vtable_vt_ref)));
    try testing.expectEqual(expected, object.vtableOf(object.fromRef(s.kernel.object_vt_ref)));
    try testing.expectEqual(expected, object.vtableOf(object.fromRef(s.kernel.nil_vt_ref)));
}

// -- :vtable attribute facts asserted ---------------------------------------

test ":vtable facts let cold-path bind dispatch on EID receivers" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try setup(&arena);

    const vtv = ref_mod.mkEid(kernel_eids.VTABLE_VT);
    try testing.expectEqual(vtv, s.store.currentValue(kernel_eids.VTABLE_VT, kernel_eids.ATTR_VTABLE).?);
    try testing.expectEqual(vtv, s.store.currentValue(kernel_eids.OBJECT_VT, kernel_eids.ATTR_VTABLE).?);
    try testing.expectEqual(ref_mod.mkEid(kernel_eids.NIL_VT), s.store.currentValue(kernel_eids.NIL, kernel_eids.ATTR_VTABLE).?);
}

// -- Delegation chain --------------------------------------------------------

test "vtable_vt delegates to object_vt; object_vt has no delegate (Date: absence)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try setup(&arena);

    const ovt = ref_mod.mkEid(kernel_eids.OBJECT_VT);
    try testing.expectEqual(ovt, s.store.currentValue(kernel_eids.VTABLE_VT, kernel_eids.ATTR_DELEGATE).?);
    try testing.expectEqual(@as(?Ref, null), s.store.currentValue(kernel_eids.OBJECT_VT, kernel_eids.ATTR_DELEGATE));
}

// -- send / bind: real dispatch ---------------------------------------------

test "send init to materialized object returns the object" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    _ = try setup(&arena);

    // Allocate an ordinary object with vt = OBJECT_VT.
    const obj = try object.allocObject(arena.allocator(), ref_mod.mkEid(kernel_eids.OBJECT_VT), 0);
    const obj_ref = object.toRef(obj);

    const result = send_mod.send0(obj_ref, kernel_eids.SEL_INIT);
    try testing.expectEqual(obj_ref, result);
}

test "send vtable to materialized object returns its vtable EID Ref" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    _ = try setup(&arena);

    const obj = try object.allocObject(arena.allocator(), ref_mod.mkEid(kernel_eids.OBJECT_VT), 0);
    const obj_ref = object.toRef(obj);

    const result = send_mod.send0(obj_ref, kernel_eids.SEL_VTABLE);
    try testing.expectEqual(ref_mod.mkEid(kernel_eids.OBJECT_VT), result);
}

test "send to vtable_vt EID uses cold-path :vtable resolution" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    _ = try setup(&arena);

    // send `init` to vtable_vt as an EID receiver. Init lives on object_vt;
    // delegation walks vtable_vt -> object_vt, finds it, dispatches.
    const recv = ref_mod.mkEid(kernel_eids.VTABLE_VT);
    const result = send_mod.send0(recv, kernel_eids.SEL_INIT);
    try testing.expectEqual(recv, result);
}

test "send to NIL is legal (cold path resolves via :vtable fact)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    _ = try setup(&arena);

    // NIL's vtable is NIL_VT, which delegates to object_vt. init returns self.
    const result = send_mod.send0(ref_mod.NIL, kernel_eids.SEL_INIT);
    try testing.expectEqual(ref_mod.NIL, result);
}

// -- vtable lookup: as user-callable reflection ------------------------------

test "[vt lookup: sel] returns the closure ref for an installed method" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try setup(&arena);

    // Look up SEL_INIT on object_vt — installed at bootstrap.
    const recv = ref_mod.mkEid(kernel_eids.OBJECT_VT);
    const result = send_mod.send1(recv, kernel_eids.SEL_LOOKUP, ref_mod.mkEid(kernel_eids.SEL_INIT));
    try testing.expect(ref_mod.isEid(result));
    // Resolve through object table to get the materialized closure.
    const closure_eid = ref_mod.untagEid(result);
    const ptr_ref = s.table.get(closure_eid).?;
    try testing.expect(ref_mod.isPointer(ptr_ref));
}

test "[vt lookup: unknown_sel] returns NIL" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    _ = try setup(&arena);

    const fake_sel = ref_mod.mkEid(9999);
    const recv = ref_mod.mkEid(kernel_eids.OBJECT_VT);
    const result = send_mod.send1(recv, kernel_eids.SEL_LOOKUP, fake_sel);
    try testing.expectEqual(ref_mod.NIL, result);
}

test "[vt lookup: sel] walks delegation chain" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    _ = try setup(&arena);

    // SEL_INIT is on OBJECT_VT but not on VTABLE_VT directly. Looking it up on
    // VTABLE_VT should follow :delegate -> OBJECT_VT and find it.
    const recv = ref_mod.mkEid(kernel_eids.VTABLE_VT);
    const result = send_mod.send1(recv, kernel_eids.SEL_LOOKUP, ref_mod.mkEid(kernel_eids.SEL_INIT));
    try testing.expect(ref_mod.isEid(result));
    try testing.expect(!ref_mod.isNil(result));
}

// -- delegated message: child vtables ---------------------------------------

test "[vt delegated] creates a new child vtable that inherits methods" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try setup(&arena);

    const recv = ref_mod.mkEid(kernel_eids.OBJECT_VT);
    const child = send_mod.send0(recv, kernel_eids.SEL_DELEGATED);
    try testing.expect(ref_mod.isEid(child));
    try testing.expect(!ref_mod.isNil(child));

    const child_eid = ref_mod.untagEid(child);
    // Child should have :delegate = OBJECT_VT.
    const delegate = s.store.currentValue(child_eid, kernel_eids.ATTR_DELEGATE).?;
    try testing.expectEqual(ref_mod.mkEid(kernel_eids.OBJECT_VT), delegate);
    // Child should have :vtable = VTABLE_VT.
    const child_vt = s.store.currentValue(child_eid, kernel_eids.ATTR_VTABLE).?;
    try testing.expectEqual(ref_mod.mkEid(kernel_eids.VTABLE_VT), child_vt);

    // Instances of child should be able to receive `init` (inherited).
    const inst = try object.allocObject(arena.allocator(), child, 0);
    const inst_ref = object.toRef(inst);
    const r = send_mod.send0(inst_ref, kernel_eids.SEL_INIT);
    try testing.expectEqual(inst_ref, r);
}

// -- mcache --------------------------------------------------------------

test "mcache populates on cold path; second send takes hot path" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    _ = try setup(&arena);

    const obj = try object.allocObject(arena.allocator(), ref_mod.mkEid(kernel_eids.OBJECT_VT), 0);
    const obj_ref = object.toRef(obj);

    mcache.clearAll();
    try testing.expectEqual(@as(?*send_mod.Closure, null), mcache.lookup(kernel_eids.OBJECT_VT, kernel_eids.SEL_INIT));

    _ = send_mod.send0(obj_ref, kernel_eids.SEL_INIT);
    try testing.expect(mcache.lookup(kernel_eids.OBJECT_VT, kernel_eids.SEL_INIT) != null);
}

test "mcache invalidates on assert of any (?, sel, ?)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try setup(&arena);

    const obj = try object.allocObject(arena.allocator(), ref_mod.mkEid(kernel_eids.OBJECT_VT), 0);
    const obj_ref = object.toRef(obj);

    _ = send_mod.send0(obj_ref, kernel_eids.SEL_INIT);
    try testing.expect(mcache.lookup(kernel_eids.OBJECT_VT, kernel_eids.SEL_INIT) != null);

    // Trigger a (?, SEL_INIT, ?) mutation — should flush the cache entry.
    const dummy_v = ref_mod.mkEid(12345);
    _ = try s.store.assert(99_999, kernel_eids.SEL_INIT, dummy_v, kernel_eids.M_BOOTSTRAP);
    try testing.expectEqual(@as(?*send_mod.Closure, null), mcache.lookup(kernel_eids.OBJECT_VT, kernel_eids.SEL_INIT));
}
