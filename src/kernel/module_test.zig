const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const id_api_mod = @import("id_api.zig");
const module = @import("module.zig");
const kernel_eids = @import("kernel_eids.zig");
const bootstrap = @import("bootstrap.zig");
const send_mod = @import("send.zig");
const strings = @import("strings.zig");

const Ref = ref_mod.Ref;

// Shell-context shim used by the integration test.
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
} {
    const allocator = arena.allocator();
    const store = try allocator.create(fact_store.FactStore);
    store.* = fact_store.FactStore.init(allocator);
    const table = try allocator.create(object_table.ObjectTable);
    table.* = object_table.ObjectTable.init(allocator);
    const interner = try allocator.create(interner_mod.Interner);
    interner.* = interner_mod.Interner.init(allocator);
    _ = try bootstrap.boot(allocator, store, table, interner);
    id_api_mod.setInterner(interner);

    const api = id_api_mod.populate();
    const manager = try allocator.create(module.Manager);
    manager.* = module.Manager.init(allocator, &shell_ctx, api);
    return .{ .store = store, .table = table, .interner = interner, .manager = manager };
}

test "load hello dylib, send greet, get back the greeting string" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try fullSetup(&arena);

    const dylib_path: [:0]const u8 = "./zig-out/lib/libbirnam_hello.dylib";
    const module_eid = (try s.manager.load(dylib_path)) orelse {
        std.debug.print("could not load hello dylib at {s}\n", .{dylib_path});
        return error.LoadFailed;
    };

    // Hello asserts [module_eid :hello_vt <hello_vt_eid>].
    const sel_hello_vt = try s.interner.intern(s.store, s.table, "hello_vt");
    const hello_vt_ref = s.store.currentValue(module_eid, sel_hello_vt).?;
    try testing.expect(ref_mod.isEid(hello_vt_ref));

    // Allocate an object whose vtable is Hello_vt.
    const hello_obj = try @import("object.zig").allocObject(arena.allocator(), hello_vt_ref, 0);
    const obj_ref = @import("object.zig").toRef(hello_obj);

    // Send greet, expect the string "hello, birnam!".
    const sel_greet = try s.interner.intern(s.store, s.table, "greet");
    const result = send_mod.send0(obj_ref, sel_greet);
    try testing.expect(strings.isString(result));
    try testing.expectEqualSlices(u8, "hello, birnam!", strings.bytes(result));
}

test "unload hello: greet no longer dispatchable; module's facts gone" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try fullSetup(&arena);

    const dylib_path: [:0]const u8 = "./zig-out/lib/libbirnam_hello.dylib";
    const module_eid = (try s.manager.load(dylib_path)) orelse return error.LoadFailed;

    const sel_hello_vt = try s.interner.intern(s.store, s.table, "hello_vt");
    const hello_vt_ref = s.store.currentValue(module_eid, sel_hello_vt).?;

    // Unload.
    try s.manager.unload(module_eid);

    // The hello_vt export fact is gone.
    try testing.expectEqual(@as(?Ref, null), s.store.currentValue(module_eid, sel_hello_vt));

    // Hello_vt's :vtable and :delegate facts are gone.
    const hello_vt_eid = ref_mod.untagEid(hello_vt_ref);
    try testing.expectEqual(@as(?Ref, null), s.store.currentValue(hello_vt_eid, kernel_eids.ATTR_VTABLE));
    try testing.expectEqual(@as(?Ref, null), s.store.currentValue(hello_vt_eid, kernel_eids.ATTR_DELEGATE));

    // Kernel facts untouched — sanity check.
    const ovt_v = ref_mod.mkEid(kernel_eids.VTABLE_VT);
    try testing.expectEqual(ovt_v, s.store.currentValue(kernel_eids.OBJECT_VT, kernel_eids.ATTR_VTABLE).?);
}

test "reload hello: greet works again, identity preserved" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const s = try fullSetup(&arena);

    const dylib_path: [:0]const u8 = "./zig-out/lib/libbirnam_hello.dylib";
    const module_eid = (try s.manager.load(dylib_path)) orelse return error.LoadFailed;

    const sel_greet = try s.interner.intern(s.store, s.table, "greet");
    const sel_hello_vt = try s.interner.intern(s.store, s.table, "hello_vt");

    // First call
    const hello_vt_ref_1 = s.store.currentValue(module_eid, sel_hello_vt).?;
    const obj_1 = try @import("object.zig").allocObject(arena.allocator(), hello_vt_ref_1, 0);
    const r1 = send_mod.send0(@import("object.zig").toRef(obj_1), sel_greet);
    try testing.expect(strings.isString(r1));

    // Reload
    try s.manager.reload(module_eid);

    // module_eid still the same (preserved across reload by design).
    const hello_vt_ref_2 = s.store.currentValue(module_eid, sel_hello_vt).?;
    const obj_2 = try @import("object.zig").allocObject(arena.allocator(), hello_vt_ref_2, 0);
    const r2 = send_mod.send0(@import("object.zig").toRef(obj_2), sel_greet);
    try testing.expect(strings.isString(r2));
    try testing.expectEqualSlices(u8, "hello, birnam!", strings.bytes(r2));
}
