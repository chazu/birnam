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
const HELLO_DYLIB = "zig-out/lib/libbirnam_hello.dylib";

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

// -- [mod symbols] lazy enumeration -----------------------------------------

test "[mod symbols] returns an RRB-Ref of symbol entities" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, HELLO_DYLIB) catch return error.SkipZigTest;
    if (ffi_module.isErrorEntity(fix.store, mod)) return error.SkipZigTest;

    const r = send_mod.send0(mod, kernel_eids.SEL_SYMBOLS);
    try testing.expect(boxed.isRrbRef(r));
    const rrb = boxed.untagRrbRef(r);
    try testing.expect(rrb.count() > 0);
}

test "[mod symbols] enumerates _birnam_module_init as a section + external symbol" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, HELLO_DYLIB) catch return error.SkipZigTest;
    if (ffi_module.isErrorEntity(fix.store, mod)) return error.SkipZigTest;

    const r = send_mod.send0(mod, kernel_eids.SEL_SYMBOLS);
    const rrb = boxed.untagRrbRef(r);

    var found_init = false;
    var i: usize = 0;
    while (i < rrb.count()) : (i += 1) {
        const sym_ref = rrb.nth(i);
        try testing.expect(ref_mod.isEid(sym_ref));
        const sym_eid = ref_mod.untagEid(sym_ref);
        const name_ref = fix.store.currentValue(sym_eid, kernel_eids.ATTR_FFI_SYMBOL_NAME) orelse continue;
        if (!strings.isString(name_ref)) continue;
        if (std.mem.eql(u8, strings.bytes(name_ref), "_birnam_module_init")) {
            const kind = fix.store.currentValue(sym_eid, kernel_eids.ATTR_FFI_SYMBOL_KIND).?;
            try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(kind)); // .section
            const ext = fix.store.currentValue(sym_eid, kernel_eids.ATTR_FFI_SYMBOL_EXTERNAL).?;
            try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(ext));
            const owner = fix.store.currentValue(sym_eid, kernel_eids.ATTR_FFI_SYMBOL_MODULE).?;
            try testing.expectEqual(mod, owner);
            found_init = true;
        }
    }
    try testing.expect(found_init);
}

test "[mod symbols] idempotent — second call doesn't double the count" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, HELLO_DYLIB) catch return error.SkipZigTest;
    if (ffi_module.isErrorEntity(fix.store, mod)) return error.SkipZigTest;

    const r1 = send_mod.send0(mod, kernel_eids.SEL_SYMBOLS);
    const r2 = send_mod.send0(mod, kernel_eids.SEL_SYMBOLS);
    try testing.expectEqual(boxed.untagRrbRef(r1).count(), boxed.untagRrbRef(r2).count());

    // The enumerated-flag fact is set after the first call.
    const flag = fix.store.currentValue(ref_mod.untagEid(mod), kernel_eids.ATTR_FFI_SYMBOLS_ENUMERATED);
    try testing.expect(flag != null);
}

// -- Phase 9.5.5: Module unload + stale-pointer machinery -----------------

const promotion_mod = @import("promotion.zig");

test "[mod unload] succeeds when ffi_depth = 0; marks Module :ffi-stale?" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, HELLO_DYLIB) catch return error.SkipZigTest;
    if (ffi_module.isErrorEntity(fix.store, mod)) return error.SkipZigTest;

    const r = send_mod.send0(mod, kernel_eids.SEL_UNLOAD);
    try testing.expectEqual(mod, r); // unload returns self

    const stale = fix.store.currentValue(ref_mod.untagEid(mod), kernel_eids.ATTR_FFI_STALE);
    try testing.expect(stale != null);
    try testing.expectEqual(@as(i64, 1), ref_mod.untagSmallInt(stale.?));
}

