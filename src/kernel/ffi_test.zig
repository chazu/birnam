const std = @import("std");
const testing = std.testing;
const builtin = @import("builtin");

const ref_mod = @import("ref.zig");
const boxed = @import("boxed.zig");
const ffi = @import("ffi.zig");
const strings = @import("strings.zig");

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

test "parseSig: unsigned u-prefix on arg + ret (19.6)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    // mix: signed int, then unsigned int; unsigned int return.
    var sig = try ffi.parseSig(a, "iui)ui");
    defer sig.deinit(a);
    try testing.expectEqual(@as(usize, 2), sig.args.len);
    try testing.expectEqual(ffi.Kind.int, sig.args[0]);
    try testing.expectEqual(ffi.Kind.int, sig.args[1]);
    try testing.expectEqual(@as(usize, 2), sig.arg_unsigned.len);
    try testing.expect(!sig.arg_unsigned[0]);
    try testing.expect(sig.arg_unsigned[1]);
    try testing.expectEqual(ffi.Kind.int, sig.ret);
    try testing.expect(sig.ret_unsigned);
}

test "parseSig: u on a non-integer type is rejected (19.6)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    try testing.expectError(error.UnknownSigchar, ffi.parseSig(a, "uf)i")); // unsigned float
    try testing.expectError(error.UnknownSigchar, ffi.parseSig(a, "uZ)i")); // unsigned cstring
}

test "parseSig: dangling u is rejected (19.6)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    try testing.expectError(error.UnknownSigchar, ffi.parseSig(a, "u)i")); // arg: u then ')'
    try testing.expectError(error.UnknownSigchar, ffi.parseSig(a, "i)u")); // ret: trailing u
}

// -- dyncall integration ----------------------------------------------------

const is_darwin = builtin.os.tag == .macos;

fn dlopenSelf() ?std.DynLib {
    return std.DynLib.open("/usr/lib/libSystem.B.dylib") catch null;
}

