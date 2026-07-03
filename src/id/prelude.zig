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
//!   Integer (smallint_vt): mod: = < <= > >= + - * /  printString  asFloat  to:do:
//!     (arithmetic is fault-tolerant: overflow/type/zero-divide → catchable errors)
//!   Float   (flt_vt):      + - * / = < <= > >=  negated abs sqrt  asInteger
//!                          floor ceiling rounded  printString
//!   Object  (object_vt):   printString  asString (→ printString of any receiver)
//!                          (new/clone are floor protocol — installed in id.zig)
//!   Boolean (bool_vt):     ifTrue:ifFalse: ifTrue: ifFalse: & | not  printString
//!   System (a singleton):  print:  println:  readFile:  writeFile:contents:
//!                          load: (eval a .birnam source file) loadModule: (dlopen
//!                          a module dylib) readLine  gc  liveObjects  autoGc:
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

// -- numeric tower: Integer + Float, with int↔float promotion ----------------
//
// SmallInt and the boxed Float are the floor's two numbers. A binary numeric op
// classifies its argument: an Error propagates, a non-number is a `#typeMismatch`,
// two Integers stay exact (i128 then range-checked), and ANY Float operand
// promotes the whole operation to f64 (`1 + 2.5` and `2.5 + 1` both → 3.5).

const NumKind = enum { int, float, err, bad };
fn numKind(o: Oop) NumKind {
    if (isErr(o)) return .err;
    if (isInt(o)) return .int;
    if (g_api.isFloat(o) != 0) return .float;
    return .bad;
}
fn isFloatO(o: Oop) bool {
    return g_api.isFloat(o) != 0;
}
/// A number's f64 value (Integer widened, or the Float's payload).
fn asF64(o: Oop) f64 {
    return if (isInt(o)) @floatFromInt(iv(o)) else g_api.floatValue(o);
}
fn mkf(v: f64) Oop {
    return g_api.mkFloat(v);
}

fn iAdd(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return switch (numKind(a[0])) {
        .err => a[0],
        .bad => mkErr("typeMismatch", "Integer arithmetic needs a number"),
        .float => mkf(asF64(self) + g_api.floatValue(a[0])),
        .int => intResult(@as(i128, iv(self)) + @as(i128, iv(a[0]))),
    };
}
fn iSub(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return switch (numKind(a[0])) {
        .err => a[0],
        .bad => mkErr("typeMismatch", "Integer arithmetic needs a number"),
        .float => mkf(asF64(self) - g_api.floatValue(a[0])),
        .int => intResult(@as(i128, iv(self)) - @as(i128, iv(a[0]))),
    };
}
fn iMul(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return switch (numKind(a[0])) {
        .err => a[0],
        .bad => mkErr("typeMismatch", "Integer arithmetic needs a number"),
        .float => mkf(asF64(self) * g_api.floatValue(a[0])),
        .int => intResult(@as(i128, iv(self)) * @as(i128, iv(a[0]))),
    };
}
fn iDiv(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return switch (numKind(a[0])) {
        .err => a[0],
        .bad => mkErr("typeMismatch", "Integer arithmetic needs a number"),
        .float => if (g_api.floatValue(a[0]) == 0.0) mkErr("zeroDivide", "division by zero") else mkf(asF64(self) / g_api.floatValue(a[0])),
        .int => if (iv(a[0]) == 0) mkErr("zeroDivide", "division by zero") else intResult(@divTrunc(@as(i128, iv(self)), @as(i128, iv(a[0])))),
    };
}
fn iMod(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    // mod: is integer-only (no float remainder in the floor protocol).
    switch (numKind(a[0])) {
        .err => return a[0],
        .int => {},
        else => return mkErr("typeMismatch", "mod: needs an Integer"),
    }
    if (iv(a[0]) == 0) return mkErr("zeroDivide", "division by zero");
    return intResult(@mod(@as(i128, iv(self)), @as(i128, iv(a[0]))));
}
fn iEq(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    // Numeric equality is total + cross-kind (`1 = 1.0`); a non-number is not equal.
    return switch (numKind(a[0])) {
        .int => boolean(iv(self) == iv(a[0])),
        .float => boolean(asF64(self) == g_api.floatValue(a[0])),
        else => g_api.falseObj,
    };
}
/// Shared integer comparison: promote on a Float arg, else compare exactly; a
/// non-number / error short-circuits. `op` picks the relation.
fn iCmp(self: Oop, arg: Oop, comptime op: std.math.CompareOperator) Oop {
    return switch (numKind(arg)) {
        .err => arg,
        .bad => mkErr("typeMismatch", "comparison needs a number"),
        .float => boolean(std.math.compare(asF64(self), op, g_api.floatValue(arg))),
        .int => boolean(std.math.compare(iv(self), op, iv(arg))),
    };
}
fn iLt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return iCmp(self, a[0], .lt);
}
fn iLe(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return iCmp(self, a[0], .lte);
}
fn iGt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return iCmp(self, a[0], .gt);
}
fn iGe(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return iCmp(self, a[0], .gte);
}
fn iPrintString(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    var buf: [24]u8 = undefined;
    const s = std.fmt.bufPrint(&buf, "{d}", .{iv(self)}) catch return g_api.nilObj;
    return g_api.mkString(s.ptr, s.len);
}
/// `[anInteger asFloat]` → the Float with the same value.
fn iAsFloat(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return mkf(@floatFromInt(iv(self)));
}

