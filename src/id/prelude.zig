//! src/id/prelude.zig — the primitive standard library.
//!
//! The libid floor is deliberately bare: SmallInt has no `mod:`, booleans have no
//! `ifTrue:ifFalse:`, there is no I/O. This module installs the irreducible
//! PRIMITIVE protocol that birnam programs assume — the parts that can't be
//! written in birnam itself because they need raw arithmetic/identity/formatting/
//! syscalls. Everything else (control idioms, collections) can be defined in
//! birnam on top of this via `Reflect defineMethod:…`.
//!
//! Installed (via the IdApi, like the floor — no store/mirror needed):
//!   Integer (smallint_vt): mod: = < <= > >= + - * /  printString  to:do:
//!   Boolean (bool_vt):     ifTrue:ifFalse: ifTrue: ifFalse: & | not
//!   System (a singleton):  print:  println:
//! Exports `System` and `Boolean` into the id environment so birnam can name them.

const std = @import("std");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;

var g_api: *idapi.IdApi = undefined;

pub var system_vt: Oop = null;
pub var system_obj: Oop = null;

// -- helpers -----------------------------------------------------------------

fn iv(o: Oop) i64 {
    return g_api.smallIntValue(o);
}
fn mki(v: i64) Oop {
    return g_api.mkSmallInt(v);
}
fn boolean(b: bool) Oop {
    return if (b) g_api.trueObj else g_api.falseObj;
}
fn send0(rcv: Oop, sel: [*:0]const u8) Oop {
    return g_api.send(rcv, g_api.intern(sel), &[_]Oop{}, 0);
}
fn send1(rcv: Oop, sel: [*:0]const u8, a: Oop) Oop {
    return g_api.send(rcv, g_api.intern(sel), &[_]Oop{a}, 1);
}

// -- Integer (smallint_vt) ---------------------------------------------------

fn iAdd(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return mki(iv(self) + iv(a[0]));
}
fn iSub(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return mki(iv(self) - iv(a[0]));
}
fn iMul(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return mki(iv(self) * iv(a[0]));
}
fn iDiv(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    if (iv(a[0]) == 0) return zeroDivide();
    return mki(@divTrunc(iv(self), iv(a[0])));
}
fn iMod(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    if (iv(a[0]) == 0) return zeroDivide();
    return mki(@mod(iv(self), iv(a[0])));
}

/// A `#zeroDivide` error if the error model (F7) is loaded, else nil. Imported
/// LAZILY (at the failure, not at bringUp) so the prelude keeps no load-order
/// dependency on the error module — arithmetic lives here, the error TYPE lives
/// there, and they meet only when a division actually fails.
fn zeroDivide() Oop {
    const E = g_api.import("Error");
    if (E == null) return g_api.nilObj;
    const m = "division by zero";
    return g_api.send(E, g_api.intern("kind:message:"), &[_]Oop{ g_api.intern("zeroDivide"), g_api.mkString(m.ptr, m.len) }, 2);
}
fn iEq(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return boolean(iv(self) == iv(a[0]));
}
fn iLt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return boolean(iv(self) < iv(a[0]));
}
fn iLe(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return boolean(iv(self) <= iv(a[0]));
}
fn iGt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return boolean(iv(self) > iv(a[0]));
}
fn iGe(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return boolean(iv(self) >= iv(a[0]));
}
fn iPrintString(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    var buf: [24]u8 = undefined;
    const s = std.fmt.bufPrint(&buf, "{d}", .{iv(self)}) catch return g_api.nilObj;
    return g_api.mkString(s.ptr, s.len);
}

/// `[start to: end do: aBlock]` — invoke `[aBlock value: i]` for i in start..end
/// (inclusive). Returns the receiver.
fn iToDo(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const end = iv(a[0]);
    const blk = a[1];
    var i = iv(self);
    while (i <= end) : (i += 1) _ = send1(blk, "value:", mki(i));
    return self;
}

// -- Boolean (bool_vt; true/false share it, so branch on identity) -----------

fn bIfTrueIfFalse(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return send0(if (self == g_api.trueObj) a[0] else a[1], "value");
}
fn bIfTrue(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return if (self == g_api.trueObj) send0(a[0], "value") else g_api.nilObj;
}
fn bIfFalse(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return if (self == g_api.trueObj) g_api.nilObj else send0(a[0], "value");
}
fn bAnd(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n; // eager AND: the argument is already evaluated (a boolean)
    return if (self == g_api.trueObj) a[0] else g_api.falseObj;
}
fn bOr(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return if (self == g_api.trueObj) g_api.trueObj else a[0];
}
fn bNot(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return boolean(self != g_api.trueObj);
}

// -- System (I/O) ------------------------------------------------------------

fn writeString(s: Oop) void {
    var len: usize = 0;
    const p = g_api.stringBytes(s, &len);
    std.fs.File.stdout().writeAll(p[0..len]) catch {};
}
fn sysPrint(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    writeString(a[0]);
    return self;
}
fn sysPrintln(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    writeString(a[0]);
    std.fs.File.stdout().writeAll("\n") catch {};
    return self;
}

/// `[System gc]` → run a collection; answers the number of objects reclaimed (a
/// no-op returning 0 until the runtime sets a stack bottom — see main.zig).
fn sysGc(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return g_api.mkSmallInt(@intCast(g_api.gcCollect()));
}

/// `[System liveObjects]` → the number of tracked (live) heap objects right now.
fn sysLiveObjects(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return g_api.mkSmallInt(@intCast(g_api.gcLiveCount()));
}

// -- bringup -----------------------------------------------------------------

pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    const int_vt = api.smallint_vt;
    api.method(int_vt, api.intern("+"), &iAdd);
    api.method(int_vt, api.intern("-"), &iSub);
    api.method(int_vt, api.intern("*"), &iMul);
    api.method(int_vt, api.intern("/"), &iDiv);
    api.method(int_vt, api.intern("mod:"), &iMod);
    api.method(int_vt, api.intern("="), &iEq);
    api.method(int_vt, api.intern("<"), &iLt);
    api.method(int_vt, api.intern("<="), &iLe);
    api.method(int_vt, api.intern(">"), &iGt);
    api.method(int_vt, api.intern(">="), &iGe);
    api.method(int_vt, api.intern("printString"), &iPrintString);
    api.method(int_vt, api.intern("to:do:"), &iToDo);

    const b_vt = api.vtableOf(api.trueObj);
    api.method(b_vt, api.intern("ifTrue:ifFalse:"), &bIfTrueIfFalse);
    api.method(b_vt, api.intern("ifTrue:"), &bIfTrue);
    api.method(b_vt, api.intern("ifFalse:"), &bIfFalse);
    api.method(b_vt, api.intern("&"), &bAnd);
    api.method(b_vt, api.intern("|"), &bOr);
    api.method(b_vt, api.intern("not"), &bNot);

    const reloading = api.isExported("System") != 0;
    if (reloading) {
        system_vt = api.import("SystemVt");
        system_obj = api.import("System");
    } else {
        system_vt = api.proto(api.object_vt);
        system_obj = api.alloc(system_vt, 0);
    }
    api.method(system_vt, api.intern("print:"), &sysPrint);
    api.method(system_vt, api.intern("println:"), &sysPrintln);
    api.method(system_vt, api.intern("gc"), &sysGc);
    api.method(system_vt, api.intern("liveObjects"), &sysLiveObjects);

    if (!reloading) {
        _ = api.exportObj("SystemVt", system_vt);
        _ = api.exportObj("System", system_obj);
        _ = api.exportObj("Boolean", b_vt);
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
