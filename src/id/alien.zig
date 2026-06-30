//! src/id/alien.zig — the FFI: wrap and absorb C libraries.
//!
//! birnam's compiler IS a C compiler (libtcc), so the FFI needs no separate
//! marshaling engine like libffi: it GENERATES a tiny C shim that unpacks birnam
//! arguments, calls the real C function, and packs the result back — then compiles
//! that shim with `Tcc` and installs it as a method. tcc resolves the C function
//! against the running process (libc and anything `dlopen`'d), and `addLibrary:`
//! links a named library when needed.
//!
//! Message api (driven through `api.send`):
//!   [Alien dlopen: pathString]                       load a shared library so its
//!                                                    symbols become resolvable
//!   [Alien wrap: cFnName signature: sig as: #sel on: vt]
//!                                                    wrap a C function as a method
//!
//! A SIGNATURE is a string: the first char is the RETURN type, the rest are the
//! ARGUMENT types, one per birnam argument:
//!   i  int   (32-bit; a SmallInt)     s  const char* (a String; freed after call)
//!   l  long  (64-bit; a SmallInt)     p  void* (a pointer, boxed in a SmallInt)
//!   v  void  (return only; → nil)
//! e.g. "i i" = `int f(int)` (abs); "l s" = `long f(const char*)` (strlen, size_t);
//! "s s" = `char* f(const char*)` (getenv). Doubles/structs are out of scope until
//! a Float/struct type lands (a pointer covers opaque handles meanwhile).
//!
//! The RECEIVER is the FIRST C argument (the Smalltalk-natural idiom), so a
//! K-argument function becomes a method of arity K-1: `["hi" clen]` calls
//! `strlen("hi")` and `[-42 absVia]` calls `abs(-42)`. Wrap on the type whose
//! instances are that first argument (String for strlen, Integer for abs).
//!
//! The wrapped function's address is resolved by the process dynamic linker
//! (`dlsym`) and patched into the unit before relocation — the same mechanism that
//! binds the `birnam_*` shims — so resolution is reliable across platforms. For a
//! library not yet in the process, `[Alien dlopen: path]` loads it first.
//!
//! Depends on the IdApi + Tcc. The shim calls the standard `birnam_*` runtime shims
//! (mkSmallInt / smallIntValue / mkString / nil / cstr / free_cstr) that
//! `bindRuntime` provides, so no struct layout is ever spelled out.

const std = @import("std");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ca = std.heap.c_allocator;
const RTLD_LAZY: c_int = 1;

var g_api: *idapi.IdApi = undefined;
var tcc: Oop = undefined;

pub var alien_vt: Oop = null;
pub var alien_obj: Oop = null;

fn s0(rcv: Oop, sel: [*:0]const u8) Oop {
    return g_api.send(rcv, g_api.intern(sel), &[_]Oop{}, 0);
}
fn s1(rcv: Oop, sel: [*:0]const u8, a: Oop) Oop {
    return g_api.send(rcv, g_api.intern(sel), &[_]Oop{a}, 1);
}

fn strBytes(o: Oop) []const u8 {
    var len: usize = 0;
    const p = g_api.stringBytes(o, &len);
    return p[0..len];
}
fn dupZ(o: Oop) [:0]u8 {
    const b = strBytes(o);
    const z = ca.allocSentinel(u8, b.len, 0) catch @panic("alien: oom");
    @memcpy(z[0..b.len], b);
    return z;
}

/// The C type a signature char denotes.
fn cType(sc: u8) ?[]const u8 {
    return switch (sc) {
        'i' => "int",
        'l' => "long",
        's' => "const char*",
        'p' => "void*",
        'v' => "void",
        else => null,
    };
}

/// A despaced copy of `sig` (signatures may be written with spaces for readability,
/// e.g. "l s"). Caller owns it.
fn despace(sig: []const u8) []u8 {
    var b: std.ArrayList(u8) = .{};
    for (sig) |c| {
        if (c != ' ' and c != '\t') b.append(ca, c) catch @panic("alien: oom");
    }
    return b.toOwnedSlice(ca) catch @panic("alien: oom");
}

