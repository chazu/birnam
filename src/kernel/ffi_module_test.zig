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

/// Commit a module's declarative wraps: dlopen + dlsym + install. The deferred
/// model means `Module load:` / `wrap:as:signature:` only assert facts; this is
/// the binding step (run for real at REPL line-commit / file-in / image load).
fn resolve(fix: Fix, mod: Ref, a: std.mem.Allocator) void {
    ffi_module.resolveModule(a, fix.store, fix.table, ref_mod.untagEid(mod)) catch {};
}

// -- Module load: ------------------------------------------------------------

test "Module load: records :ffi-path (deferred); resolve opens + sets :ffi-handle" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = try ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, LIBSYSTEM);
    try testing.expect(ref_mod.isEid(mod));
    try testing.expect(!ffi_module.isErrorEntity(fix.store, mod));
    const mod_eid = ref_mod.untagEid(mod);

    // Deferred: :ffi-path recorded, but NO dlopen / :ffi-handle yet.
    const path = fix.store.currentValue(mod_eid, kernel_eids.ATTR_FFI_PATH).?;
    try testing.expect(strings.isString(path));
    try testing.expect(std.mem.eql(u8, strings.bytes(path), LIBSYSTEM));
    try testing.expect(fix.store.currentValue(mod_eid, kernel_eids.ATTR_FFI_HANDLE) == null);

    // Resolve = the deferred dlopen.
    resolve(fix, mod, a);
    const handle = fix.store.currentValue(mod_eid, kernel_eids.ATTR_FFI_HANDLE).?;
    try testing.expect(boxed.isFfiPtr(handle));
}

test "Module load: bad path — load succeeds (deferred); resolve leaves it unopened" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    // dlopen is deferred, so load of a bad path is NOT an error — it just
    // records the path. The open failure surfaces (silently) at resolve.
    const r = try ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, "/no/such/path.dylib");
    try testing.expect(ref_mod.isEid(r));
    try testing.expect(!ffi_module.isErrorEntity(fix.store, r));

    resolve(fix, r, a);
    // dlopen failed → no handle installed; the module stays unopened/inert.
    try testing.expect(fix.store.currentValue(ref_mod.untagEid(r), kernel_eids.ATTR_FFI_HANDLE) == null);
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
    resolve(fix, mod, a); // handle only exists after the deferred dlopen
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
    // Chainable: wrap: is declarative, returns receiver (mod).
    try testing.expectEqual(mod, result);

    // Commit: dlopen + dlsym + install. Only now is the method dispatchable.
    resolve(fix, mod, a);

    const arg = try boxed.mkFloat(a, 16.0);
    const r = send_mod.send1(mod, sqrt_sel, arg);
    try testing.expect(boxed.isFloat(r));
    try testing.expectEqual(@as(f64, 4.0), boxed.untagFloat(r));

    ffi.shutdown();
}

test "[mod wrap:] on missing symbol — resolve leaves the wrap unbound" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = try ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, LIBSYSTEM);
    const bogus_sel = try fix.interner.intern(fix.store, fix.table, "noSuch:");

    const name_ref = try strings.mkString(a, "nonexistent_symbol_xyz");
    const sig_ref = try strings.mkString(a, "d)d");
    // Declarative wrap: succeeds (returns self) — the symbol isn't checked yet.
    const result = send_mod.send3(mod, kernel_eids.SEL_WRAP_AS_SIG, ref_mod.mkEid(bogus_sel), name_ref, sig_ref);
    try testing.expectEqual(mod, result);

    // At resolve, dlsym misses → the wrap is left unbound (no install). Sending
    // the selector finds no method → NIL. No crash, no broken state.
    resolve(fix, mod, a);
    const r = send_mod.send1(mod, bogus_sel, ref_mod.mkSmallInt(1).?);
    try testing.expectEqual(ref_mod.NIL, r);
}

