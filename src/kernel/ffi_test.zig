const std = @import("std");
const testing = std.testing;
const builtin = @import("builtin");

const ref_mod = @import("ref.zig");
const boxed = @import("boxed.zig");
const ffi = @import("ffi.zig");

const Ref = ref_mod.Ref;

// -- Sig parser -------------------------------------------------------------

test "parseSig: scalar in, scalar out" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var sig = try ffi.parseSig(a, "d)d");
    defer sig.deinit(a);
    try testing.expectEqual(@as(usize, 1), sig.args.len);
    try testing.expectEqual(ffi.Kind.double, sig.args[0]);
    try testing.expectEqual(ffi.Kind.double, sig.ret);
    try testing.expectEqual(@as(?usize, null), sig.variadic_at);
}

test "parseSig: pointer in, long out" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var sig = try ffi.parseSig(a, "Z)l");
    defer sig.deinit(a);
    try testing.expectEqual(@as(usize, 1), sig.args.len);
    try testing.expectEqual(ffi.Kind.cstring, sig.args[0]);
    try testing.expectEqual(ffi.Kind.long, sig.ret);
}

test "parseSig: variadic marker" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var sig = try ffi.parseSig(a, "Zeii)i");
    defer sig.deinit(a);
    try testing.expectEqual(@as(usize, 3), sig.args.len);
    try testing.expectEqual(ffi.Kind.cstring, sig.args[0]);
    try testing.expectEqual(ffi.Kind.int, sig.args[1]);
    try testing.expectEqual(ffi.Kind.int, sig.args[2]);
    try testing.expectEqual(ffi.Kind.int, sig.ret);
    try testing.expectEqual(@as(?usize, 1), sig.variadic_at);
}

test "parseSig: missing separator" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const r = ffi.parseSig(a, "id");
    try testing.expectError(error.MissingReturnSeparator, r);
}

test "parseSig: unknown sigchar" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    const r = ffi.parseSig(a, "X)i");
    try testing.expectError(error.UnknownSigchar, r);
}

// -- dyncall integration ----------------------------------------------------

const is_darwin = builtin.os.tag == .macos;

fn dlopenSelf() ?std.DynLib {
    return std.DynLib.open("/usr/lib/libSystem.B.dylib") catch null;
}

test "invoke: libm sqrt via dyncall" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var lib = dlopenSelf() orelse return error.SkipZigTest;
    defer lib.close();
    const sym_ptr = lib.lookup(*anyopaque, "sqrt") orelse return error.SkipZigTest;
    const addr: usize = @intFromPtr(sym_ptr);

    var sig = try ffi.parseSig(a, "d)d");
    defer sig.deinit(a);

    const arg = try boxed.mkFloat(a, 16.0);
    const args = [_]Ref{arg};
    const r = ffi.invoke(a, addr, sig, &args);
    try testing.expect(boxed.isFloat(r));
    try testing.expectEqual(@as(f64, 4.0), boxed.untagFloat(r));

    ffi.shutdown();
}

test "invoke: libSystem strlen via dyncall" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var lib = dlopenSelf() orelse return error.SkipZigTest;
    defer lib.close();
    const sym_ptr = lib.lookup(*anyopaque, "strlen") orelse return error.SkipZigTest;
    const addr: usize = @intFromPtr(sym_ptr);

    var sig = try ffi.parseSig(a, "Z)l");
    defer sig.deinit(a);

    const cstr = "hello, world";
    const cstr_ptr = try boxed.mkFfiPtr(a, @intFromPtr(cstr.ptr));
    const args = [_]Ref{cstr_ptr};
    const r = ffi.invoke(a, addr, sig, &args);
    try testing.expect(ref_mod.isSmallInt(r));
    try testing.expectEqual(@as(i64, @intCast(cstr.len)), ref_mod.untagSmallInt(r));

    ffi.shutdown();
}

// -- Wrap + send-dispatch end-to-end ----------------------------------------

