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
//!     (arithmetic is fault-tolerant: overflow/type/zero-divide → catchable errors)
//!   Boolean (bool_vt):     ifTrue:ifFalse: ifTrue: ifFalse: & | not  printString
//!   System (a singleton):  print:  println:  readFile:  writeFile:contents:
//!                          readLine  gc  liveObjects  autoGc:
//! Exports `System` and `Boolean` into the id environment so birnam can name them.

const std = @import("std");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ca = std.heap.c_allocator;

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

// SmallInt is a 63-bit signed fixnum (the floor's maru-style tag): a result must
// land in [-2^62, 2^62-1] or it has overflowed.
const SMALLINT_MAX: i128 = (1 << 62) - 1;
const SMALLINT_MIN: i128 = -(1 << 62);

fn isInt(o: Oop) bool {
    return g_api.isSmallInt(o) != 0;
}

// The Error vtable, imported lazily (null if the F7 module isn't loaded), so
// arithmetic can PROPAGATE an error operand rather than mask it as a type mismatch.
var g_error_vt: Oop = null;
fn isErr(o: Oop) bool {
    if (g_error_vt == null) g_error_vt = g_api.import("ErrorVt");
    return g_error_vt != null and o != null and g_api.vtableOf(o) == g_error_vt;
}

/// Mint an Error of `kind` (F7), or nil if the error model isn't loaded. Imported
/// LAZILY at the fault so the prelude keeps no load-order dependency on the error
/// module — arithmetic lives here, the error TYPE there, meeting only on a fault.
fn mkErr(kind: [*:0]const u8, msg: []const u8) Oop {
    const E = g_api.import("Error");
    if (E == null) return g_api.nilObj;
    return g_api.send(E, g_api.intern("kind:message:"), &[_]Oop{ g_api.intern(kind), g_api.mkString(msg.ptr, msg.len) }, 2);
}

/// Tag an arithmetic result, or a `#overflow` error if it left the SmallInt range.
/// (The op is computed in i128 first, so it can never wrap silently.)
fn intResult(v: i128) Oop {
    if (v < SMALLINT_MIN or v > SMALLINT_MAX) return mkErr("overflow", "integer overflow");
    return mki(@intCast(v));
}

/// Guard a binary-arithmetic argument: propagate an error operand, reject a
/// non-Integer with `#typeMismatch`, else null (it is a usable Integer).
fn argFault(o: Oop) ?Oop {
    if (isErr(o)) return o;
    if (!isInt(o)) return mkErr("typeMismatch", "Integer arithmetic needs an Integer");
    return null;
}

