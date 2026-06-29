//! src/id/tcc.zig — rung 3c (part 1): the libtcc compiler as an id module.
//!
//! The proximate goal's backend: compile C source to native code in memory and
//! hand back callable symbols. The compiler is privileged host infrastructure,
//! so it **statically links the vendored libtcc** and calls it from Zig (a
//! general C-FFI / "alien" module is a separate, later rung — see `docs/35`).
//! No general FFI is needed here because compiled birnam uses birnam's OWN
//! Method ABI: a `tcc_get_symbol` result is cast to the known `Method` type and
//! installed via `api.method`.
//!
//! Message api (driven through `api.send`, like every rung):
//!   [Tcc compile: cSourceString]            -> a TccUnit (compiled+relocated) or nil
//!                                              (the all-in-one; no runtime symbols)
//!   [unit install: cFnName as: #sel on: vt] -> install the named C function
//!                                              (Method ABI) as a method on vt
//!
//! The all-in-one `compile:` is fine when the emitted C is self-contained (the
//! 3c.2 arithmetic MVP). But real codegen (3c.3) emits C that calls **back into
//! libid** — to construct values, intern selectors, and `send` — so those names
//! are unresolved externals that `tcc_add_symbol` must patch BEFORE `tcc_relocate`
//! (after relocate the code is frozen). `compile:` does new+compile+relocate in
//! one shot, leaving no such window. So there is also a STAGED path that opens it:
//!   [Tcc new]                         -> a fresh, uncompiled TccUnit
//!   [unit compile: cSourceString]     -> compile into it (repeatable); unit or nil
//!   [unit bindRuntime]                -> register the standard libid runtime shims
//!                                        (birnam_send / birnam_mkSmallInt / …)
//!   [unit addSymbol: cName value: ad] -> register an arbitrary symbol -> address
//!                                        (ad = a SmallInt holding the pointer)
//!   [unit relocate]                   -> finalise (output to memory); unit or nil
//! `install:as:on:` / `symbol:` are valid only after `relocate`.
//!
//! Depends only on the IdApi + libtcc.

const std = @import("std");
const idapi = @import("idapi.zig");
const c = @cImport({
    @cInclude("libtcc.h");
});

const Oop = idapi.Oop;
const ca = std.heap.c_allocator;

var g_api: *idapi.IdApi = undefined;

pub var tcc_vt: Oop = null; // the Tcc facade's vtable
pub var tcc_obj: Oop = null; // the Tcc facade singleton (exported "Tcc")
pub var unit_vt: Oop = null; // a compiled translation unit

/// A translation unit. The native code lives in the TCCState's memory, so the
/// state is kept alive for as long as the unit (never deleted on success).
/// `relocated` guards the one-way transition: symbols may be added only before
/// it, symbols may be resolved only after it.
const Unit = struct { state: *c.TCCState, relocated: bool = false };
fn unitOf(o: Oop) *Unit {
    return @ptrCast(@alignCast(o.?));
}

/// Copy a libid String's bytes into a NUL-terminated C string (caller frees).
fn dupZ(o: Oop) [:0]u8 {
    var len: usize = 0;
    const p = g_api.stringBytes(o, &len);
    const z = ca.allocSentinel(u8, len, 0) catch @panic("tcc: oom");
    @memcpy(z[0..len], p[0..len]);
    return z;
}

/// Swallow libtcc's diagnostics for now (the message api reports failure as nil;
/// a `lastError` accessor / capturing handler can expose them when codegen wants
/// them). Without this, a failed compile spams stderr.
fn errSilent(ud: ?*anyopaque, msg: [*c]const u8) callconv(.c) void {
    _ = ud;
    _ = msg;
}

// -- messages ----------------------------------------------------------------