test "invoke: libm sqrt via libffi" {
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

test "invoke: libSystem strlen via libffi" {
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

test "invoke: unsigned arg + return via htonl (19.6)" {
    // htonl(uint32) -> uint32 is a pure byte-swap — a deterministic oracle for
    // the unsigned marshal path on a little-endian host. `ui)ui` exercises BOTH
    // directions of the high bit:
    //   - return zero-extend: htonl(0x00000080) = 0x80000000 (2147483648).
    //     A signed `i` return would sign-extend that to -2147483648.
    //   - arg accepts > i32 max without a range-checked @intCast panic:
    //     htonl(0x80000000) = 0x00000080 (128).
    if (!is_darwin) return error.SkipZigTest;
    if (builtin.cpu.arch.endian() != .little) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var lib = dlopenSelf() orelse return error.SkipZigTest;
    defer lib.close();
    const sym_ptr = lib.lookup(*anyopaque, "htonl") orelse return error.SkipZigTest;
    const addr: usize = @intFromPtr(sym_ptr);

    var sig = try ffi.parseSig(a, "ui)ui");
    defer sig.deinit(a);

    // return zero-extend (high bit set)
    {
        const args = [_]Ref{ref_mod.mkSmallInt(0x80).?};
        const r = ffi.invoke(a, addr, sig, &args);
        try testing.expect(ref_mod.isSmallInt(r));
        try testing.expectEqual(@as(i64, 0x80000000), ref_mod.untagSmallInt(r));
    }
    // arg above i32 max marshals without panic
    {
        const args = [_]Ref{ref_mod.mkSmallInt(0x80000000).?};
        const r = ffi.invoke(a, addr, sig, &args);
        try testing.expect(ref_mod.isSmallInt(r));
        try testing.expectEqual(@as(i64, 0x80), ref_mod.untagSmallInt(r));
    }

    ffi.shutdown();
}

// -- Wrap + send-dispatch end-to-end ----------------------------------------

const send_mod = @import("send.zig");
const object = @import("object.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const siphon = @import("siphon.zig");
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
    _ = try siphon.boot(a, &store, &table, &interner);

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
    _ = try siphon.boot(a, &store, &table, &interner);

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

test "wrap: arity 4 now accepted (old 0..3 cap removed)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    var table = object_table.ObjectTable.init(a);
    var interner = interner_mod.Interner.init(a);
    _ = try siphon.boot(a, &store, &table, &interner);

    // 4 args: rejected pre-change, now resolves to a dispatch closure.
    const closure_eid = try ffi.registerWrap(a, &store, &table, 0x1000, "iiii)i");
    try testing.expect(closure_eid != 0);
}

test "wrap: arity > 255 still rejected (SEND_N u8 ceiling)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    var table = object_table.ObjectTable.init(a);
    var interner = interner_mod.Interner.init(a);
    _ = try siphon.boot(a, &store, &table, &interner);

    // 256 'i' args + ")i". Exceeds SEND_N's u8 arg-count operand.
    var buf: [259]u8 = undefined;
    @memset(buf[0..256], 'i');
    buf[256] = ')';
    buf[257] = 'i';
    const r = ffi.registerWrap(a, &store, &table, 0x1000, buf[0..258]);
    try testing.expectError(error.ArityTooHigh, r);
}

test "invoke: variadic printf via libffi arm64 darwin" {
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

// -- Phase 15.E — Sig free hook ---------------------------------------------

test "freeSig: frees the heap Sig holder and retracts :ffi-sig" {
    // Store on an arena (auto-freed); the Sig holder on the leak-checked
    // testing allocator, so a missed free of struct+args is flagged.
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    var store = fact_store.FactStore.init(arena.allocator());

    const wrap_eid: u64 = 4242;
    const sig = try ffi.parseSig(testing.allocator, "d)d"); // allocates sig.args
    const holder = try testing.allocator.create(ffi.Sig);
    holder.* = sig;
    _ = try store.assert(wrap_eid, kernel_eids.ATTR_FFI_SIG, ref_mod.mkPointer(@ptrCast(holder)), kernel_eids.M_BOOTSTRAP);

    ffi.freeSig(&store, testing.allocator, wrap_eid);

    // Fact retracted → resolveWrap would miss → shim safely returns NIL.
    try testing.expect(store.currentValue(wrap_eid, kernel_eids.ATTR_FFI_SIG) == null);
    // freeSig on a wrap with no :ffi-sig is a no-op.
    ffi.freeSig(&store, testing.allocator, 9999);
}

// -- Phase 18 prelude: cstring marshaling (changes 2/3/4) -------------------

test "invoke: Z string arg + Z cstring return (getenv PATH)" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var lib = dlopenSelf() orelse return error.SkipZigTest;
    defer lib.close();
    const addr: usize = @intFromPtr(lib.lookup(*anyopaque, "getenv") orelse return error.SkipZigTest);

    var sig = try ffi.parseSig(a, "Z)Z");
    defer sig.deinit(a);

    // Change 2: pass a birnam string for the `Z` arg (auto NUL-terminated copy).
    // Change 4: the `char*` return comes back as a birnam string.
    const name = try strings.mkString(a, "PATH");
    const r = ffi.invoke(a, addr, sig, &[_]Ref{name});
    try testing.expect(strings.isString(r));
    try testing.expect(strings.bytes(r).len > 0);

    ffi.shutdown();
}

test "invoke: Z cstring return NULL becomes nil (getenv missing)" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var lib = dlopenSelf() orelse return error.SkipZigTest;
    defer lib.close();
    const addr: usize = @intFromPtr(lib.lookup(*anyopaque, "getenv") orelse return error.SkipZigTest);

    var sig = try ffi.parseSig(a, "Z)Z");
    defer sig.deinit(a);

    const name = try strings.mkString(a, "BIRNAM_NONEXISTENT_VAR_ZZZ_42");
    const r = ffi.invoke(a, addr, sig, &[_]Ref{name});
    try testing.expectEqual(ref_mod.NIL, r);

    ffi.shutdown();
}