test "[mod wrap:] non-string arg still returns ERROR_VT synchronously" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = try ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, LIBSYSTEM);
    const sel = try fix.interner.intern(fix.store, fix.table, "bad:");
    // sig arg is a SmallInt, not a string → arg-shape error at wrap: time.
    const result = send_mod.send3(mod, kernel_eids.SEL_WRAP_AS_SIG, ref_mod.mkEid(sel), try strings.mkString(a, "x"), ref_mod.mkSmallInt(0).?);
    try testing.expect(ffi_module.isErrorEntity(fix.store, result));

    const kind_ref = fix.store.currentValue(ref_mod.untagEid(result), kernel_eids.ATTR_ERROR_KIND).?;
    const kind_name = fix.interner.nameOf(ref_mod.untagEid(kind_ref)).?;
    try testing.expect(std.mem.eql(u8, kind_name, "ffi-sig-parse-failure"));
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
    resolve(fix, mod, a); // bind the declarative wrap
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
    resolve(fix, mod, a); // bind the declarative wrap

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

test "[mod wrap:] with arity-too-high sig — resolve rejects it, wrap unbound" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = try ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, LIBSYSTEM);
    const sel = try fix.interner.intern(fix.store, fix.table, "manyArg:");

    const name_ref = try strings.mkString(a, "printf");
    // >255 args = ArityTooHigh (the 0..3 cap is gone; the surviving ceiling is
    // SEND_N's u8 arg-count). Declarative wrap still records (returns self);
    // the limit is enforced at resolve (buildWrapClosure), which skips the
    // install — the selector stays unbound.
    var sig_buf: [259]u8 = undefined;
    @memset(sig_buf[0..256], 'i');
    sig_buf[256] = ')';
    sig_buf[257] = 'i';
    const sig_ref = try strings.mkString(a, sig_buf[0..258]);
    const result = send_mod.send3(mod, kernel_eids.SEL_WRAP_AS_SIG, ref_mod.mkEid(sel), name_ref, sig_ref);
    try testing.expectEqual(mod, result);

    resolve(fix, mod, a);
    try testing.expectEqual(ref_mod.NIL, send_mod.send1(mod, sel, ref_mod.mkSmallInt(0).?));
}

// -- Phase 18 prelude: FFI memory primitives --------------------------------

test "FFI_PTR alloc/poke/peek/address/isNull/free round-trip via send" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    _ = try setup(&arena);

    const proto = ref_mod.mkEid(kernel_eids.MODULE_PROTO);
    const cell = send_mod.send1(proto, kernel_eids.SEL_FFI_ALLOC, ref_mod.mkSmallInt(16).?);
    try testing.expect(boxed.isFfiPtr(cell));

    // alloc zeroes the buffer.
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(send_mod.send1(cell, kernel_eids.SEL_FFI_PEEK_INT, ref_mod.mkSmallInt(0).?)));
    // not null.
    try testing.expectEqual(ref_mod.mkEid(kernel_eids.FALSE_EID), send_mod.send0(cell, kernel_eids.SEL_FFI_IS_NULL));

    // pokeInt:at: returns self (chainable).
    const back = send_mod.send2(cell, kernel_eids.SEL_FFI_POKE_INT, ref_mod.mkSmallInt(123456789).?, ref_mod.mkSmallInt(0).?);
    try testing.expectEqual(cell, back);
    try testing.expectEqual(@as(i64, 123456789), ref_mod.untagSmallInt(send_mod.send1(cell, kernel_eids.SEL_FFI_PEEK_INT, ref_mod.mkSmallInt(0).?)));
    // low byte of 123456789 = 21.
    try testing.expectEqual(@as(i64, 21), ref_mod.untagSmallInt(send_mod.send1(cell, kernel_eids.SEL_FFI_PEEK_BYTE, ref_mod.mkSmallInt(0).?)));
    // address is the boxed raw addr.
    try testing.expect(ref_mod.untagSmallInt(send_mod.send0(cell, kernel_eids.SEL_FFI_ADDRESS)) > 0);

    _ = send_mod.send0(cell, kernel_eids.SEL_FFI_FREE);
}