/// `[Tcc compile: cSource]` — new + compile + relocate (output in memory).
fn mCompile(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const s = c.tcc_new() orelse return g_api.nilObj;
    c.tcc_set_error_func(s, null, &errSilent);
    _ = c.tcc_set_output_type(s, c.TCC_OUTPUT_MEMORY);

    const src = dupZ(a[0]);
    defer ca.free(src);
    if (c.tcc_compile_string(s, src.ptr) == -1) {
        c.tcc_delete(s);
        return g_api.nilObj;
    }
    if (c.tcc_relocate(s) < 0) {
        c.tcc_delete(s);
        return g_api.nilObj;
    }
    const u = g_api.alloc(unit_vt, @sizeOf(Unit));
    unitOf(u).* = .{ .state = s, .relocated = true };
    return u;
}

// -- the libid runtime, as flat C-ABI shims for emitted code -----------------
//
// Emitted C calls libid through these `birnam_*` entry points rather than the
// IdApi struct directly, so the generated source never has to spell out the
// (append-only, evolving) struct layout — only these stable signatures. Each
// shim just forwards to the live `g_api`. `bindRuntime` registers their
// addresses under these names just before `relocate`.

fn rtSend(rcv: Oop, sel: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    return g_api.send(rcv, sel, args, n);
}
fn rtIntern(name: [*:0]const u8) callconv(.c) Oop {
    return g_api.intern(name);
}
fn rtMkSmallInt(v: i64) callconv(.c) Oop {
    return g_api.mkSmallInt(v);
}
fn rtSmallIntValue(o: Oop) callconv(.c) i64 {
    return g_api.smallIntValue(o);
}
fn rtMkString(p: [*]const u8, n: usize) callconv(.c) Oop {
    return g_api.mkString(p, n);
}
fn rtNil() callconv(.c) Oop {
    return g_api.nilObj;
}
fn rtTrue() callconv(.c) Oop {
    return g_api.trueObj;
}
fn rtFalse() callconv(.c) Oop {
    return g_api.falseObj;
}

// Object-model construction — for emitted closures: build a block's vtable
// (proto + method install) and allocate a capturing instance.
fn rtObjectVt() callconv(.c) Oop {
    return g_api.object_vt;
}
fn rtProto(parent: Oop) callconv(.c) Oop {
    return g_api.proto(parent);
}
fn rtMethod(vt: Oop, sel: Oop, fnptr: ?*anyopaque) callconv(.c) void {
    g_api.method(vt, sel, @ptrCast(@alignCast(fnptr)));
}
fn rtAlloc(vt: Oop, nbytes: i64) callconv(.c) Oop {
    return g_api.alloc(vt, @intCast(nbytes));
}

const RuntimeSym = struct { name: [*:0]const u8, addr: *const anyopaque };
const runtime_syms = [_]RuntimeSym{
    .{ .name = "birnam_send", .addr = @ptrCast(&rtSend) },
    .{ .name = "birnam_intern", .addr = @ptrCast(&rtIntern) },
    .{ .name = "birnam_mkSmallInt", .addr = @ptrCast(&rtMkSmallInt) },
    .{ .name = "birnam_smallIntValue", .addr = @ptrCast(&rtSmallIntValue) },
    .{ .name = "birnam_mkString", .addr = @ptrCast(&rtMkString) },
    .{ .name = "birnam_nil", .addr = @ptrCast(&rtNil) },
    .{ .name = "birnam_true", .addr = @ptrCast(&rtTrue) },
    .{ .name = "birnam_false", .addr = @ptrCast(&rtFalse) },
    .{ .name = "birnam_objectVt", .addr = @ptrCast(&rtObjectVt) },
    .{ .name = "birnam_proto", .addr = @ptrCast(&rtProto) },
    .{ .name = "birnam_method", .addr = @ptrCast(&rtMethod) },
    .{ .name = "birnam_alloc", .addr = @ptrCast(&rtAlloc) },
};

// -- staged compile: open the pre-relocate symbol window ---------------------

/// `[Tcc new]` — a fresh, uncompiled unit set to output to memory. Caller drives
/// it through `compile:`, optional `bindRuntime`/`addSymbol:value:`, `relocate`.
fn mNew(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    const s = c.tcc_new() orelse return g_api.nilObj;
    c.tcc_set_error_func(s, null, &errSilent);
    _ = c.tcc_set_output_type(s, c.TCC_OUTPUT_MEMORY);
    const u = g_api.alloc(unit_vt, @sizeOf(Unit));
    unitOf(u).* = .{ .state = s, .relocated = false };
    return u;
}

