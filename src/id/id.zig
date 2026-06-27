//! libid — the pure Piumarta object system, hand-written in Zig.
//!
//! A faithful port of the model in `~/dev/c/birnam/src/id.c` (Piumarta's "open,
//! extensible object model"). This is the FLOOR the siphon loads first — before
//! peg, before the compiler, and long before the fact store. There are **no
//! eids and no fact store here**: identity is the pointer, the vtable pointer
//! lives at `object[-1]`, selectors are interned symbol objects compared by
//! pointer, and a vtable carries its own method dict (`keys`/`values`) and a
//! `parent` for delegation.
//!
//! Bootstrap (the 5 messages): `lookup`, `addMethod`, `allocate`, `delegated`
//! are installed on the metavtable; `addMethod` is then used metacircularly to
//! install the rest. `bind` bottoms out when binding `lookup` on the metavtable.

const std = @import("std");
const state_mod = @import("state.zig");

const ca = std.heap.c_allocator;

/// Every object is referenced by an opaque pointer; its vtable pointer sits one
/// word *before* it (`object[-1]`). `Oop` = "ordinary object pointer".
pub const Oop = ?*anyopaque;

/// A primitive method: receiver + a flat argument buffer. Pointer-based, no eids.
pub const Method = *const fn (self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop;

/// The word reserved before each payload for the vtable pointer.
const VT: usize = @sizeOf(usize);

/// A vtable is an object too (its own vtable pointer at [-1] is the metavtable).
/// `keys`/`values` are the parallel method-dictionary arrays; `parent` is the
/// delegation link (null = top of chain).
pub const Vtable = extern struct {
    size: usize,
    tally: usize,
    keys: ?[*]Oop,
    values: ?[*]Oop,
    parent: ?*Vtable,
};

/// An interned name. Selectors are `*Symbol`, compared by pointer identity.
pub const Symbol = extern struct {
    string: [*:0]u8,
};

// -- The well-known objects (set by `boot`) ---------------------------------

pub var vtable_vt: *Vtable = undefined; // the vtable of vtables (self-describing)
pub var object_vt: *Vtable = undefined; // top of the delegation chain
pub var symbol_vt: *Vtable = undefined;
var symbol_list: *Vtable = undefined; // the intern table (keys = symbols)

pub var s_lookup: *Symbol = undefined;
pub var s_addMethod: *Symbol = undefined;
pub var s_allocate: *Symbol = undefined;
pub var s_delegated: *Symbol = undefined;

var booted = false;

// -- Raw allocation + the `-1` vtable-pointer trick -------------------------

fn allocRaw(payload: usize) Oop {
    const total = (VT + payload + 7) & ~@as(usize, 7);
    const buf = ca.alignedAlloc(u8, .@"16", total) catch @panic("libid: oom");
    @memset(buf, 0);
    return @ptrFromInt(@intFromPtr(buf.ptr) + VT);
}

fn vtSlot(obj: Oop) *?*Vtable {
    return @ptrFromInt(@intFromPtr(obj.?) - VT);
}
pub fn vtableOf(obj: Oop) ?*Vtable {
    return vtSlot(obj).*;
}
fn setVtable(obj: Oop, vt: ?*Vtable) void {
    vtSlot(obj).* = vt;
}

inline fn oop(p: anytype) Oop {
    return @ptrCast(p);
}
inline fn asVtable(o: Oop) *Vtable {
    return @ptrCast(@alignCast(o.?));
}

// -- send / bind (metacircular) ---------------------------------------------

/// Resolve `sel` on `rcv`'s vtable. Bottoms out when binding `lookup` on the
/// metavtable (else infinite regress); otherwise sends `lookup` to the vtable.
pub fn bind(rcv: Oop, sel: *Symbol) ?Method {
    const vt = vtableOf(rcv) orelse return null;
    const m: Oop = if (vt == vtable_vt and sel == s_lookup)
        vtableLookup(oop(vt), &[_]Oop{oop(sel)}, 1)
    else
        send1(oop(vt), s_lookup, oop(sel));
    if (m == null) return null;
    return @ptrCast(@alignCast(m));
}

pub fn send(rcv: Oop, sel: *Symbol, args: []const Oop) Oop {
    const m = bind(rcv, sel) orelse {
        std.debug.print("libid: doesNotUnderstand: {s}\n", .{sel.string});
        @panic("libid: method not found");
    };
    return m(rcv, args.ptr, args.len);
}

// Thin arity helpers (no varargs packing — Zig slices do the job).
pub fn send0(rcv: Oop, sel: *Symbol) Oop {
    return send(rcv, sel, &[_]Oop{});
}
pub fn send1(rcv: Oop, sel: *Symbol, a: Oop) Oop {
    return send(rcv, sel, &[_]Oop{a});
}
pub fn send2(rcv: Oop, sel: *Symbol, a: Oop, b: Oop) Oop {
    return send(rcv, sel, &[_]Oop{ a, b });
}

// -- The primitive methods (installed on vtable_vt) -------------------------

/// `[vt lookup: key]` — find the method bound to `key`, walking `parent`.
fn vtableLookup(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const vt = asVtable(self);
    const key = args[0];
    if (vt.keys) |keys| {
        var i: usize = 0;
        while (i < vt.tally) : (i += 1) {
            if (keys[i] == key) return vt.values.?[i];
        }
    }
    if (vt.parent) |p| return send1(oop(p), s_lookup, key);
    return null;
}

/// `[vt addMethod: key with: method]` — bind `key` → `method` (overwrite if present).
fn vtableAddMethod(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const vt = asVtable(self);
    const key = args[0];
    const method = args[1];
    var i: usize = 0;
    while (i < vt.tally) : (i += 1) {
        if (vt.keys.?[i] == key) {
            vt.values.?[i] = method;
            return method;
        }
    }
    if (vt.tally == vt.size) growDict(vt);
    vt.keys.?[vt.tally] = key;
    vt.values.?[vt.tally] = method;
    vt.tally += 1;
    return method;
}

/// `[vt allocate: size]` — a fresh instance whose vtable is `vt`. `size` is the
/// payload byte count, passed pointer-encoded (libid is pre-SmallInt).
fn vtableAllocate(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const vt = asVtable(self);
    const size: usize = @intFromPtr(args[0]);
    const obj = allocRaw(size);
    setVtable(obj, vt);
    return obj;
}

/// `[vt delegated]` — a fresh child vtable delegating to `vt`.
fn vtableDelegated(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    return oop(newVtable(asVtable(self)));
}

// -- Construction helpers (used by boot; not messages) ----------------------

fn growDict(vt: *Vtable) void {
    const ns = if (vt.size == 0) 4 else vt.size * 2;
    const nk = ca.alloc(Oop, ns) catch @panic("libid: oom");
    const nv = ca.alloc(Oop, ns) catch @panic("libid: oom");
    @memset(nk, null);
    @memset(nv, null);
    if (vt.keys) |k| std.mem.copyForwards(Oop, nk, k[0..vt.tally]);
    if (vt.values) |v| std.mem.copyForwards(Oop, nv, v[0..vt.tally]);
    vt.keys = nk.ptr;
    vt.values = nv.ptr;
    vt.size = ns;
}

/// Allocate a bare vtable object (vtable pointer = vtable_vt) with `parent`.
fn newVtable(parent: ?*Vtable) *Vtable {
    const obj = allocRaw(@sizeOf(Vtable));
    // During boot, vtable_vt may not exist yet; `boot` patches the first ones.
    if (booted) setVtable(obj, vtable_vt);
    const vt = asVtable(obj);
    vt.size = 0;
    vt.tally = 0;
    vt.keys = null;
    vt.values = null;
    vt.parent = parent;
    return vt;
}

fn intern(string: [:0]const u8) *Symbol {
    var i: usize = 0;
    while (i < symbol_list.tally) : (i += 1) {
        const sym: *Symbol = @ptrCast(@alignCast(symbol_list.keys.?[i]));
        if (std.mem.orderZ(u8, sym.string, string) == .eq) return sym;
    }
    const obj = allocRaw(@sizeOf(Symbol));
    setVtable(obj, symbol_vt);
    const sym = asSymbol(obj);
    sym.string = (ca.dupeZ(u8, string) catch @panic("libid: oom")).ptr;
    _ = vtableAddMethod(oop(symbol_list), &[_]Oop{ oop(sym), null }, 2);
    return sym;
}
inline fn asSymbol(o: Oop) *Symbol {
    return @ptrCast(@alignCast(o.?));
}

// -- Bootstrap --------------------------------------------------------------

/// Bring up the object system. After this, `send` is fully operational.
pub fn boot() void {
    // vtable_vt: the metavtable, self-describing (its own vtable is itself).
    vtable_vt = newVtable(null);
    setVtable(oop(vtable_vt), vtable_vt);

    // object_vt: top of the delegation chain; vtables delegate to it.
    object_vt = newVtable(null);
    setVtable(oop(object_vt), vtable_vt);
    vtable_vt.parent = object_vt;

    symbol_vt = newVtable(object_vt);
    setVtable(oop(symbol_vt), vtable_vt);

    symbol_list = newVtable(null);
    setVtable(oop(symbol_list), vtable_vt);
    booted = true;

    s_lookup = intern("lookup");
    s_addMethod = intern("addMethod");
    s_allocate = intern("allocate");
    s_delegated = intern("delegated");

    // Install lookup + addMethod directly (send can't work without them), then
    // use addMethod *via send* (metacircular) for the rest.
    _ = vtableAddMethod(oop(vtable_vt), &[_]Oop{ oop(s_lookup), @constCast(@ptrCast(&vtableLookup)) }, 2);
    _ = vtableAddMethod(oop(vtable_vt), &[_]Oop{ oop(s_addMethod), @constCast(@ptrCast(&vtableAddMethod)) }, 2);
    _ = send2(oop(vtable_vt), s_addMethod, oop(s_allocate), @constCast(@ptrCast(&vtableAllocate)));
    _ = send2(oop(vtable_vt), s_addMethod, oop(s_delegated), @constCast(@ptrCast(&vtableDelegated)));
}

/// Intern a symbol from outside (the siphon / later modules need this).
pub fn symbol(string: [:0]const u8) *Symbol {
    return intern(string);
}

// -- Module entry (the siphon loads libid through this) ---------------------

fn sendC(rcv: Oop, sel: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    return send(rcv, @ptrCast(@alignCast(sel)), args[0..n]);
}
fn internC(string: [*:0]const u8) callconv(.c) Oop {
    return oop(intern(std.mem.span(string)));
}

/// `entrypoint(state)` — boot the object system and publish its primitives into
/// the shared state. Matches the C module ABI: the siphon dlopens this dylib,
/// finds `entrypoint`, and calls it.
pub export fn entrypoint(st: *state_mod.State) callconv(.c) void {
    boot();
    st.send = &sendC;
    st.intern = &internC;
    st.vtable_vt = oop(vtable_vt);
    st.object_vt = oop(object_vt);
    st.symbol_vt = oop(symbol_vt);
}

// -- Proof: the object system stands on its own -----------------------------

test "libid: metacircular bootstrap + a user method dispatches" {
    boot();

    // The closure is self-describing: vtable_vt's vtable is itself.
    try std.testing.expect(vtableOf(oop(vtable_vt)) == vtable_vt);
    try std.testing.expect(vtableOf(oop(object_vt)) == vtable_vt);
    try std.testing.expect(vtable_vt.parent == object_vt);

    // Define a Point vtable delegating to object, via SEND (metacircular path):
    //   point_vt = [object_vt delegated]
    const point_vt = asVtable(send0(oop(object_vt), s_delegated));
    try std.testing.expect(point_vt.parent == object_vt);

    // Install a method on it via send: [point_vt addMethod: #answer with: fn]
    const s_answer = intern("answer");
    _ = send2(oop(point_vt), s_addMethod, oop(s_answer), @constCast(@ptrCast(&answerImpl)));

    // Allocate an instance: [point_vt allocate: 0], then send it #answer.
    const p = send1(oop(point_vt), s_allocate, @ptrFromInt(0));
    try std.testing.expect(vtableOf(p) == point_vt);
    const result = send0(p, s_answer);
    try std.testing.expectEqual(@as(usize, 42), @intFromPtr(result));

    // Delegation works: #answer is found by walking point -> (its vt) -> dict;
    // and an unknown-on-point selector resolves up the chain to object_vt.
    // (object_vt has no methods yet, so we only assert the positive path here.)
}

fn answerImpl(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = args;
    _ = n;
    return @ptrFromInt(42);
}