// -- Float (float_vt) --------------------------------------------------------

fn fAdd(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return switch (numKind(a[0])) {
        .err => a[0],
        .bad => mkErr("typeMismatch", "Float arithmetic needs a number"),
        .int, .float => mkf(g_api.floatValue(self) + asF64(a[0])),
    };
}
fn fSub(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return switch (numKind(a[0])) {
        .err => a[0],
        .bad => mkErr("typeMismatch", "Float arithmetic needs a number"),
        .int, .float => mkf(g_api.floatValue(self) - asF64(a[0])),
    };
}
fn fMul(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return switch (numKind(a[0])) {
        .err => a[0],
        .bad => mkErr("typeMismatch", "Float arithmetic needs a number"),
        .int, .float => mkf(g_api.floatValue(self) * asF64(a[0])),
    };
}
fn fDiv(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    switch (numKind(a[0])) {
        .err => return a[0],
        .bad => return mkErr("typeMismatch", "Float arithmetic needs a number"),
        .int, .float => {},
    }
    const d = asF64(a[0]);
    if (d == 0.0) return mkErr("zeroDivide", "division by zero");
    return mkf(g_api.floatValue(self) / d);
}
fn fCmp(self: Oop, arg: Oop, comptime op: std.math.CompareOperator) Oop {
    return switch (numKind(arg)) {
        .err => arg,
        .bad => mkErr("typeMismatch", "comparison needs a number"),
        .int, .float => boolean(std.math.compare(g_api.floatValue(self), op, asF64(arg))),
    };
}
fn fLt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return fCmp(self, a[0], .lt);
}
fn fLe(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return fCmp(self, a[0], .lte);
}
fn fGt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return fCmp(self, a[0], .gt);
}
fn fGe(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return fCmp(self, a[0], .gte);
}
fn fEq(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return switch (numKind(a[0])) {
        .int, .float => boolean(g_api.floatValue(self) == asF64(a[0])),
        else => g_api.falseObj,
    };
}
fn fNegated(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return mkf(-g_api.floatValue(self));
}
/// `[aFloat asInteger]` → the Integer formed by truncating toward zero (range-
/// checked: an out-of-range or non-finite float is an `#overflow` error).
fn fAsInteger(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const v = g_api.floatValue(self);
    const fmin: f64 = @floatFromInt(SMALLINT_MIN);
    const fmax: f64 = @floatFromInt(SMALLINT_MAX);
    if (!std.math.isFinite(v) or v < fmin or v > fmax)
        return mkErr("overflow", "float out of Integer range");
    return mki(@intFromFloat(std.math.trunc(v)));
}

// A Float rounded toward -inf / +inf / nearest, as an Integer (range-checked like
// `asInteger`). `round` selects which; only the rounding op differs.
fn floatToIntBy(self: Oop, comptime round: anytype) Oop {
    const v = round(g_api.floatValue(self));
    const fmin: f64 = @floatFromInt(SMALLINT_MIN);
    const fmax: f64 = @floatFromInt(SMALLINT_MAX);
    if (!std.math.isFinite(v) or v < fmin or v > fmax)
        return mkErr("overflow", "float out of Integer range");
    return mki(@intFromFloat(v));
}
fn fFloor(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return floatToIntBy(self, std.math.floor);
}
fn fCeiling(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return floatToIntBy(self, std.math.ceil);
}
fn fRounded(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return floatToIntBy(self, std.math.round);
}
fn fAbs(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return mkf(@abs(g_api.floatValue(self)));
}
/// `[f sqrt]` — the square root, or a `#domain` error for a negative receiver.
fn fSqrt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const v = g_api.floatValue(self);
    if (v < 0) return mkErr("domain", "sqrt of a negative number");
    return mkf(@sqrt(v));
}

/// `[anObject asString]` — the receiver's `printString` (a String), for any type.
/// Delegates through a send so each type's own `printString` override is used.
fn objAsString(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return send0(self, "printString");
}