/// Validate a (despaced) signature: non-empty, every char a known type, and `v`
/// only as the return (an argument can't be void).
fn validSig(sig: []const u8) bool {
    if (sig.len == 0) return false;
    if (cType(sig[0]) == null) return false;
    for (sig[1..]) |sc| {
        if (cType(sc) == null or sc == 'v') return false;
    }
    return true;
}

/// Generate the Method-ABI C shim wrapping `fn_name` per `sig`. Caller owns it.
fn genShim(fn_name: []const u8, sig: []const u8) []u8 {
    const ret = sig[0];
    const args = sig[1..];
    var b: std.ArrayList(u8) = .{};
    const w = struct {
        fn f(buf: *std.ArrayList(u8), s: []const u8) void {
            buf.appendSlice(ca, s) catch @panic("alien: oom");
        }
    }.f;

    // The runtime shims the marshaling needs, plus strlen for string returns.
    w(&b,
        \\extern void* birnam_mkSmallInt(long);
        \\extern long birnam_smallIntValue(void*);
        \\extern const char* birnam_cstr(void*);
        \\extern void birnam_free_cstr(const char*);
        \\extern void* birnam_mkString(const char*, long);
        \\extern void* birnam_nil(void);
        \\extern long strlen(const char*);
        \\
    );

    // The wrapped function's prototype.
    w(&b, cType(ret).?);
    w(&b, " ");
    w(&b, fn_name);
    w(&b, "(");
    if (args.len == 0) {
        w(&b, "void");
    } else {
        for (args, 0..) |sc, i| {
            if (i != 0) w(&b, ", ");
            w(&b, cType(sc).?);
        }
    }
    w(&b, ");\n");

    // The shim: unmarshal args, call, free temporaries, marshal the result.
    // The RECEIVER is the first C argument (the Smalltalk-natural idiom, e.g.
    // `["hi" clen]` = strlen("hi")); any further C args come from `args[]`. So a
    // K-argument C function is a method of arity K-1 (K=0 → unary, receiver unused).
    w(&b, "void* __shim(void* self, void** args, long n){ (void)self; (void)args; (void)n;\n");
    var tmp: [96]u8 = undefined;
    var srcbuf: [24]u8 = undefined;
    for (args, 0..) |sc, i| {
        const src = if (i == 0) "self" else (std.fmt.bufPrint(&srcbuf, "args[{d}]", .{i - 1}) catch unreachable);
        const line = switch (sc) {
            'i' => std.fmt.bufPrint(&tmp, "int a{d} = (int)birnam_smallIntValue({s});\n", .{ i, src }),
            'l' => std.fmt.bufPrint(&tmp, "long a{d} = birnam_smallIntValue({s});\n", .{ i, src }),
            's' => std.fmt.bufPrint(&tmp, "const char* a{d} = birnam_cstr({s});\n", .{ i, src }),
            'p' => std.fmt.bufPrint(&tmp, "void* a{d} = (void*)birnam_smallIntValue({s});\n", .{ i, src }),
            else => unreachable,
        } catch @panic("alien: oom");
        w(&b, line);
    }

    // The call.
    if (ret == 'v') {
        w(&b, fn_name);
    } else {
        w(&b, cType(ret).?);
        w(&b, " __r = ");
        w(&b, fn_name);
    }
    w(&b, "(");
    for (args, 0..) |_, i| {
        if (i != 0) w(&b, ", ");
        w(&b, (std.fmt.bufPrint(&tmp, "a{d}", .{i}) catch unreachable));
    }
    w(&b, ");\n");

    // Free any string temporaries (after the call has used them).
    for (args, 0..) |sc, i| {
        if (sc == 's') w(&b, std.fmt.bufPrint(&tmp, "birnam_free_cstr(a{d});\n", .{i}) catch unreachable);
    }

    // Marshal the result.
    switch (ret) {
        'i', 'l' => w(&b, "return birnam_mkSmallInt((long)__r);\n"),
        'p' => w(&b, "return birnam_mkSmallInt((long)__r);\n"),
        's' => w(&b, "return __r ? birnam_mkString(__r, strlen(__r)) : birnam_nil();\n"),
        'v' => w(&b, "return birnam_nil();\n"),
        else => unreachable,
    }
    w(&b, "}\n");
    return b.toOwnedSlice(ca) catch @panic("alien: oom");
}