test "resolve reuses the instance vt across passes (no churn; cold-but-valid)" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = try ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, LIBSYSTEM);
    const mod_eid = ref_mod.untagEid(mod);
    const sqrt_sel = try fix.interner.intern(fix.store, fix.table, "sqrt:");
    _ = send_mod.send3(mod, kernel_eids.SEL_WRAP_AS_SIG, ref_mod.mkEid(sqrt_sel), try strings.mkString(a, "sqrt"), try strings.mkString(a, "d)d"));

    resolve(fix, mod, a);
    const vt1 = ref_mod.untagEid(fix.store.currentValue(mod_eid, kernel_eids.ATTR_VTABLE).?);

    // The vt is demoted by the D6 hook the moment it is created (its own
    // :vtable/:delegate asserts touch the freshly-materialized entity), so it is
    // NOT in the object table — yet it is a valid, dispatchable entity.
    try testing.expect(fix.table.get(vt1) == null);

    // Re-resolving (as every REPL line-commit does) must REUSE vt1, not re-mint.
    resolve(fix, mod, a);
    const vt2 = ref_mod.untagEid(fix.store.currentValue(mod_eid, kernel_eids.ATTR_VTABLE).?);
    try testing.expectEqual(vt1, vt2);

    // And the selector still dispatches through the reused cold vt.
    try testing.expectEqual(@as(f64, 5.0), boxed.untagFloat(send_mod.send1(mod, sqrt_sel, try boxed.mkFloat(a, 25.0))));

    ffi.shutdown();
}

test "resolve re-installs onto a re-minted instance vt (self-healing)" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = try ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, LIBSYSTEM);
    const mod_eid = ref_mod.untagEid(mod);
    const sqrt_sel = try fix.interner.intern(fix.store, fix.table, "sqrt:");

    _ = send_mod.send3(mod, kernel_eids.SEL_WRAP_AS_SIG, ref_mod.mkEid(sqrt_sel), try strings.mkString(a, "sqrt"), try strings.mkString(a, "d)d"));
    resolve(fix, mod, a);
    try testing.expectEqual(@as(f64, 4.0), boxed.untagFloat(send_mod.send1(mod, sqrt_sel, try boxed.mkFloat(a, 16.0))));

    // Simulate the instance vt being lost between REPL lines (GC reclaiming the
    // vt object → ensureInstanceVt re-mints a fresh one): retract the :vtable
    // fact so the next resolve mints a new, empty vt. With the old skip-on-
    // :ffi-addr logic the wrap would be skipped and sqrt: would vanish; the
    // self-healing install must re-bind it on the new vt.
    const vt_ref = fix.store.currentValue(mod_eid, kernel_eids.ATTR_VTABLE).?;
    _ = try fix.store.retract(mod_eid, kernel_eids.ATTR_VTABLE, vt_ref, kernel_eids.M_BOOTSTRAP);

    resolve(fix, mod, a);
    const new_vt = fix.store.currentValue(mod_eid, kernel_eids.ATTR_VTABLE).?;
    try testing.expect(vt_ref != new_vt); // genuinely re-minted
    try testing.expectEqual(@as(f64, 9.0), boxed.untagFloat(send_mod.send1(mod, sqrt_sel, try boxed.mkFloat(a, 81.0))));

    ffi.shutdown();
}

test "out-param: strcpy into alloc'd buffer; peekString reads it back" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = try ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, LIBSYSTEM);
    const sel = try fix.interner.intern(fix.store, fix.table, "copyTo:from:");

    // char *strcpy(char *dst, const char *src) — dst is an FfiPtr (p), src a
    // birnam string (Z, auto-marshaled), returns dst (p).
    _ = send_mod.send3(mod, kernel_eids.SEL_WRAP_AS_SIG, ref_mod.mkEid(sel), try strings.mkString(a, "strcpy"), try strings.mkString(a, "pZ)p"));
    resolve(fix, mod, a);

    const proto = ref_mod.mkEid(kernel_eids.MODULE_PROTO);
    const dst = send_mod.send1(proto, kernel_eids.SEL_FFI_ALLOC, ref_mod.mkSmallInt(8).?);
    try testing.expect(boxed.isFfiPtr(dst));

    _ = send_mod.send2(mod, sel, dst, try strings.mkString(a, "hi"));

    const got = send_mod.send1(dst, kernel_eids.SEL_FFI_PEEK_STRING, ref_mod.mkSmallInt(0).?);
    try testing.expect(strings.isString(got));
    try testing.expect(std.mem.eql(u8, strings.bytes(got), "hi"));

    _ = send_mod.send0(dst, kernel_eids.SEL_FFI_FREE);
    ffi.shutdown();
}

