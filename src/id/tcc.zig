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
//!   [unit install: cFnName as: #sel on: vt] -> install the named C function
//!                                              (Method ABI) as a method on vt
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

/// A relocated translation unit. The native code lives in the TCCState's memory,
/// so the state is kept alive for as long as the unit (never deleted on success).
const Unit = struct { state: *c.TCCState };
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
    unitOf(u).* = .{ .state = s };
    return u;
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