// The process-wide symbol search handle (macOS/Linux RTLD_DEFAULT = -2): find a
// symbol in the main program or any loaded library.
const RTLD_DEFAULT: ?*anyopaque = @ptrFromInt(@as(usize, @bitCast(@as(isize, -2))));

/// Patch a unit's external symbol `name` to a raw address before relocation.
fn addSym(unit: Oop, name: []const u8, addr: usize) void {
    _ = g_api.send(unit, g_api.intern("addSymbol:value:"), &[_]Oop{ g_api.mkString(name.ptr, name.len), g_api.mkSmallInt(@bitCast(addr)) }, 2);
}

/// Compile + install a wrapper for the C function `fn_name`. Resolves the function
/// in the process (libc + anything dlopen'd) and patches its address into the unit,
/// so the emitted `extern`/call binds to the real symbol. Returns vt, or nil if the
/// signature is invalid, the symbol is not found, or compilation fails.
fn buildWrapper(fn_name: []const u8, sig_raw: []const u8, sel: Oop, vt: Oop) Oop {
    const sig = despace(sig_raw);
    defer ca.free(sig);
    if (!validSig(sig)) return g_api.nilObj;

    const fz = ca.allocSentinel(u8, fn_name.len, 0) catch @panic("alien: oom");
    defer ca.free(fz);
    @memcpy(fz[0..fn_name.len], fn_name);
    const addr = g_api.dlsym(RTLD_DEFAULT, fz.ptr) orelse return g_api.nilObj; // not found

    const shim = genShim(fn_name, sig);
    defer ca.free(shim);
    if (std.posix.getenv("ALIEN_DUMP") != null) std.debug.print("[alien] shim for {s}:\n{s}\n", .{ fn_name, shim });

    const unit = s0(tcc, "new");
    if (unit == g_api.nilObj) return g_api.nilObj;
    if (s1(unit, "compile:", g_api.mkString(shim.ptr, shim.len)) == g_api.nilObj) return g_api.nilObj;
    _ = s0(unit, "bindRuntime"); // the marshaling shims (mkSmallInt / cstr / …)
    addSym(unit, fn_name, @intFromPtr(addr)); // the wrapped function itself
    if (s0(unit, "relocate") == g_api.nilObj) return g_api.nilObj;

    const fname = "__shim";
    return g_api.send(unit, g_api.intern("install:as:on:"), &[_]Oop{ g_api.mkString(fname, fname.len), sel, vt }, 3);
}

/// `[Alien wrap: cFnName signature: sig as: #sel on: vt]`
fn mWrap(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return buildWrapper(strBytes(a[0]), strBytes(a[1]), a[2], a[3]);
}

/// `[Alien dlopen: path]` — load a shared library into the process so its symbols
/// become resolvable by later `wrap:`. Answers true, or nil if it cannot load.
fn mDlopen(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const path = dupZ(a[0]);
    defer ca.free(path);
    const h = g_api.dlopen(path.ptr, RTLD_LAZY);
    return if (h != null) g_api.trueObj else g_api.nilObj;
}

pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    tcc = api.import("Tcc") orelse @panic("alien: Tcc not loaded");

    const reloading = api.isExported("Alien") != 0;
    if (reloading) {
        alien_vt = api.import("AlienVt");
        alien_obj = api.import("Alien");
    } else {
        alien_vt = api.proto(api.object_vt);
        alien_obj = api.alloc(alien_vt, 0);
    }

    api.method(alien_vt, api.intern("wrap:signature:as:on:"), &mWrap);
    api.method(alien_vt, api.intern("dlopen:"), &mDlopen);

    if (!reloading) {
        _ = api.exportObj("AlienVt", alien_vt);
        _ = api.exportObj("Alien", alien_obj);
    }
}

fn moduleInit(api: *idapi.IdApi, state: ?*anyopaque) callconv(.c) ?*anyopaque {
    _ = state;
    bringUp(api);
    return null;
}
comptime {
    if (!@import("builtin").is_test) @export(&moduleInit, .{ .name = "module_init" });
}