fn iAdd(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    if (argFault(a[0])) |f| return f;
    return intResult(@as(i128, iv(self)) + @as(i128, iv(a[0])));
}
fn iSub(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    if (argFault(a[0])) |f| return f;
    return intResult(@as(i128, iv(self)) - @as(i128, iv(a[0])));
}
fn iMul(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    if (argFault(a[0])) |f| return f;
    return intResult(@as(i128, iv(self)) * @as(i128, iv(a[0])));
}
fn iDiv(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    if (argFault(a[0])) |f| return f;
    if (iv(a[0]) == 0) return mkErr("zeroDivide", "division by zero");
    return intResult(@divTrunc(@as(i128, iv(self)), @as(i128, iv(a[0]))));
}
fn iMod(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    if (argFault(a[0])) |f| return f;
    if (iv(a[0]) == 0) return mkErr("zeroDivide", "division by zero");
    return intResult(@mod(@as(i128, iv(self)), @as(i128, iv(a[0]))));
}
fn iEq(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    // Equality is total: a non-Integer (or an error) is simply not equal.
    return boolean(isInt(a[0]) and iv(self) == iv(a[0]));
}
fn iLt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    if (argFault(a[0])) |f| return f; // ordering across types is undefined
    return boolean(iv(self) < iv(a[0]));
}
fn iLe(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    if (argFault(a[0])) |f| return f;
    return boolean(iv(self) <= iv(a[0]));
}
fn iGt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    if (argFault(a[0])) |f| return f;
    return boolean(iv(self) > iv(a[0]));
}
fn iGe(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    if (argFault(a[0])) |f| return f;
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
fn bPrintString(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const s: []const u8 = if (self == g_api.trueObj) "true" else "false";
    return g_api.mkString(s.ptr, s.len);
}

/// The DEFAULT `printString` (installed on `Object`), so every object is printable
/// for display / `,` / the REPL. Specific types (Integer, String, Vector, …)
/// override it; this is the fallback for the rest (blocks, plain instances).
fn objPrintString(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    const s = "anObject";
    return g_api.mkString(s.ptr, s.len);
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

fn pathOf(o: Oop) []const u8 {
    var len: usize = 0;
    const p = g_api.stringBytes(o, &len);
    return p[0..len];
}

/// `[System readFile: pathString]` → the file's contents as a String, or an
/// `#ioError` value if it cannot be read (no NULL / no panic).
fn sysReadFile(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const data = std.fs.cwd().readFileAlloc(ca, pathOf(a[0]), 1 << 30) catch
        return mkErr("ioError", "could not read file");
    defer ca.free(data);
    return g_api.mkString(data.ptr, data.len);
}

/// `[System writeFile: pathString contents: aString]` → write `aString` to `path`
/// (creating/truncating); answers self, or an `#ioError` value on failure.
fn sysWriteFile(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var len: usize = 0;
    const p = g_api.stringBytes(a[1], &len);
    std.fs.cwd().writeFile(.{ .sub_path = pathOf(a[0]), .data = p[0..len] }) catch
        return mkErr("ioError", "could not write file");
    return self;
}

/// `[System readLine]` → the next line from stdin as a String (newline stripped),
/// or nil at end of input. Line-oriented (one byte at a time) so it composes with
/// an interactive read loop.
fn sysReadLine(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    const stdin = std.fs.File.stdin();
    var buf: std.ArrayList(u8) = .{};
    defer buf.deinit(ca);
    while (true) {
        var b: [1]u8 = undefined;
        const nr = stdin.read(&b) catch return mkErr("ioError", "stdin read failed");
        if (nr == 0) {
            if (buf.items.len == 0) return g_api.nilObj; // EOF with nothing buffered
            break;
        }
        if (b[0] == '\n') break;
        buf.append(ca, b[0]) catch @panic("prelude: oom");
    }
    return g_api.mkString(buf.items.ptr, buf.items.len);
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

/// `[System autoGc: aBoolean]` → enable/disable automatic collection.
fn sysAutoGc(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    g_api.gcSetAuto(if (a[0] == g_api.trueObj) 1 else 0);
    return self;
}

// -- bringup -----------------------------------------------------------------

pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    g_error_vt = null; // re-resolve against THIS api (matters for fresh test inits)
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

    // A default printString on Object so any object can be displayed; specific
    // types override it. (Installed before the type-specific ones below.)
    api.method(api.object_vt, api.intern("printString"), &objPrintString);

    const b_vt = api.vtableOf(api.trueObj);
    api.method(b_vt, api.intern("ifTrue:ifFalse:"), &bIfTrueIfFalse);
    api.method(b_vt, api.intern("ifTrue:"), &bIfTrue);
    api.method(b_vt, api.intern("ifFalse:"), &bIfFalse);
    api.method(b_vt, api.intern("&"), &bAnd);
    api.method(b_vt, api.intern("|"), &bOr);
    api.method(b_vt, api.intern("not"), &bNot);
    api.method(b_vt, api.intern("printString"), &bPrintString);

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
    api.method(system_vt, api.intern("readFile:"), &sysReadFile);
    api.method(system_vt, api.intern("writeFile:contents:"), &sysWriteFile);
    api.method(system_vt, api.intern("readLine"), &sysReadLine);
    api.method(system_vt, api.intern("gc"), &sysGc);
    api.method(system_vt, api.intern("liveObjects"), &sysLiveObjects);
    api.method(system_vt, api.intern("autoGc:"), &sysAutoGc);

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
