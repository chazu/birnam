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
//! Installs:
//!   Error (error_vt)   doesNotUnderstand: → self (propagate); isError → true;
//!                        kind; message; printString; ifError:; ifNotError:
//!   Error factory      kind:message:  kind:  message:   (exported "Error")
//!   Object (object_vt) isError → false; doesNotUnderstand: → a #doesNotUnderstand
//!                        error (THE nil-send fix); on:do:; ensure:; ifError:;
//!                        ifNotError:
//!   String (string_vt) error  → raise #error;  error: → raise a kinded error
//! `on: #error do:` is the catch-all wildcard; `on: #kind do:` matches one kind;
//! `on: #[#a #b] do:` (any collection answering `includes:`) matches a set of kinds.
//! `[protected ensure: cleanup]` always runs cleanup, then answers protected's result.

const std = @import("std");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ca = std.heap.c_allocator;

var g_api: *idapi.IdApi = undefined;

pub var error_vt: Oop = null;
pub var factory_vt: Oop = null;
pub var factory_obj: Oop = null;

var s_value: Oop = null; // "value" — run a 0-arg protected block
var s_value1: Oop = null; // "value:" — invoke a handler block
var s_wild: Oop = null; // "error" — the on:do: catch-all kind
var s_includes: Oop = null; // "includes:" — multi-kind on:do: membership

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
fn isErrorValue(o: Oop) bool {
    return g_api.vtableOf(o) == error_vt;
}

// -- Error protocol ----------------------------------------------------------

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
fn errPrintString(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const e = err(self);
    var buf: std.ArrayList(u8) = .{};
    defer buf.deinit(ca);
    buf.appendSlice(ca, "error(") catch @panic("error: oom");
    var had_kind = false;
    if (g_api.vtableOf(e.kind) == g_api.symbol_vt) {
        buf.append(ca, '#') catch @panic("error: oom");
        buf.appendSlice(ca, std.mem.span(g_api.symbolName(e.kind))) catch @panic("error: oom");
        had_kind = true;
    }
    if (g_api.vtableOf(e.message) == g_api.string_vt) {
        if (had_kind) buf.appendSlice(ca, ": ") catch @panic("error: oom");
        var len: usize = 0;
        const p = g_api.stringBytes(e.message, &len);
        buf.appendSlice(ca, p[0..len]) catch @panic("error: oom");
    }
    buf.append(ca, ')') catch @panic("error: oom");
    return g_api.mkString(buf.items.ptr, buf.items.len);
}
/// `[anError ifError: handler]` → run the handler with the error.
fn errIfError(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return g_api.send(a[0], s_value1, &[_]Oop{self}, 1);
}
/// `[anError ifNotError: handler]` → the error itself (handler not run).
fn errIfNotError(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return self;
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

// -- Object protocol (the swap + handling) -----------------------------------

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
/// Does a caught error's kind `k` satisfy the `want` spec? Three cases, in order:
/// the wildcard `#error` (catch anything); a single kind symbol (exact match); or
/// a *collection* of kinds — anything that answers `includes:` — so `#[#a #b]`
/// catches either. The collection case is pure dispatch (a doomed `includes:` to a
/// non-collection just fails to match), so error.zig needn't know the collection
/// module exists. A plain non-wildcard symbol short-circuits to exact match, so we
/// never mint a junk DNU error sending `includes:` to a symbol on the common path.
fn kindMatches(want: Oop, k: Oop) bool {
    if (want == s_wild or want == k) return true;
    if (g_api.vtableOf(want) == g_api.symbol_vt) return false;
    return g_api.send(want, s_includes, &[_]Oop{k}, 1) == g_api.trueObj;
}
/// `[[protectedBlock] on: spec do: handler]` — run the 0-arg block; if it yields
/// an error whose kind matches `spec` (a kind symbol, the `#error` wildcard, or a
/// collection of kinds), run the handler with it; otherwise pass the value (or
/// unmatched error) straight through.
fn objOnDo(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const want = a[0];
    const handler = a[1];
    const val = g_api.send(self, s_value, &[_]Oop{}, 0);
    if (!isErrorValue(val)) return val;
    if (kindMatches(want, err(val).kind)) return g_api.send(handler, s_value1, &[_]Oop{val}, 1);
    return val; // unmatched — keep propagating
}
/// `[protectedBlock ensure: cleanupBlock]` — run the protected block, then ALWAYS
/// run the cleanup block (for its side effects), and answer the protected result,
/// whether a value or a propagating error. In the value model an error is just a
/// value that flowed back — no stack unwound — so unwind-protect is exactly "run
/// cleanup on the way out, keep the original answer". (No handler stack needed.)
fn objEnsure(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const val = g_api.send(self, s_value, &[_]Oop{}, 0);
    _ = g_api.send(a[0], s_value, &[_]Oop{}, 0); // cleanup — result discarded
    return val;
}
/// On a non-error, `ifError:` is a no-op (answer self) and `ifNotError:` runs.
fn objIfError(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return self;
}
fn objIfNotError(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return g_api.send(a[0], s_value1, &[_]Oop{self}, 1);
}

// -- raising from a String message -------------------------------------------

fn strError(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return mkError(s_wild, self); // `["boom" error]` → kind #error
}
fn strErrorKind(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return mkError(a[0], self); // `["boom" error: #myKind]`
}

// -- bringup -----------------------------------------------------------------

pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    s_value = api.intern("value");
    s_value1 = api.intern("value:");
    s_wild = api.intern("error");
    s_includes = api.intern("includes:");

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
    api.method(error_vt, api.intern("doesNotUnderstand:"), &errDnu);
    api.method(error_vt, api.intern("isError"), &errIsError);
    api.method(error_vt, api.intern("kind"), &errKind);
    api.method(error_vt, api.intern("message"), &errMessage);
    api.method(error_vt, api.intern("printString"), &errPrintString);
    api.method(error_vt, api.intern("ifError:"), &errIfError);
    api.method(error_vt, api.intern("ifNotError:"), &errIfNotError);

    api.method(factory_vt, api.intern("kind:message:"), &facKindMessage);
    api.method(factory_vt, api.intern("kind:"), &facKind);
    api.method(factory_vt, api.intern("message:"), &facMessage);

    // The swap + handling on Object (inherited by everything, incl. nil).
    api.method(api.object_vt, api.intern("isError"), &objIsError);
    api.method(api.object_vt, api.intern("doesNotUnderstand:"), &objDnu);
    api.method(api.object_vt, api.intern("on:do:"), &objOnDo);
    api.method(api.object_vt, api.intern("ensure:"), &objEnsure);
    api.method(api.object_vt, api.intern("ifError:"), &objIfError);
    api.method(api.object_vt, api.intern("ifNotError:"), &objIfNotError);

    // Raising from a message string.
    api.method(api.string_vt, api.intern("error"), &strError);
    api.method(api.string_vt, api.intern("error:"), &strErrorKind);

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