// (Base-level object creation — `new`/`clone` — lives on object_vt in the floor
// (id.zig), installed at boot so it is always present, not gated on this module.)
fn fPrintString(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    var buf: [32]u8 = undefined;
    const s = std.fmt.bufPrint(&buf, "{d}", .{g_api.floatValue(self)}) catch return g_api.nilObj;
    // Ensure a float reads as one: a bare integer-valued float (`4`) gets a `.0`
    // so it round-trips to the `float` grammar rule (4.0), not the `number` rule.
    var plain = true;
    for (s) |ch| if (ch == '.' or ch == 'e' or ch == 'E' or ch == 'n' or ch == 'i') {
        plain = false;
        break;
    };
    if (!plain) return g_api.mkString(s.ptr, s.len);
    var b2: [34]u8 = undefined;
    const s2 = std.fmt.bufPrint(&b2, "{s}.0", .{s}) catch return g_api.mkString(s.ptr, s.len);
    return g_api.mkString(s2.ptr, s2.len);
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

const RTLD_LAZY: c_int = 1;

/// `[System load: pathString]` — read a birnam SOURCE file and evaluate it in the
/// running image (top-level `let`s persist to Globals, methods install), exactly
/// as if the file's forms had been typed at the REPL. Answers the value of the
/// last form; a `#ioError` if the file can't be read. This is how a birnam module
/// (e.g. a PEG-based parser) is loaded from code — no siphon involvement.
fn sysLoad(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const src = std.fs.cwd().readFileAlloc(ca, pathOf(a[0]), 1 << 30) catch
        return mkErr("ioError", "could not read source file");
    defer ca.free(src);
    const codegen = g_api.import("Codegen");
    if (codegen == null) return mkErr("noCodegen", "Codegen module is not loaded");
    return g_api.send(codegen, g_api.intern("evalPersisting:"), &[_]Oop{g_api.mkString(src.ptr, src.len)}, 1);
}

/// `[System loadModule: pathString]` — dlopen a birnam MODULE dylib and run its
/// `module_init` (installing whatever protocol/objects it exports), the same way
/// the siphon loads a rung at boot. Answers self on success, else an error. For
/// resolving raw C symbols (FFI), use `[Alien dlopen:]` instead — this is for
/// dylibs that self-install through `module_init`.
fn sysLoadModule(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const path = pathOf(a[0]);
    const pathz = ca.allocSentinel(u8, path.len, 0) catch @panic("prelude: oom");
    defer ca.free(pathz);
    @memcpy(pathz[0..path.len], path);
    const h = g_api.dlopen(pathz.ptr, RTLD_LAZY) orelse
        return mkErr("ioError", "could not load module dylib");
    const raw = g_api.dlsym(h, "module_init") orelse
        return mkErr("notAModule", "dylib has no module_init");
    const init: idapi.ModuleInitFn = @ptrCast(@alignCast(raw));
    _ = init(g_api, null);
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

/// `[System preciseGc: aBoolean]` → trace layout-bearing types' payloads exactly
/// (docs/40 §3c) instead of conservatively; types without a layout descriptor
/// stay conservative either way.
fn sysPreciseGc(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    g_api.gcSetPrecise(if (a[0] == g_api.trueObj) 1 else 0);
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
    api.method(int_vt, api.intern("asFloat"), &iAsFloat);
    api.method(int_vt, api.intern("to:do:"), &iToDo);

    // Float (boxed f64): the same numeric protocol, promoting Integer args.
    const flt_vt = api.float_vt;
    api.method(flt_vt, api.intern("+"), &fAdd);
    api.method(flt_vt, api.intern("-"), &fSub);
    api.method(flt_vt, api.intern("*"), &fMul);
    api.method(flt_vt, api.intern("/"), &fDiv);
    api.method(flt_vt, api.intern("="), &fEq);
    api.method(flt_vt, api.intern("<"), &fLt);
    api.method(flt_vt, api.intern("<="), &fLe);
    api.method(flt_vt, api.intern(">"), &fGt);
    api.method(flt_vt, api.intern(">="), &fGe);
    api.method(flt_vt, api.intern("negated"), &fNegated);
    api.method(flt_vt, api.intern("asInteger"), &fAsInteger);
    api.method(flt_vt, api.intern("abs"), &fAbs);
    api.method(flt_vt, api.intern("floor"), &fFloor);
    api.method(flt_vt, api.intern("ceiling"), &fCeiling);
    api.method(flt_vt, api.intern("rounded"), &fRounded);
    api.method(flt_vt, api.intern("sqrt"), &fSqrt);
    api.method(flt_vt, api.intern("printString"), &fPrintString);

    // A default printString on Object so any object can be displayed; specific
    // types override it. (Installed before the type-specific ones below.)
    api.method(api.object_vt, api.intern("printString"), &objPrintString);
    api.method(api.object_vt, api.intern("asString"), &objAsString);

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
    api.method(system_vt, api.intern("load:"), &sysLoad);
    api.method(system_vt, api.intern("loadModule:"), &sysLoadModule);
    api.method(system_vt, api.intern("readLine"), &sysReadLine);
    api.method(system_vt, api.intern("gc"), &sysGc);
    api.method(system_vt, api.intern("liveObjects"), &sysLiveObjects);
    api.method(system_vt, api.intern("autoGc:"), &sysAutoGc);
    api.method(system_vt, api.intern("preciseGc:"), &sysPreciseGc);

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