test "invoke: nil marshals to NULL pointer arg (free NULL)" {
    if (!is_darwin) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var lib = dlopenSelf() orelse return error.SkipZigTest;
    defer lib.close();
    const addr: usize = @intFromPtr(lib.lookup(*anyopaque, "free") orelse return error.SkipZigTest);

    var sig = try ffi.parseSig(a, "p)v");
    defer sig.deinit(a);

    // Change 3: nil → NULL. free(NULL) is a well-defined no-op; void return → nil.
    const r = ffi.invoke(a, addr, sig, &[_]Ref{ref_mod.NIL});
    try testing.expectEqual(ref_mod.NIL, r);

    ffi.shutdown();
}

// -- Phase 18 prelude: struct-by-value --------------------------------------

const Point = extern struct { x: c_longlong, y: c_longlong };
export fn birnam_test_sumPoint(p: Point) callconv(.c) c_longlong {
    return p.x + p.y;
}
export fn birnam_test_makePoint(a: c_longlong, b: c_longlong) callconv(.c) Point {
    return .{ .x = a, .y = b };
}

const DivT = extern struct { quot: c_int, rem: c_int };

// A homogeneous float aggregate (HFA): on AAPCS a struct of ≤4 same-typed
// floats/doubles is passed/returned in the V (SIMD) registers, NOT the GP regs
// or stack. This is the arm64-specific classification dyncall 1.4 could not do
// and the int structs above do not exercise.
const Vec2 = extern struct { x: f64, y: f64 };
export fn birnam_test_sumVec2(v: Vec2) callconv(.c) f64 {
    return v.x + v.y;
}
export fn birnam_test_makeVec2(a: f64, b: f64) callconv(.c) Vec2 {
    return .{ .x = a, .y = b };
}

// Phase 19.5 nested/array struct fields.
const Line = extern struct { a: Point, b: Point }; // struct-of-structs
export fn birnam_test_sumLine(l: Line) callconv(.c) c_longlong {
    return l.a.x + l.a.y + l.b.x + l.b.y;
}
export fn birnam_test_makeLine(ax: c_longlong, ay: c_longlong, bx: c_longlong, by: c_longlong) callconv(.c) Line {
    return .{ .a = .{ .x = ax, .y = ay }, .b = .{ .x = bx, .y = by } };
}
const Arr3 = extern struct { v: [3]c_int }; // fixed array field
export fn birnam_test_sumArr3(arr: Arr3) callconv(.c) c_int {
    return arr.v[0] + arr.v[1] + arr.v[2];
}

test "parseSig: aggregate arg + scalar ret, natural-alignment layout" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var sig = try ffi.parseSig(a, "{id})i");
    defer sig.deinit(a);

    try testing.expectEqual(@as(usize, 1), sig.args.len);
    try testing.expectEqual(ffi.Kind.aggregate, sig.args[0]);
    try testing.expectEqual(ffi.Kind.int, sig.ret);
    const ag = sig.arg_aggrs[0].?;
    try testing.expectEqual(@as(usize, 2), ag.fields.len);
    try testing.expectEqual(ffi.Kind.int, ag.fields[0]);
    try testing.expectEqual(ffi.Kind.double, ag.fields[1]);
    // int@0, double padded to 8 → @8, total size 16.
    try testing.expectEqual(@as(u32, 0), ag.offsets[0]);
    try testing.expectEqual(@as(u32, 8), ag.offsets[1]);
    try testing.expectEqual(@as(usize, 16), ag.size);
}