test "argv: + peekStringArray: char** round-trip (19.3)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    _ = try setup(&arena);

    const proto = ref_mod.mkEid(kernel_eids.MODULE_PROTO);

    // birnam vector #["alpha" "beta" "gamma"] -> single-block char**.
    const vec = try boxed.mkRrb(a, &.{
        try strings.mkString(a, "alpha"),
        try strings.mkString(a, "beta"),
        try strings.mkString(a, "gamma"),
    });
    const argv = send_mod.send1(proto, kernel_eids.SEL_FFI_ARGV, vec);
    try testing.expect(boxed.isFfiPtr(argv));

    // Cross-check the raw C layout independently of peekStringArray: slot 0 is a
    // pointer; the cstring it points at is "alpha".
    const slot0 = send_mod.send1(argv, kernel_eids.SEL_FFI_PEEK_PTR, ref_mod.mkSmallInt(0).?);
    try testing.expect(boxed.isFfiPtr(slot0));
    const s0 = send_mod.send1(slot0, kernel_eids.SEL_FFI_PEEK_STRING, ref_mod.mkSmallInt(0).?);
    try testing.expect(std.mem.eql(u8, strings.bytes(s0), "alpha"));

    // count-bounded read back.
    const got = send_mod.send1(argv, kernel_eids.SEL_FFI_PEEK_STRING_ARRAY, ref_mod.mkSmallInt(3).?);
    try testing.expect(boxed.isRrbRef(got));
    try testing.expectEqual(@as(usize, 3), boxed.rrbLen(got));
    try testing.expect(std.mem.eql(u8, strings.bytes(boxed.rrbNth(got, 0)), "alpha"));
    try testing.expect(std.mem.eql(u8, strings.bytes(boxed.rrbNth(got, 1)), "beta"));
    try testing.expect(std.mem.eql(u8, strings.bytes(boxed.rrbNth(got, 2)), "gamma"));

    // NULL-terminated read (nil count) stops at the terminator slot → 3 elems.
    const got_nt = send_mod.send1(argv, kernel_eids.SEL_FFI_PEEK_STRING_ARRAY, ref_mod.NIL);
    try testing.expect(boxed.isRrbRef(got_nt));
    try testing.expectEqual(@as(usize, 3), boxed.rrbLen(got_nt));
    try testing.expect(std.mem.eql(u8, strings.bytes(boxed.rrbNth(got_nt, 2)), "gamma"));

    // one free reclaims the whole block (pointer array + all strings).
    _ = send_mod.send0(argv, kernel_eids.SEL_FFI_FREE);
}

test "argv: rejects a non-vector / non-string element (19.3)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    _ = try setup(&arena);

    const proto = ref_mod.mkEid(kernel_eids.MODULE_PROTO);

    // not a vector → nil.
    try testing.expectEqual(ref_mod.NIL, send_mod.send1(proto, kernel_eids.SEL_FFI_ARGV, ref_mod.mkSmallInt(5).?));

    // vector with a non-string element → nil (no partial alloc handed back).
    const bad = try boxed.mkRrb(a, &.{
        try strings.mkString(a, "ok"),
        ref_mod.mkSmallInt(7).?,
    });
    try testing.expectEqual(ref_mod.NIL, send_mod.send1(proto, kernel_eids.SEL_FFI_ARGV, bad));
}

