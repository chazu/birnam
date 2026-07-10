//! src/id/error.zig — F7, the error model (rung 3). Read docs/34 §F7.
//!
//! **Errors-as-propagating-values** (the pragmatic floor). An Error is an ordinary
//! libid object whose `doesNotUnderstand:` returns *itself*, so it propagates
//! through any chain of sends with ZERO unwinding — `[[x foo] bar]` yields the
//! original error. This is exactly the model that fits the native-compiled send
//! path: codegen emits `birnam_send(birnam_send(x, foo…), bar…)`, and if the
//! inner send errored the outer one dispatches `bar` to the Error, which answers
//! itself. No handler stack, no longjmp, no codegen changes.
//!
//! Conditions & restarts (the docs/34 §F7 *aspirational* target — resumable,
//! handlers-high/restarts-low) need a handler stack this floor doesn't have yet;
//! they wait for a richer eval layer. This module is the floor that a condition
//! system would later subsume.
//!
//! The keystone is F2-alignment: the floor's default `doesNotUnderstand:` PANICS
//! (libid stays minimal, docs/34 left it deliberately *swappable*). Loading this
//! module SWAPS it on `object_vt` to mint a `#doesNotUnderstand` error instead —
//! so a send to `nil` (or any object) that misses now yields a catchable value,
//! not a crash.
//!
//! **This module is the CORE — the representation + the swap + the factory only.**
//! The error *protocol* (`on:do:` / `ensure:` / `ifError:` / `ifNotError:` /
//! `printString`, and `String` `error`/`error:`) is now SELF-HOSTED birnam in
//! `stdlib.birnam` — pure block orchestration over the primitives below (docs/44a).
//!
//! Installs:
//!   Error (error_vt)   doesNotUnderstand: → self (propagate); isError → true;
//!                        kind; message
//!   Error factory      kind:message:  kind:  message:   (exported "Error")
//!   Object (object_vt) isError → false; doesNotUnderstand: → a #doesNotUnderstand
//!                        error (THE nil-send fix)

const std = @import("std");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ca = std.heap.c_allocator;

var g_api: *idapi.IdApi = undefined;

pub var error_vt: Oop = null;
pub var factory_vt: Oop = null;
pub var factory_obj: Oop = null;

var s_wild: Oop = null; // "error" — the kind of a bare `[Error message:]` / `["x" error]`

/// An error's payload: a `kind` symbol (or nil) and a `message` string (or nil).
const Err = extern struct {
    kind: Oop,
    message: Oop,
};

fn err(o: Oop) *Err {
    return @ptrCast(@alignCast(o.?));
}
fn mkError(kind: Oop, message: Oop) Oop {
    const o = g_api.alloc(error_vt, @sizeOf(Err));
    err(o).* = .{ .kind = kind, .message = message };
    return o;
}

// -- Error protocol (core: propagation + accessors) --------------------------

/// The propagation heart: any unhandled message to an error answers the error.
fn errDnu(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return self;
}
fn errIsError(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return g_api.trueObj;
}
fn errKind(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return err(self).kind;
}
fn errMessage(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return err(self).message;
}

// -- the Error factory -------------------------------------------------------

fn facKindMessage(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return mkError(a[0], a[1]);
}
fn facKind(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return mkError(a[0], g_api.nilObj);
}
fn facMessage(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    return mkError(s_wild, a[0]);
}

// -- Object protocol (the swap) ----------------------------------------------

fn objIsError(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return g_api.falseObj;
}
/// THE SWAP: the new default `doesNotUnderstand:` — mint a propagating error
/// naming the missed selector, instead of the floor's panic. Aligns F2 with F7.
fn objDnu(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const sel = a[0]; // the original (unbound) selector symbol
    const name = std.mem.span(g_api.symbolName(sel));
    const msg = std.fmt.allocPrint(ca, "does not understand #{s}", .{name}) catch @panic("error: oom");
    defer ca.free(msg);
    return mkError(g_api.intern("doesNotUnderstand"), g_api.mkString(msg.ptr, msg.len));
}

// -- bringup -----------------------------------------------------------------

pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    s_wild = api.intern("error");

    const reloading = api.isExported("Error") != 0;
    if (reloading) {
        error_vt = api.import("ErrorVt");
        factory_vt = api.import("ErrorFactoryVt");
        factory_obj = api.import("Error");
    } else {
        error_vt = api.proto(api.object_vt);
        factory_vt = api.proto(api.object_vt);
        factory_obj = api.alloc(factory_vt, 0);
    }

    // Exact payload layout (F3): an Err is {kind, message} — both Oops.
    api.setLayout(error_vt, null, 0, idapi.slot_oop);

    // Error protocol — note its OWN doesNotUnderstand: (→ self) must shadow the
    // object_vt swap below, else propagating through an error would re-error.
    // `printString`/`ifError:`/`ifNotError:` are self-hosted on ErrorVt in
    // stdlib.birnam (docs/44a); only the propagation + accessors are here.
    api.method(error_vt, api.intern("doesNotUnderstand:"), &errDnu);
    api.method(error_vt, api.intern("isError"), &errIsError);
    api.method(error_vt, api.intern("kind"), &errKind);
    api.method(error_vt, api.intern("message"), &errMessage);

    api.method(factory_vt, api.intern("kind:message:"), &facKindMessage);
    api.method(factory_vt, api.intern("kind:"), &facKind);
    api.method(factory_vt, api.intern("message:"), &facMessage);

    // The swap on Object (inherited by everything, incl. nil). `isError`→false is
    // the base the ErrorVt override flips. `on:do:`/`ensure:`/`ifError:`/
    // `ifNotError:` are self-hosted on Object in stdlib.birnam (docs/44a).
    api.method(api.object_vt, api.intern("isError"), &objIsError);
    api.method(api.object_vt, api.intern("doesNotUnderstand:"), &objDnu);

    if (!reloading) {
        _ = api.exportObj("ErrorVt", error_vt);
        _ = api.exportObj("ErrorFactoryVt", factory_vt);
        _ = api.exportObj("Error", factory_obj);
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