/// `[unit compile: cSource]` — add a translation unit. Repeatable before
/// relocate. Returns the unit, or nil on a compile error.
fn mCompileInto(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const src = dupZ(a[0]);
    defer ca.free(src);
    if (c.tcc_compile_string(unitOf(self).state, src.ptr) == -1) return g_api.nilObj;
    return self;
}

/// `[unit bindRuntime]` — register every `birnam_*` runtime shim so emitted C
/// can call back into libid. Must precede `relocate`. Returns the unit.
fn mBindRuntime(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const s = unitOf(self).state;
    for (runtime_syms) |rs| _ = c.tcc_add_symbol(s, rs.name, rs.addr);
    return self;
}

/// `[unit addSymbol: cName value: addrInt]` — patch an arbitrary external name
/// to a raw address (a SmallInt holding the pointer). The general escape hatch
/// alongside `bindRuntime`. Must precede `relocate`. Returns the unit.
fn mAddSymbol(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const name = dupZ(a[0]);
    defer ca.free(name);
    const addr: usize = @bitCast(g_api.smallIntValue(a[1]));
    _ = c.tcc_add_symbol(unitOf(self).state, name.ptr, @ptrFromInt(addr));
    return self;
}

/// `[unit relocate]` — finalise: copy code to executable memory and patch every
/// symbol. After this `install:`/`symbol:` work and nothing more may be added.
/// Returns the unit, or nil on a relocation error. Idempotent.
fn mRelocate(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const u = unitOf(self);
    if (u.relocated) return self;
    if (c.tcc_relocate(u.state) < 0) return g_api.nilObj;
    u.relocated = true;
    return self;
}

/// `[unit install: cFnName as: #sel on: vt]` — resolve the named function (which
/// must have the Method ABI) and bind it as a method on `vt`. Returns vt, or nil
/// if the symbol is absent.
fn mInstall(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const name = dupZ(a[0]);
    defer ca.free(name);
    const sym = c.tcc_get_symbol(unitOf(self).state, name.ptr) orelse return g_api.nilObj;
    const m: idapi.Method = @ptrCast(@alignCast(sym));
    g_api.method(a[2], a[1], m); // method(vt, selector, impl)
    return a[2];
}

/// `[unit symbol: cName]` — the raw native address of a symbol, as a SmallInt
/// (or nil). For codegen to grab function/data pointers it will marshal itself.
fn mSymbol(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const name = dupZ(a[0]);
    defer ca.free(name);
    const sym = c.tcc_get_symbol(unitOf(self).state, name.ptr) orelse return g_api.nilObj;
    return g_api.mkSmallInt(@bitCast(@intFromPtr(sym)));
}

// -- bringup / module entrypoint --------------------------------------------

pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    const reloading = api.isExported("Tcc") != 0;
    if (reloading) {
        tcc_vt = api.import("TccVt");
        unit_vt = api.import("TccUnitVt");
        tcc_obj = api.import("Tcc");
    } else {
        tcc_vt = api.proto(api.object_vt);
        unit_vt = api.proto(api.object_vt);
        tcc_obj = api.alloc(tcc_vt, 0);
    }

    api.method(tcc_vt, api.intern("compile:"), &mCompile);
    api.method(tcc_vt, api.intern("new"), &mNew);
    api.method(unit_vt, api.intern("compile:"), &mCompileInto);
    api.method(unit_vt, api.intern("bindRuntime"), &mBindRuntime);
    api.method(unit_vt, api.intern("addSymbol:value:"), &mAddSymbol);
    api.method(unit_vt, api.intern("relocate"), &mRelocate);
    api.method(unit_vt, api.intern("install:as:on:"), &mInstall);
    api.method(unit_vt, api.intern("symbol:"), &mSymbol);

    if (!reloading) {
        _ = api.exportObj("TccVt", tcc_vt);
        _ = api.exportObj("TccUnitVt", unit_vt);
        _ = api.exportObj("Tcc", tcc_obj);
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