const send_mod = @import("send.zig");
const object = @import("object.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const bootstrap = @import("bootstrap.zig");
const kernel_eids = @import("kernel_eids.zig");

fn makeUserVtAndReceiver(
    a: std.mem.Allocator,
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,
) !struct { vt: u64, recv: u64 } {
    const vt = table.alloc();
    const obj_ptr = try object.allocObject(a, ref_mod.mkEid(kernel_eids.VTABLE_VT), 0);
    try table.put(vt, object.toRef(obj_ptr));
    _ = try store.assert(vt, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.VTABLE_VT), kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(vt, kernel_eids.ATTR_DELEGATE, ref_mod.mkEid(kernel_eids.OBJECT_VT), kernel_eids.M_BOOTSTRAP);

    const recv = table.alloc();
    _ = try store.assert(recv, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(vt), kernel_eids.M_BOOTSTRAP);
    return .{ .vt = vt, .recv = recv };
}

test "wrap + send: sqrt(16) via send1 dispatch" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    var table = object_table.ObjectTable.init(a);
    var interner = interner_mod.Interner.init(a);
    _ = try bootstrap.boot(a, &store, &table, &interner);

    var lib = dlopenSelf() orelse return error.SkipZigTest;
    defer lib.close();
    const sym_ptr = lib.lookup(*anyopaque, "sqrt") orelse return error.SkipZigTest;
    const addr: usize = @intFromPtr(sym_ptr);

    const closure_eid = try ffi.registerWrap(a, &store, &table, addr, "d)d");

    const fix = try makeUserVtAndReceiver(a, &store, &table);
    const sel = try interner.intern(&store, &table, "sqrt:");
    _ = try store.assert(fix.vt, sel, ref_mod.mkEid(closure_eid), kernel_eids.M_BOOTSTRAP);

    const arg = try boxed.mkFloat(a, 16.0);
    const r = send_mod.send1(ref_mod.mkEid(fix.recv), sel, arg);
    try testing.expect(boxed.isFloat(r));
    try testing.expectEqual(@as(f64, 4.0), boxed.untagFloat(r));

    ffi.shutdown();
}

test "wrap + send: strlen via send1 dispatch" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    var table = object_table.ObjectTable.init(a);
    var interner = interner_mod.Interner.init(a);
    _ = try bootstrap.boot(a, &store, &table, &interner);

    var lib = dlopenSelf() orelse return error.SkipZigTest;
    defer lib.close();
    const sym_ptr = lib.lookup(*anyopaque, "strlen") orelse return error.SkipZigTest;
    const addr: usize = @intFromPtr(sym_ptr);

    const closure_eid = try ffi.registerWrap(a, &store, &table, addr, "Z)l");

    const fix = try makeUserVtAndReceiver(a, &store, &table);
    const sel = try interner.intern(&store, &table, "strlen:");
    _ = try store.assert(fix.vt, sel, ref_mod.mkEid(closure_eid), kernel_eids.M_BOOTSTRAP);

    const cstr = "birnam";
    const cstr_ptr = try boxed.mkFfiPtr(a, @intFromPtr(cstr.ptr));
    const r = send_mod.send1(ref_mod.mkEid(fix.recv), sel, cstr_ptr);
    try testing.expect(ref_mod.isSmallInt(r));
    try testing.expectEqual(@as(i64, 6), ref_mod.untagSmallInt(r));

    ffi.shutdown();
}

test "wrap: arity > 3 rejected" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    var table = object_table.ObjectTable.init(a);
    var interner = interner_mod.Interner.init(a);
    _ = try bootstrap.boot(a, &store, &table, &interner);

    const r = ffi.registerWrap(a, &store, &table, 0x1000, "iiii)i");
    try testing.expectError(error.ArityTooHigh, r);
}

test "invoke: variadic printf via dyncall arm64 darwin" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var lib = dlopenSelf() orelse return error.SkipZigTest;
    defer lib.close();
    const sym_ptr = lib.lookup(*anyopaque, "printf") orelse return error.SkipZigTest;
    const addr: usize = @intFromPtr(sym_ptr);

    // Sig: fmt-string (named arg), then variadic int.
    // `Z e i ) i` = Z, then variadic begins, then int, returns int.
    var sig = try ffi.parseSig(a, "Zei)i");
    defer sig.deinit(a);

    const fmt = "n=%d\n";
    const fmt_ptr = try boxed.mkFfiPtr(a, @intFromPtr(fmt.ptr));
    const n = ref_mod.mkSmallInt(42).?;
    const args = [_]Ref{ fmt_ptr, n };
    const r = ffi.invoke(a, addr, sig, &args);
    try testing.expect(ref_mod.isSmallInt(r));
    // printf returns number of chars printed. "n=42\n" = 5.
    try testing.expectEqual(@as(i64, 5), ref_mod.untagSmallInt(r));

    ffi.shutdown();
}