test "invoke: HFA struct of doubles by value, arg + return (19.1, arm64 V-regs)" {
    // The classification dyncall 1.4 could not do on arm64-apple; libffi handles
    // it. A {dd} arg goes in v0/v1; a {dd} return comes back in v0/v1.
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    // HFA arg: sumVec2({1.5, 2.25}) = 3.75.
    var sig_arg = try ffi.parseSig(a, "{dd})d");
    defer sig_arg.deinit(a);
    var v = Vec2{ .x = 1.5, .y = 2.25 };
    const v_ptr = try boxed.mkFfiPtr(a, @intFromPtr(&v));
    const sum = ffi.invoke(a, @intFromPtr(&birnam_test_sumVec2), sig_arg, &[_]Ref{v_ptr});
    try testing.expect(boxed.isFloat(sum));
    try testing.expectEqual(@as(f64, 3.75), boxed.untagFloat(sum));

    // HFA return: makeVec2(8.5, 0.5) → {8.5, 0.5}.
    var sig_ret = try ffi.parseSig(a, "dd){dd}");
    defer sig_ret.deinit(a);
    const r = ffi.invoke(a, @intFromPtr(&birnam_test_makeVec2), sig_ret, &[_]Ref{ try boxed.mkFloat(a, 8.5), try boxed.mkFloat(a, 0.5) });
    try testing.expect(boxed.isFfiPtr(r));
    const got: *const Vec2 = @ptrFromInt(boxed.untagFfiPtr(r));
    try testing.expectEqual(@as(f64, 8.5), got.x);
    try testing.expectEqual(@as(f64, 0.5), got.y);
    std.c.free(@ptrFromInt(boxed.untagFfiPtr(r)));

    ffi.shutdown();
}

test "invoke: struct passed by value (sumPoint)" {
    if (!ffi.aggregates_supported) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var sig = try ffi.parseSig(a, "{ll})L");
    defer sig.deinit(a);

    var p = Point{ .x = 30, .y = 12 };
    const p_ptr = try boxed.mkFfiPtr(a, @intFromPtr(&p));
    const r = ffi.invoke(a, @intFromPtr(&birnam_test_sumPoint), sig, &[_]Ref{p_ptr});
    try testing.expect(ref_mod.isSmallInt(r));
    try testing.expectEqual(@as(i64, 42), ref_mod.untagSmallInt(r));

    ffi.shutdown();
}

test "invoke: struct returned by value (makePoint)" {
    if (!ffi.aggregates_supported) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var sig = try ffi.parseSig(a, "LL){ll}");
    defer sig.deinit(a);

    const r = ffi.invoke(a, @intFromPtr(&birnam_test_makePoint), sig, &[_]Ref{ ref_mod.mkSmallInt(7).?, ref_mod.mkSmallInt(35).? });
    try testing.expect(boxed.isFfiPtr(r));
    const got: *const Point = @ptrFromInt(boxed.untagFfiPtr(r));
    try testing.expectEqual(@as(c_longlong, 7), got.x);
    try testing.expectEqual(@as(c_longlong, 35), got.y);
    std.c.free(@ptrFromInt(boxed.untagFfiPtr(r)));

    ffi.shutdown();
}

test "invoke: libc div returns div_t struct by value" {
    if (!is_darwin or !ffi.aggregates_supported) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var lib = dlopenSelf() orelse return error.SkipZigTest;
    defer lib.close();
    const addr: usize = @intFromPtr(lib.lookup(*anyopaque, "div") orelse return error.SkipZigTest);

    // div_t div(int numer, int denom) → {int quot; int rem;}
    var sig = try ffi.parseSig(a, "ii){ii}");
    defer sig.deinit(a);

    const r = ffi.invoke(a, addr, sig, &[_]Ref{ ref_mod.mkSmallInt(17).?, ref_mod.mkSmallInt(5).? });
    try testing.expect(boxed.isFfiPtr(r));
    const got: *const DivT = @ptrFromInt(boxed.untagFfiPtr(r));
    try testing.expectEqual(@as(c_int, 3), got.quot);
    try testing.expectEqual(@as(c_int, 2), got.rem);
    std.c.free(@ptrFromInt(boxed.untagFfiPtr(r)));

    ffi.shutdown();
}

// -- Phase 19.5: nested + array struct fields -------------------------------

