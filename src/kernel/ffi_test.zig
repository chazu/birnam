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