test "pokeBytes:at: + peekBytes:len: bulk copy round-trip (19.4)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    _ = try setup(&arena);

    const proto = ref_mod.mkEid(kernel_eids.MODULE_PROTO);
    const buf = send_mod.send1(proto, kernel_eids.SEL_FFI_ALLOC, ref_mod.mkSmallInt(16).?);
    try testing.expect(boxed.isFfiPtr(buf));

    // poke a string's bytes at offset 0; pokeBytes returns self (chainable).
    const back = send_mod.send2(buf, kernel_eids.SEL_FFI_POKE_BYTES, try strings.mkString(a, "hello"), ref_mod.mkSmallInt(0).?);
    try testing.expectEqual(buf, back);

    const got = send_mod.send2(buf, kernel_eids.SEL_FFI_PEEK_BYTES, ref_mod.mkSmallInt(0).?, ref_mod.mkSmallInt(5).?);
    try testing.expect(strings.isString(got));
    try testing.expect(std.mem.eql(u8, strings.bytes(got), "hello"));

    // poke a byte-vector #[1 2 254] at offset 8; read it back as raw bytes.
    const bytes_vec = try boxed.mkRrb(a, &.{
        ref_mod.mkSmallInt(1).?, ref_mod.mkSmallInt(2).?, ref_mod.mkSmallInt(254).?,
    });
    _ = send_mod.send2(buf, kernel_eids.SEL_FFI_POKE_BYTES, bytes_vec, ref_mod.mkSmallInt(8).?);
    const got2 = send_mod.send2(buf, kernel_eids.SEL_FFI_PEEK_BYTES, ref_mod.mkSmallInt(8).?, ref_mod.mkSmallInt(3).?);
    try testing.expect(strings.isString(got2));
    try testing.expectEqualSlices(u8, &.{ 1, 2, 254 }, strings.bytes(got2));

    // peek of len 0 is the empty string.
    const empty = send_mod.send2(buf, kernel_eids.SEL_FFI_PEEK_BYTES, ref_mod.mkSmallInt(0).?, ref_mod.mkSmallInt(0).?);
    try testing.expect(strings.isString(empty));
    try testing.expectEqual(@as(usize, 0), strings.bytes(empty).len);

    // bad source type → nil; negative len → nil.
    try testing.expectEqual(ref_mod.NIL, send_mod.send2(buf, kernel_eids.SEL_FFI_POKE_BYTES, ref_mod.mkSmallInt(9).?, ref_mod.mkSmallInt(0).?));
    try testing.expectEqual(ref_mod.NIL, send_mod.send2(buf, kernel_eids.SEL_FFI_PEEK_BYTES, ref_mod.mkSmallInt(0).?, ref_mod.mkSmallInt(-1).?));

    _ = send_mod.send0(buf, kernel_eids.SEL_FFI_FREE);
}

test "out-param cells: outInt/outLong/outPtr/outDouble value (19.7)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    _ = try setup(&arena);

    const proto = ref_mod.mkEid(kernel_eids.MODULE_PROTO);

    // outLong: an 8-byte cell; pokeInt writes an i64 that value reads back.
    const cl = send_mod.send0(proto, kernel_eids.SEL_FFI_OUT_LONG);
    try testing.expect(boxed.isFfiPtr(cl));
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(send_mod.send0(cl, kernel_eids.SEL_VALUE))); // zeroed
    _ = send_mod.send2(cl, kernel_eids.SEL_FFI_POKE_INT, ref_mod.mkSmallInt(123456).?, ref_mod.mkSmallInt(0).?);
    try testing.expectEqual(@as(i64, 123456), ref_mod.untagSmallInt(send_mod.send0(cl, kernel_eids.SEL_VALUE)));
    _ = send_mod.send0(cl, kernel_eids.SEL_FFI_FREE);

    // outInt: a 4-byte cell, read as i32. Write little-endian bytes via pokeBytes
    // (pokeInt would overflow the 4-byte buffer). 0x80000000 must sign-extend.
    const ci = send_mod.send0(proto, kernel_eids.SEL_FFI_OUT_INT);
    const le = try boxed.mkRrb(a, &.{
        ref_mod.mkSmallInt(0).?, ref_mod.mkSmallInt(0).?, ref_mod.mkSmallInt(0).?, ref_mod.mkSmallInt(0x80).?,
    });
    _ = send_mod.send2(ci, kernel_eids.SEL_FFI_POKE_BYTES, le, ref_mod.mkSmallInt(0).?);
    try testing.expectEqual(@as(i64, -2147483648), ref_mod.untagSmallInt(send_mod.send0(ci, kernel_eids.SEL_VALUE)));
    _ = send_mod.send0(ci, kernel_eids.SEL_FFI_FREE);

    // outDouble.
    const cd = send_mod.send0(proto, kernel_eids.SEL_FFI_OUT_DOUBLE);
    _ = send_mod.send2(cd, kernel_eids.SEL_FFI_POKE_DOUBLE, try boxed.mkFloat(a, 3.5), ref_mod.mkSmallInt(0).?);
    try testing.expectEqual(@as(f64, 3.5), boxed.untagFloat(send_mod.send0(cd, kernel_eids.SEL_VALUE)));
    _ = send_mod.send0(cd, kernel_eids.SEL_FFI_FREE);

    // outPtr: pokePtr a known address; value reads it back as an FfiPtr.
    const cp = send_mod.send0(proto, kernel_eids.SEL_FFI_OUT_PTR);
    const some = send_mod.send1(proto, kernel_eids.SEL_FFI_ALLOC, ref_mod.mkSmallInt(8).?);
    _ = send_mod.send2(cp, kernel_eids.SEL_FFI_POKE_PTR, some, ref_mod.mkSmallInt(0).?);
    const got_ptr = send_mod.send0(cp, kernel_eids.SEL_VALUE);
    try testing.expect(boxed.isFfiPtr(got_ptr));
    try testing.expectEqual(boxed.untagFfiPtr(some), boxed.untagFfiPtr(got_ptr));
    _ = send_mod.send0(some, kernel_eids.SEL_FFI_FREE);
    _ = send_mod.send0(cp, kernel_eids.SEL_FFI_FREE);

    // a plain (non-cell) alloc'd pointer has width tag 0 -> value is nil.
    const plain = send_mod.send1(proto, kernel_eids.SEL_FFI_ALLOC, ref_mod.mkSmallInt(8).?);
    try testing.expectEqual(ref_mod.NIL, send_mod.send0(plain, kernel_eids.SEL_VALUE));
    _ = send_mod.send0(plain, kernel_eids.SEL_FFI_FREE);
}

