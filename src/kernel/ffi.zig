//! Phase 9: dyncall-backed foreign function dispatch.
//!
//! Two layers:
//!   1. Thin Zig extern decls for the dyncall C API.
//!   2. Sig-string parser + invoke(addr, sig, args) Ref marshaler.
//!
//! Sig strings follow dyncall convention (DCsigchar):
//!   B = bool, c = char, s = short, i = int, l = long, L = long long,
//!   f = float, d = double, p = pointer, Z = c-string (alias of p),
//!   v = void. `)` separates args from return type. `e` at the call site
//!   marks the start of variadic args.
//!
//! Phase 9 scope: pass-by-value scalars + pointer + c-string. Aggregates
//! (structs by value) deferred to 9.5+.

const std = @import("std");
const ref_mod = @import("ref.zig");
const boxed = @import("boxed.zig");

pub const Ref = ref_mod.Ref;

// -- dyncall C API extern decls --------------------------------------------

pub const DCCallVM = opaque {};

pub const DC_CALL_C_DEFAULT: c_int = 0;
pub const DC_CALL_C_ELLIPSIS: c_int = 100;
pub const DC_CALL_C_ELLIPSIS_VARARGS: c_int = 101;

pub extern "c" fn dcNewCallVM(size: usize) ?*DCCallVM;
pub extern "c" fn dcFree(vm: *DCCallVM) void;
pub extern "c" fn dcReset(vm: *DCCallVM) void;
pub extern "c" fn dcMode(vm: *DCCallVM, mode: c_int) void;

pub extern "c" fn dcArgBool(vm: *DCCallVM, v: c_int) void;
pub extern "c" fn dcArgChar(vm: *DCCallVM, v: u8) void;
pub extern "c" fn dcArgShort(vm: *DCCallVM, v: c_short) void;
pub extern "c" fn dcArgInt(vm: *DCCallVM, v: c_int) void;
pub extern "c" fn dcArgLong(vm: *DCCallVM, v: c_long) void;
pub extern "c" fn dcArgLongLong(vm: *DCCallVM, v: c_longlong) void;
pub extern "c" fn dcArgFloat(vm: *DCCallVM, v: f32) void;
pub extern "c" fn dcArgDouble(vm: *DCCallVM, v: f64) void;
pub extern "c" fn dcArgPointer(vm: *DCCallVM, v: ?*const anyopaque) void;

pub extern "c" fn dcCallVoid(vm: *DCCallVM, fnp: *const anyopaque) void;
pub extern "c" fn dcCallBool(vm: *DCCallVM, fnp: *const anyopaque) c_int;
pub extern "c" fn dcCallChar(vm: *DCCallVM, fnp: *const anyopaque) u8;
pub extern "c" fn dcCallShort(vm: *DCCallVM, fnp: *const anyopaque) c_short;
pub extern "c" fn dcCallInt(vm: *DCCallVM, fnp: *const anyopaque) c_int;
pub extern "c" fn dcCallLong(vm: *DCCallVM, fnp: *const anyopaque) c_long;
pub extern "c" fn dcCallLongLong(vm: *DCCallVM, fnp: *const anyopaque) c_longlong;
pub extern "c" fn dcCallFloat(vm: *DCCallVM, fnp: *const anyopaque) f32;
pub extern "c" fn dcCallDouble(vm: *DCCallVM, fnp: *const anyopaque) f64;
pub extern "c" fn dcCallPointer(vm: *DCCallVM, fnp: *const anyopaque) ?*anyopaque;

// -- Sig-string parser ------------------------------------------------------

pub const Kind = enum {
    void_,
    bool_,
    char,
    short,
    int,
    long,
    long_long,
    float,
    double,
    pointer,
    cstring,
};

pub const Sig = struct {
    args: []Kind,
    ret: Kind,
    /// Position in `args` (0-indexed) where variadic varargs begin. null
    /// means the signature is not variadic. Marker char in source: `e`.
    variadic_at: ?usize = null,

    pub fn deinit(self: *Sig, allocator: std.mem.Allocator) void {
        allocator.free(self.args);
    }
};

pub const SigParseError = error{
    UnknownSigchar,
    MissingReturnSeparator,
    EmptySig,
    OutOfMemory,
};

fn kindOf(c: u8) ?Kind {
    return switch (c) {
        'v' => .void_,
        'B' => .bool_,
        'c' => .char,
        's' => .short,
        'i' => .int,
        'l' => .long,
        'L' => .long_long,
        'f' => .float,
        'd' => .double,
        'p' => .pointer,
        'Z' => .cstring,
        else => null,
    };
}

pub fn parseSig(allocator: std.mem.Allocator, s: []const u8) SigParseError!Sig {
    if (s.len == 0) return error.EmptySig;
    var args = std.ArrayList(Kind){};
    defer args.deinit(allocator);
    var variadic_at: ?usize = null;
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        const c = s[i];
        if (c == ')') {
            if (i + 1 >= s.len) return error.MissingReturnSeparator;
            const ret_kind = kindOf(s[i + 1]) orelse return error.UnknownSigchar;
            const owned = try args.toOwnedSlice(allocator);
            return .{ .args = owned, .ret = ret_kind, .variadic_at = variadic_at };
        }
        if (c == 'e') {
            variadic_at = args.items.len;
            continue;
        }
        const k = kindOf(c) orelse return error.UnknownSigchar;
        try args.append(allocator, k);
    }
    return error.MissingReturnSeparator;
}

// -- VM lifecycle ----------------------------------------------------------

