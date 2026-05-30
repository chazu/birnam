const std = @import("std");
const testing = std.testing;
const builtin = @import("builtin");

const ref_mod = @import("ref.zig");
const object = @import("object.zig");
const boxed = @import("boxed.zig");
const strings = @import("strings.zig");
const send_mod = @import("send.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const bootstrap = @import("bootstrap.zig");
const ffi_module = @import("ffi_module.zig");
const ffi = @import("ffi.zig");
const id_api_mod = @import("id_api.zig");
const kernel_eids = @import("kernel_eids.zig");

const Ref = ref_mod.Ref;
const is_darwin = builtin.os.tag == .macos;

const Fix = struct {
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
    interner: *interner_mod.Interner,
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
    id_api_mod.setInterner(interner);
    return .{ .store = store, .table = table, .interner = interner };
}

const LIBSYSTEM = "/usr/lib/libSystem.B.dylib";

// -- Module load: ------------------------------------------------------------

test "Module load: returns a Module Ref with :ffi-handle and :ffi-path facts" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = try ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, LIBSYSTEM);
    try testing.expect(ref_mod.isEid(mod));
    try testing.expect(!ffi_module.isErrorEntity(fix.store, mod));

    const mod_eid = ref_mod.untagEid(mod);
    const handle = fix.store.currentValue(mod_eid, kernel_eids.ATTR_FFI_HANDLE).?;
    try testing.expect(boxed.isFfiPtr(handle));

    const path = fix.store.currentValue(mod_eid, kernel_eids.ATTR_FFI_PATH).?;
    try testing.expect(strings.isString(path));
    try testing.expect(std.mem.eql(u8, strings.bytes(path), LIBSYSTEM));
}

test "Module load: nonexistent path returns ERROR_VT diagnostic entity" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const r = try ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, "/no/such/path.dylib");
    try testing.expect(ffi_module.isErrorEntity(fix.store, r));

    const err_eid = ref_mod.untagEid(r);
    const kind_ref = fix.store.currentValue(err_eid, kernel_eids.ATTR_ERROR_KIND).?;
    try testing.expect(ref_mod.isEid(kind_ref));
    const kind_name = fix.interner.nameOf(ref_mod.untagEid(kind_ref)).?;
    try testing.expect(std.mem.eql(u8, kind_name, "ffi-load-failure"));

    const err_path = fix.store.currentValue(err_eid, kernel_eids.ATTR_ERROR_PATH).?;
    try testing.expect(strings.isString(err_path));
}

// -- [mod path] and [mod handle] via send dispatch --------------------------

test "[mod path] returns the boxed-string Ref via send dispatch" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = try ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, LIBSYSTEM);
    const r = send_mod.send0(mod, kernel_eids.SEL_PATH);
    try testing.expect(strings.isString(r));
    try testing.expect(std.mem.eql(u8, strings.bytes(r), LIBSYSTEM));
}

test "[mod handle] returns the boxed FfiPtr Ref via send dispatch" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = try ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, LIBSYSTEM);
    const r = send_mod.send0(mod, kernel_eids.SEL_HANDLE);
    try testing.expect(boxed.isFfiPtr(r));
    try testing.expect(boxed.untagFfiPtr(r) != 0);
}

// -- [mod wrap:as:signature:] + dispatch --------------------------------------

test "[mod wrap:as:signature:] installs method; [mod sqrt:] dispatches via FFI" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = try ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, LIBSYSTEM);
    const sqrt_sel = try fix.interner.intern(fix.store, fix.table, "sqrt:");

    const name_ref = try strings.mkString(a, "sqrt");
    const sig_ref = try strings.mkString(a, "d)d");
    const result = send_mod.send3(mod, kernel_eids.SEL_WRAP_AS_SIG, ref_mod.mkEid(sqrt_sel), name_ref, sig_ref);
    // Chainable: returns receiver (mod) on success.
    try testing.expectEqual(mod, result);

    const arg = try boxed.mkFloat(a, 16.0);
    const r = send_mod.send1(mod, sqrt_sel, arg);
    try testing.expect(boxed.isFloat(r));
    try testing.expectEqual(@as(f64, 4.0), boxed.untagFloat(r));

    ffi.shutdown();
}

test "[mod wrap:] on missing symbol returns ERROR_VT entity" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = try ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, LIBSYSTEM);
    const bogus_sel = try fix.interner.intern(fix.store, fix.table, "noSuch:");

    const name_ref = try strings.mkString(a, "nonexistent_symbol_xyz");
    const sig_ref = try strings.mkString(a, "d)d");
    const result = send_mod.send3(mod, kernel_eids.SEL_WRAP_AS_SIG, ref_mod.mkEid(bogus_sel), name_ref, sig_ref);
    try testing.expect(ffi_module.isErrorEntity(fix.store, result));

    const kind_ref = fix.store.currentValue(ref_mod.untagEid(result), kernel_eids.ATTR_ERROR_KIND).?;
    const kind_name = fix.interner.nameOf(ref_mod.untagEid(kind_ref)).?;
    try testing.expect(std.mem.eql(u8, kind_name, "ffi-symbol-not-found"));
}

test "[mod wrap:] with arity-too-high sig returns ERROR_VT entity" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = try ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, LIBSYSTEM);
    const sel = try fix.interner.intern(fix.store, fix.table, "manyArg:");

    const name_ref = try strings.mkString(a, "printf");
    // 4 args = ArityTooHigh
    const sig_ref = try strings.mkString(a, "iiii)i");
    const result = send_mod.send3(mod, kernel_eids.SEL_WRAP_AS_SIG, ref_mod.mkEid(sel), name_ref, sig_ref);
    try testing.expect(ffi_module.isErrorEntity(fix.store, result));

    const kind_ref = fix.store.currentValue(ref_mod.untagEid(result), kernel_eids.ATTR_ERROR_KIND).?;
    const kind_name = fix.interner.nameOf(ref_mod.untagEid(kind_ref)).?;
    try testing.expect(std.mem.eql(u8, kind_name, "ffi-arity-too-high"));
}