test "out-param cell: real C out-param via strtol endptr (19.7)" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const fix = try setup(&arena);
    const mod = try ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, LIBSYSTEM);
    const sel = try fix.interner.intern(fix.store, fix.table, "parse:end:base:");

    // long strtol(const char *nptr, char **endptr, int base). nptr is passed as a
    // STABLE alloc'd buffer (`p`), NOT a `Z` string: strtol writes into *endptr a
    // pointer INTO nptr, and a `Z` arg's scratch buffer is freed after the call —
    // endptr would dangle. So poke the input into an alloc'd cell and pass `p`.
    _ = send_mod.send3(mod, kernel_eids.SEL_WRAP_AS_SIG, ref_mod.mkEid(sel), try strings.mkString(a, "strtol"), try strings.mkString(a, "ppi)l"));
    resolve(fix, mod, a);

    const proto = ref_mod.mkEid(kernel_eids.MODULE_PROTO);
    const input = send_mod.send1(proto, kernel_eids.SEL_FFI_ALLOC, ref_mod.mkSmallInt(6).?); // "42abc" + NUL (alloc zeroes)
    _ = send_mod.send2(input, kernel_eids.SEL_FFI_POKE_BYTES, try strings.mkString(a, "42abc"), ref_mod.mkSmallInt(0).?);
    const endptr = send_mod.send0(proto, kernel_eids.SEL_FFI_OUT_PTR);

    // "42abc" base 10 -> 42, and endptr left pointing at "abc".
    const n = send_mod.send3(mod, sel, input, endptr, ref_mod.mkSmallInt(10).?);
    try testing.expectEqual(@as(i64, 42), ref_mod.untagSmallInt(n));

    const rest = send_mod.send0(endptr, kernel_eids.SEL_VALUE); // FfiPtr to where parsing stopped
    try testing.expect(boxed.isFfiPtr(rest));
    const rest_str = send_mod.send1(rest, kernel_eids.SEL_FFI_PEEK_STRING, ref_mod.mkSmallInt(0).?);
    try testing.expect(std.mem.eql(u8, strings.bytes(rest_str), "abc"));

    _ = send_mod.send0(input, kernel_eids.SEL_FFI_FREE);
    _ = send_mod.send0(endptr, kernel_eids.SEL_FFI_FREE);
    ffi.shutdown();
}