test "[mod unload] refuses with error-entity when ffi_depth > 0" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, HELLO_DYLIB) catch return error.SkipZigTest;
    if (ffi_module.isErrorEntity(fix.store, mod)) return error.SkipZigTest;

    promotion_mod.ffiEnter();
    defer promotion_mod.ffiExit();

    const r = send_mod.send0(mod, kernel_eids.SEL_UNLOAD);
    try testing.expect(ffi_module.isErrorEntity(fix.store, r));
    const kind_ref = fix.store.currentValue(ref_mod.untagEid(r), kernel_eids.ATTR_ERROR_KIND).?;
    const kind_name = fix.interner.nameOf(ref_mod.untagEid(kind_ref)).?;
    try testing.expect(std.mem.eql(u8, kind_name, "ffi-unload-blocked-active-call"));
}

test "[mod unload] stales wraps and ffi-owned boxed pointers" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, LIBSYSTEM) catch return error.SkipZigTest;
    if (ffi_module.isErrorEntity(fix.store, mod)) return error.SkipZigTest;
    const mod_eid = ref_mod.untagEid(mod);

    // Install a strdup wrap so we get an owned FFI pointer back.
    const dup_sel = try fix.interner.intern(fix.store, fix.table, "strdup:");
    _ = send_mod.send3(
        mod,
        kernel_eids.SEL_WRAP_AS_SIG,
        ref_mod.mkEid(dup_sel),
        try strings.mkString(a, "strdup"),
        try strings.mkString(a, "Z)p"),
    );
    const cstr = "hi";
    const cstr_ptr = try boxed.mkFfiPtr(a, @intFromPtr(cstr.ptr));
    const returned_ptr = send_mod.send1(mod, dup_sel, cstr_ptr);
    try testing.expect(boxed.isFfiPtr(returned_ptr));
    const box_eid = boxed.ffiPtrBoxEid(returned_ptr);
    try testing.expect(box_eid != 0);

    // Sanity: the returned ptr's :ffi-owner is mod_eid.
    const owner = fix.store.currentValue(box_eid, kernel_eids.ATTR_FFI_OWNER).?;
    try testing.expectEqual(ref_mod.mkEid(mod_eid), owner);

    // Unload.
    _ = send_mod.send0(mod, kernel_eids.SEL_UNLOAD);

    // Returned pointer is now stale.
    const stale = fix.store.currentValue(box_eid, kernel_eids.ATTR_FFI_STALE);
    try testing.expect(stale != null);

    ffi.shutdown();
}

test "pushArg refuses stale pointers — dispatch returns NIL" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, LIBSYSTEM) catch return error.SkipZigTest;
    if (ffi_module.isErrorEntity(fix.store, mod)) return error.SkipZigTest;
    const mod_eid = ref_mod.untagEid(mod);

    // Wrap strlen so we can pass a pointer back.
    const len_sel = try fix.interner.intern(fix.store, fix.table, "strlen:");
    _ = send_mod.send3(
        mod,
        kernel_eids.SEL_WRAP_AS_SIG,
        ref_mod.mkEid(len_sel),
        try strings.mkString(a, "strlen"),
        try strings.mkString(a, "Z)l"),
    );

    // Mint an owned FFI pointer that lives long enough.
    const cstr = "birnam";
    const owned_ptr = try boxed.mkFfiPtrOwned(a, fix.store, fix.table, @intFromPtr(cstr.ptr), mod_eid);
    const box_eid = boxed.ffiPtrBoxEid(owned_ptr);

    // Pre-stale: dispatch returns 6.
    const r_ok = send_mod.send1(mod, len_sel, owned_ptr);
    try testing.expectEqual(@as(i64, 6), ref_mod.untagSmallInt(r_ok));

    // Stale the pointer manually (simulates post-unload).
    _ = try fix.store.assert(box_eid, kernel_eids.ATTR_FFI_STALE, ref_mod.mkSmallInt(1).?, kernel_eids.M_BOOTSTRAP);

    // Post-stale: pushArg refuses, invoke returns NIL.
    const r_stale = send_mod.send1(mod, len_sel, owned_ptr);
    try testing.expectEqual(ref_mod.NIL, r_stale);

    ffi.shutdown();
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