test "parseSig: nested struct field layout + ffi_type recursion" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    // {{LL}{LL}} — two nested {long,long} structs.
    var sig = try ffi.parseSig(a, "{{LL}{LL}})L");
    defer sig.deinit(a);
    const ag = sig.arg_aggrs[0].?;
    try testing.expectEqual(@as(usize, 2), ag.fields.len);
    try testing.expectEqual(ffi.Kind.aggregate, ag.fields[0]);
    try testing.expectEqual(ffi.Kind.aggregate, ag.fields[1]);
    try testing.expectEqual(@as(usize, 32), ag.size); // 2 × {8,8}
    try testing.expectEqual(@as(u32, 0), ag.offsets[0]);
    try testing.expectEqual(@as(u32, 16), ag.offsets[1]);
    // The nested aggrs are recorded and recurse into their own ffi_type.
    try testing.expect(ag.field_aggrs[0] != null);
    try testing.expectEqual(@as(usize, 16), ag.field_aggrs[0].?.size);
    try testing.expect(ag.ensureFfiType(a) != null);
}

test "parseSig: scalar array field flattens (3i == iii)" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var sig = try ffi.parseSig(a, "{3i})i");
    defer sig.deinit(a);
    const ag = sig.arg_aggrs[0].?;
    try testing.expectEqual(@as(usize, 3), ag.fields.len); // flattened to 3 ints
    try testing.expectEqual(@as(u32, 0), ag.offsets[0]);
    try testing.expectEqual(@as(u32, 4), ag.offsets[1]);
    try testing.expectEqual(@as(u32, 8), ag.offsets[2]);
    try testing.expectEqual(@as(usize, 12), ag.size);
}

test "parseSig: struct array (count before brace) is rejected" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    try testing.expectError(error.UnknownSigchar, ffi.parseSig(arena.allocator(), "{2{ii}})i"));
}

test "invoke: nested struct by value (sumLine), arg + return" {
    if (!ffi.aggregates_supported) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    // Arg: sumLine({{1,2},{3,4}}) = 10.
    var sig = try ffi.parseSig(a, "{{LL}{LL}})L");
    defer sig.deinit(a);
    var l = Line{ .a = .{ .x = 1, .y = 2 }, .b = .{ .x = 3, .y = 4 } };
    const l_ptr = try boxed.mkFfiPtr(a, @intFromPtr(&l));
    const r = ffi.invoke(a, @intFromPtr(&birnam_test_sumLine), sig, &[_]Ref{l_ptr});
    try testing.expectEqual(@as(i64, 10), ref_mod.untagSmallInt(r));

    // Return: makeLine(5,6,7,8) → {{5,6},{7,8}}.
    var sig2 = try ffi.parseSig(a, "LLLL){{LL}{LL}}");
    defer sig2.deinit(a);
    const r2 = ffi.invoke(a, @intFromPtr(&birnam_test_makeLine), sig2, &[_]Ref{
        ref_mod.mkSmallInt(5).?, ref_mod.mkSmallInt(6).?, ref_mod.mkSmallInt(7).?, ref_mod.mkSmallInt(8).?,
    });
    try testing.expect(boxed.isFfiPtr(r2));
    const got: *const Line = @ptrFromInt(boxed.untagFfiPtr(r2));
    try testing.expectEqual(@as(c_longlong, 5), got.a.x);
    try testing.expectEqual(@as(c_longlong, 8), got.b.y);
    std.c.free(@ptrFromInt(boxed.untagFfiPtr(r2)));
    ffi.shutdown();
}

test "invoke: array-field struct by value (sumArr3)" {
    if (!ffi.aggregates_supported) return error.SkipZigTest;
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var sig = try ffi.parseSig(a, "{3i})i");
    defer sig.deinit(a);
    var arr = Arr3{ .v = .{ 10, 20, 30 } };
    const arr_ptr = try boxed.mkFfiPtr(a, @intFromPtr(&arr));
    const r = ffi.invoke(a, @intFromPtr(&birnam_test_sumArr3), sig, &[_]Ref{arr_ptr});
    try testing.expectEqual(@as(i64, 60), ref_mod.untagSmallInt(r));
    ffi.shutdown();
}