/// Default VM stack size — generous enough for the Phase 9 test plan.
pub const DEFAULT_VM_SIZE: usize = 4096;

/// Single kernel-global VM pointer. dyncall is single-threaded; single VM
/// is enough until concurrency lands.
var g_vm: ?*DCCallVM = null;

pub fn ensureVm() ?*DCCallVM {
    if (g_vm) |v| return v;
    g_vm = dcNewCallVM(DEFAULT_VM_SIZE);
    return g_vm;
}

pub fn shutdown() void {
    if (g_vm) |v| {
        dcFree(v);
        g_vm = null;
    }
}

// -- Ref → dyncall arg marshaling -----------------------------------------

/// Push `value` into the VM as `kind`. Returns false on type mismatch so the
/// caller can abort + return NIL.
fn pushArg(vm: *DCCallVM, kind: Kind, value: Ref) bool {
    switch (kind) {
        .void_ => return false,
        .bool_ => {
            if (!ref_mod.isSmallInt(value)) return false;
            dcArgBool(vm, @intCast(@as(c_int, @intCast(ref_mod.untagSmallInt(value)))));
        },
        .char => {
            if (!ref_mod.isSmallInt(value)) return false;
            dcArgChar(vm, @intCast(@as(u8, @truncate(@as(u64, @bitCast(ref_mod.untagSmallInt(value)))))));
        },
        .short => {
            if (!ref_mod.isSmallInt(value)) return false;
            dcArgShort(vm, @intCast(ref_mod.untagSmallInt(value)));
        },
        .int => {
            if (!ref_mod.isSmallInt(value)) return false;
            dcArgInt(vm, @intCast(ref_mod.untagSmallInt(value)));
        },
        .long => {
            if (!ref_mod.isSmallInt(value)) return false;
            dcArgLong(vm, @intCast(ref_mod.untagSmallInt(value)));
        },
        .long_long => {
            if (!ref_mod.isSmallInt(value)) return false;
            dcArgLongLong(vm, @intCast(ref_mod.untagSmallInt(value)));
        },
        .float => {
            if (!boxed.isFloat(value)) return false;
            dcArgFloat(vm, @floatCast(boxed.untagFloat(value)));
        },
        .double => {
            if (!boxed.isFloat(value)) return false;
            dcArgDouble(vm, boxed.untagFloat(value));
        },
        .pointer, .cstring => {
            if (!boxed.isFfiPtr(value)) return false;
            const addr = boxed.untagFfiPtr(value);
            dcArgPointer(vm, @ptrFromInt(addr));
        },
    }
    return true;
}

/// Dispatch the appropriate dcCallX based on return kind. Returns the raw
/// C-side value boxed/tagged into a birnam Ref.
fn callAndWrap(allocator: std.mem.Allocator, vm: *DCCallVM, ret: Kind, addr: usize) Ref {
    const fnp: *const anyopaque = @ptrFromInt(addr);
    switch (ret) {
        .void_ => {
            dcCallVoid(vm, fnp);
            return ref_mod.NIL;
        },
        .bool_ => {
            const r = dcCallBool(vm, fnp);
            return ref_mod.mkSmallInt(@intCast(r)) orelse ref_mod.NIL;
        },
        .char => {
            const r = dcCallChar(vm, fnp);
            return ref_mod.mkSmallInt(@intCast(r)) orelse ref_mod.NIL;
        },
        .short => {
            const r = dcCallShort(vm, fnp);
            return ref_mod.mkSmallInt(@intCast(r)) orelse ref_mod.NIL;
        },
        .int => {
            const r = dcCallInt(vm, fnp);
            return ref_mod.mkSmallInt(@intCast(r)) orelse ref_mod.NIL;
        },
        .long => {
            const r = dcCallLong(vm, fnp);
            return ref_mod.mkSmallInt(@intCast(r)) orelse ref_mod.NIL;
        },
        .long_long => {
            const r = dcCallLongLong(vm, fnp);
            return ref_mod.mkSmallInt(@intCast(r)) orelse ref_mod.NIL;
        },
        .float => {
            const r = dcCallFloat(vm, fnp);
            return boxed.mkFloat(allocator, @floatCast(r)) catch ref_mod.NIL;
        },
        .double => {
            const r = dcCallDouble(vm, fnp);
            return boxed.mkFloat(allocator, r) catch ref_mod.NIL;
        },
        .pointer, .cstring => {
            const r = dcCallPointer(vm, fnp);
            const addr_ret: usize = @intFromPtr(r);
            return boxed.mkFfiPtr(allocator, addr_ret) catch ref_mod.NIL;
        },
    }
}

/// Top-level invocation. Resolves the kernel-global VM, configures variadic
/// mode if the sig is variadic, marshals args, calls, wraps return.
pub fn invoke(
    allocator: std.mem.Allocator,
    addr: usize,
    sig: Sig,
    args: []const Ref,
) Ref {
    if (args.len != sig.args.len) return ref_mod.NIL;
    const vm = ensureVm() orelse return ref_mod.NIL;
    dcReset(vm);
    const is_variadic = sig.variadic_at != null;
    if (is_variadic) dcMode(vm, DC_CALL_C_ELLIPSIS);
    for (sig.args, args, 0..) |kind, value, i| {
        if (is_variadic and i == sig.variadic_at.?) {
            dcMode(vm, DC_CALL_C_ELLIPSIS_VARARGS);
        }
        if (!pushArg(vm, kind, value)) return ref_mod.NIL;
    }
    return callAndWrap(allocator, vm, sig.ret, addr);
}