test "struct-as-entity: define, new, field get/set, ptr layout (19.5)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const fix = try setup(&arena);

    const proto = ref_mod.mkEid(kernel_eids.MODULE_PROTO);

    // [Module struct: #Point fields: #(#x #l  #y #l)]  -- two C longs.
    const x_sel = try fix.interner.intern(fix.store, fix.table, "x");
    const y_sel = try fix.interner.intern(fix.store, fix.table, "y");
    const l_sym = try fix.interner.intern(fix.store, fix.table, "l");
    const name = ref_mod.mkEid(try fix.interner.intern(fix.store, fix.table, "Point"));
    const fields = try boxed.mkRrb(a, &.{
        ref_mod.mkEid(x_sel), ref_mod.mkEid(l_sym),
        ref_mod.mkEid(y_sel), ref_mod.mkEid(l_sym),
    });
    const PointT = send_mod.send2(proto, kernel_eids.SEL_FFI_STRUCT, name, fields);
    try testing.expect(ref_mod.isPointer(PointT));

    // [Point new] -> instance; fields zeroed.
    const pt = send_mod.send0(PointT, kernel_eids.SEL_FFI_STRUCT_NEW);
    try testing.expectEqual(@as(i64, 0), ref_mod.untagSmallInt(send_mod.send0(pt, x_sel)));

    // [pt x: 30] / [pt y: 12]  (setters return self).
    const x_set = try fix.interner.intern(fix.store, fix.table, "x:");
    const y_set = try fix.interner.intern(fix.store, fix.table, "y:");
    try testing.expectEqual(pt, send_mod.send1(pt, x_set, ref_mod.mkSmallInt(30).?));
    _ = send_mod.send1(pt, y_set, ref_mod.mkSmallInt(12).?);

    // [pt x] / [pt y] read them back.
    try testing.expectEqual(@as(i64, 30), ref_mod.untagSmallInt(send_mod.send0(pt, x_sel)));
    try testing.expectEqual(@as(i64, 12), ref_mod.untagSmallInt(send_mod.send0(pt, y_sel)));

    // [pt ptr] is the backing struct*; cross-check field offsets via raw peek
    // (x at 0, y at 8 — independent of the accessor path).
    const raw = send_mod.send0(pt, kernel_eids.SEL_FFI_STRUCT_PTR);
    try testing.expect(boxed.isFfiPtr(raw));
    try testing.expectEqual(@as(i64, 30), ref_mod.untagSmallInt(send_mod.send1(raw, kernel_eids.SEL_FFI_PEEK_INT, ref_mod.mkSmallInt(0).?)));
    try testing.expectEqual(@as(i64, 12), ref_mod.untagSmallInt(send_mod.send1(raw, kernel_eids.SEL_FFI_PEEK_INT, ref_mod.mkSmallInt(8).?)));

    _ = send_mod.send0(pt, kernel_eids.SEL_FFI_FREE);
}

test "struct-as-entity: wrap a C struct return (libc div -> div_t) (19.5)" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();
    const fix = try setup(&arena);

    const mod = try ffi_module.moduleLoad(a, fix.store, fix.table, fix.interner, LIBSYSTEM);
    const sel = try fix.interner.intern(fix.store, fix.table, "div:by:");
    // div_t div(int, int) -> {int quot; int rem;}, returned as an FfiPtr.
    _ = send_mod.send3(mod, kernel_eids.SEL_WRAP_AS_SIG, ref_mod.mkEid(sel), try strings.mkString(a, "div"), try strings.mkString(a, "ii){ii}"));
    resolve(fix, mod, a);

    const proto = ref_mod.mkEid(kernel_eids.MODULE_PROTO);
    const quot_sel = try fix.interner.intern(fix.store, fix.table, "quot");
    const rem_sel = try fix.interner.intern(fix.store, fix.table, "rem");
    const i_sym = try fix.interner.intern(fix.store, fix.table, "i");
    const fields = try boxed.mkRrb(a, &.{
        ref_mod.mkEid(quot_sel), ref_mod.mkEid(i_sym),
        ref_mod.mkEid(rem_sel), ref_mod.mkEid(i_sym),
    });
    const DivT = send_mod.send2(proto, kernel_eids.SEL_FFI_STRUCT, ref_mod.mkEid(try fix.interner.intern(fix.store, fix.table, "DivT")), fields);

    // div(17, 5) -> {quot:3, rem:2}. Wrap the returned struct* as an entity.
    const result = send_mod.send2(mod, sel, ref_mod.mkSmallInt(17).?, ref_mod.mkSmallInt(5).?);
    try testing.expect(boxed.isFfiPtr(result));
    const dv = send_mod.send1(DivT, kernel_eids.SEL_FFI_STRUCT_FROM, result);
    try testing.expectEqual(@as(i64, 3), ref_mod.untagSmallInt(send_mod.send0(dv, quot_sel)));
    try testing.expectEqual(@as(i64, 2), ref_mod.untagSmallInt(send_mod.send0(dv, rem_sel)));

    std.c.free(@ptrFromInt(boxed.untagFfiPtr(result)));
    ffi.shutdown();
}
