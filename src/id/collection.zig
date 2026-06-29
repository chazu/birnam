//! src/id/collection.zig — the id-native collection library (rung 3 stdlib).
//!
//! The first SUPERIMPOSITION of a real data structure on the libid floor, built
//! "id-native" by deliberate choice (the F3/F5 endgame, not a throwaway array):
//! the persistent vector's internal tree nodes are themselves libid objects
//! allocated through `api.alloc`, carrying their own vtable at [-1] and holding
//! arrays of `Oop` slots. That buys three things a Zig-opaque payload can't:
//!   * UNIFORM GC — when the collector lands, a per-vtable layout descriptor
//!     ("K Oop-slots") traces a node like any other object; no bespoke trace fn.
//!   * STRUCTURAL SHARING — `add:` is persistent (path-copying), so updates share
//!     subtrees (Clojure PersistentVector / the RRB family), matching F4's
//!     immutable-values invariant.
//!   * REFLECTABILITY — the structure is walkable through `send`, not only Zig.
//!
//! Provides:
//!   Vector (vector_vt)  a persistent ordered vector over a 32-way radix trie:
//!                         size at: add: do: select: collect: detect:
//!                         inject:into: isEmpty asVector printString ,
//!   Range  (range_vt)   a lazy inclusive integer range (start..end), value type:
//!                         size do: select: collect: detect: inject:into:
//!                         includes: asVector printString ,
//!   Integer (smallint_vt)  to:  → a Range
//!   Object  (object_vt)    isNil → false,  notNil → true
//!   nil                    isNil → true,   notNil → false
//!   String  (string_vt)    ,    → byte concatenation
//! Exports the `Vector` / `Range` / `VectorNode` vtables so birnam can name them.
//!
//! Maps/sets (the HAMT sibling of this RRB) arrive with map literals `{k:v}`;
//! this module is structured so adding a `hamt.zig`-style node beside the vector
//! node is the natural next step. Ordered sequences are what the examples
//! (primes, word-frequency) need first, so the vector trie comes first.

const std = @import("std");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ca = std.heap.c_allocator;

var g_api: *idapi.IdApi = undefined;

// The vtables this module owns (created on first load, re-imported on reload so
// instances minted by a previous load keep dispatching).
pub var vnode_vt: Oop = null; // trie node: payload = [WIDTH]Oop
pub var vector_vt: Oop = null; // persistent ordered vector
pub var range_vt: Oop = null; // lazy integer range

// Cached selectors for invoking caller blocks (value: / value:value:).
var s_value1: Oop = null;
var s_value2: Oop = null;
var s_printString: Oop = null;
var s_asVector: Oop = null;

// -- the radix trie (Clojure PersistentVector layout) ------------------------
// BITS=5 ⇒ 32-way branching; SHIFT increases by BITS per level. A node's payload
// is exactly WIDTH `Oop` slots: in an internal node they are child nodes, in a
// leaf they are elements. The `tail` is a leaf node buffering the trailing <=32
// elements so `add:` is amortized O(1).

const BITS: u6 = 5;
const WIDTH: usize = 1 << BITS; // 32
const MASK: usize = WIDTH - 1; // 31

/// A vector's payload. `root` is the trie root (an internal node); `tail` is a
/// leaf node holding the last `tail_len` elements not yet pushed into the trie.
const Vec = extern struct {
    count: usize, // total element count
    shift: usize, // root-level shift in bits (BITS * (height-1)); u6 when shifting
    root: Oop,
    tail: Oop,
    tail_len: usize,
};

const Range = extern struct {
    start: i64,
    end: i64, // inclusive
};

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
fn isTrue(o: Oop) bool {
    return o == g_api.trueObj;
}
fn call1(blk: Oop, x: Oop) Oop {
    return g_api.send(blk, s_value1, &[_]Oop{x}, 1);
}
fn call2(blk: Oop, x: Oop, y: Oop) Oop {
    return g_api.send(blk, s_value2, &[_]Oop{ x, y }, 2);
}

fn nodeSlots(node: Oop) [*]Oop {
    return @ptrCast(@alignCast(node.?));
}
fn newNode() Oop {
    return g_api.alloc(vnode_vt, WIDTH * @sizeOf(Oop)); // alloc zeroes → slots null
}
fn copyNode(node: Oop) Oop {
    const n = newNode();
    const dst = nodeSlots(n);
    const src = nodeSlots(node);
    var i: usize = 0;
    while (i < WIDTH) : (i += 1) dst[i] = src[i];
    return n;
}

fn vec(o: Oop) *Vec {
    return @ptrCast(@alignCast(o.?));
}
fn mkVec(count: usize, shift: usize, root: Oop, tail: Oop, tail_len: usize) Oop {
    const o = g_api.alloc(vector_vt, @sizeOf(Vec));
    vec(o).* = .{ .count = count, .shift = shift, .root = root, .tail = tail, .tail_len = tail_len };
    return o;
}
fn emptyVec() Oop {
    return mkVec(0, BITS, newNode(), newNode(), 0);
}

fn tailoff(v: *Vec) usize {
    return v.count - v.tail_len;
}

/// The leaf array holding element `i` (Clojure `arrayFor`).
fn arrayFor(v: *Vec, i: usize) Oop {
    if (i >= tailoff(v)) return v.tail;
    var node = v.root;
    var level: u6 = @intCast(v.shift);
    while (level > 0) : (level -= BITS) {
        node = nodeSlots(node)[(i >> level) & MASK];
    }
    return node;
}

fn vecAt(v: *Vec, i: usize) Oop {
    if (i >= v.count) return g_api.nilObj;
    return nodeSlots(arrayFor(v, i))[i & MASK];
}

/// `newPath` — a chain of single-child nodes `level` deep ending in `node`.
fn newPath(level: u6, node: Oop) Oop {
    if (level == 0) return node;
    const ret = newNode();
    nodeSlots(ret)[0] = newPath(level - BITS, node);
    return ret;
}

/// `pushTail` — insert a full tail leaf into the trie, path-copying. `old_count`
/// is the count BEFORE the append (Clojure uses `count-1` to find the slot).
fn pushTail(old_count: usize, level: u6, parent: Oop, tailnode: Oop) Oop {
    const subidx = ((old_count - 1) >> level) & MASK;
    const ret = copyNode(parent);
    const insert: Oop = if (level == BITS) tailnode else blk: {
        const child = nodeSlots(parent)[subidx];
        break :blk if (child != null)
            pushTail(old_count, level - BITS, child, tailnode)
        else
            newPath(level - BITS, tailnode);
    };
    nodeSlots(ret)[subidx] = insert;
    return ret;
}

/// Persistent append — returns a NEW vector sharing structure with the old one.
fn vecCons(o: Oop, val: Oop) Oop {
    const v = vec(o);
    // Room in the tail: copy it, append, done (the common path).
    if (v.tail_len < WIDTH) {
        const ntail = copyNode(v.tail);
        nodeSlots(ntail)[v.tail_len] = val;
        return mkVec(v.count + 1, v.shift, v.root, ntail, v.tail_len + 1);
    }
    // Tail full: it becomes a leaf in the trie; the new value starts a fresh tail.
    const tailnode = v.tail;
    const shift: u6 = @intCast(v.shift);
    var nshift = v.shift;
    var nroot: Oop = undefined;
    if ((v.count >> BITS) > (@as(usize, 1) << shift)) {
        // Root overflow: grow the trie one level taller.
        nroot = newNode();
        nodeSlots(nroot)[0] = v.root;
        nodeSlots(nroot)[1] = newPath(shift, tailnode);
        nshift = v.shift + BITS;
    } else {
        nroot = pushTail(v.count, shift, v.root, tailnode);
    }
    const ntail = newNode();
    nodeSlots(ntail)[0] = val;
    return mkVec(v.count + 1, nshift, nroot, ntail, 1);
}

// -- Vector methods ----------------------------------------------------------

fn vSize(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return mki(@intCast(vec(self).count));
}
fn vIsEmpty(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return boolean(vec(self).count == 0);
}
fn vAt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const i = iv(a[0]);
    if (i < 0) return g_api.nilObj;
    return vecAt(vec(self), @intCast(i));
}
fn vAdd(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return vecCons(self, a[0]);
}
fn vDo(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const v = vec(self);
    var i: usize = 0;
    while (i < v.count) : (i += 1) _ = call1(a[0], vecAt(v, i));
    return self;
}
fn vSelect(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const v = vec(self);
    var out = emptyVec();
    var i: usize = 0;
    while (i < v.count) : (i += 1) {
        const e = vecAt(v, i);
        if (isTrue(call1(a[0], e))) out = vecCons(out, e);
    }
    return out;
}
fn vCollect(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const v = vec(self);
    var out = emptyVec();
    var i: usize = 0;
    while (i < v.count) : (i += 1) out = vecCons(out, call1(a[0], vecAt(v, i)));
    return out;
}
fn vDetect(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const v = vec(self);
    var i: usize = 0;
    while (i < v.count) : (i += 1) {
        const e = vecAt(v, i);
        if (isTrue(call1(a[0], e))) return e;
    }
    return g_api.nilObj;
}
fn vInject(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const v = vec(self);
    var acc = a[0];
    var i: usize = 0;
    while (i < v.count) : (i += 1) acc = call2(a[1], acc, vecAt(v, i));
    return acc;
}
fn vAsVector(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return self;
}
fn vPrintString(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return printElements(self);
}
fn vComma(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const other = ensureVector(a[0]);
    var out = self;
    const ov = vec(other);
    var i: usize = 0;
    while (i < ov.count) : (i += 1) out = vecCons(out, vecAt(ov, i));
    return out;
}

/// Coerce any sequence to a Vector (Range answers `asVector`; a Vector is self).
fn ensureVector(o: Oop) Oop {
    if (g_api.vtableOf(o) == vector_vt) return o;
    return g_api.send(o, s_asVector, &[_]Oop{}, 0);
}

/// `#[e0 e1 …]` — each element's own `printString`, space-joined in brackets.
/// Mirrors birnam's vector-literal syntax so a printed vector reads back.
fn printElements(o: Oop) Oop {
    var buf: std.ArrayList(u8) = .{};
    defer buf.deinit(ca);
    buf.appendSlice(ca, "#[") catch @panic("collection: oom");
    const v = vec(ensureVector(o));
    var i: usize = 0;
    while (i < v.count) : (i += 1) {
        if (i != 0) buf.append(ca, ' ') catch @panic("collection: oom");
        const s = g_api.send(vecAt(v, i), s_printString, &[_]Oop{}, 0);
        var len: usize = 0;
        const p = g_api.stringBytes(s, &len);
        buf.appendSlice(ca, p[0..len]) catch @panic("collection: oom");
    }
    buf.append(ca, ']') catch @panic("collection: oom");
    return g_api.mkString(buf.items.ptr, buf.items.len);
}

// -- Range methods (lazy; enumerations materialize to a Vector) --------------

fn rng(o: Oop) *Range {
    return @ptrCast(@alignCast(o.?));
}
fn mkRange(start: i64, end: i64) Oop {
    const o = g_api.alloc(range_vt, @sizeOf(Range));
    rng(o).* = .{ .start = start, .end = end };
    return o;
}

fn rSize(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const r = rng(self);
    return mki(if (r.end < r.start) 0 else r.end - r.start + 1);
}
fn rIncludes(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const r = rng(self);
    const x = iv(a[0]);
    return boolean(x >= r.start and x <= r.end);
}
fn rDo(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const r = rng(self);
    var i = r.start;
    while (i <= r.end) : (i += 1) _ = call1(a[0], mki(i));
    return self;
}
fn rSelect(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const r = rng(self);
    var out = emptyVec();
    var i = r.start;
    while (i <= r.end) : (i += 1) {
        const e = mki(i);
        if (isTrue(call1(a[0], e))) out = vecCons(out, e);
    }
    return out;
}
fn rCollect(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const r = rng(self);
    var out = emptyVec();
    var i = r.start;
    while (i <= r.end) : (i += 1) out = vecCons(out, call1(a[0], mki(i)));
    return out;
}
fn rDetect(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const r = rng(self);
    var i = r.start;
    while (i <= r.end) : (i += 1) {
        const e = mki(i);
        if (isTrue(call1(a[0], e))) return e;
    }
    return g_api.nilObj;
}
fn rInject(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const r = rng(self);
    var acc = a[0];
    var i = r.start;
    while (i <= r.end) : (i += 1) acc = call2(a[1], acc, mki(i));
    return acc;
}
fn rAsVector(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const r = rng(self);
    var out = emptyVec();
    var i = r.start;
    while (i <= r.end) : (i += 1) out = vecCons(out, mki(i));
    return out;
}
fn rPrintString(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return printElements(self);
}
fn rComma(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return vComma(rAsVector(self, undefined, 0), a, 1);
}

// -- extensions to existing floor types --------------------------------------

/// `[start to: end]` → an inclusive Range value (lazy; `to:do:` stays in the
/// prelude for the imperative loop — `to:` answers the first-class collection).
fn iTo(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return mkRange(iv(self), iv(a[0]));
}

fn falseAns(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return g_api.falseObj;
}
fn trueAns(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return g_api.trueObj;
}

/// `aString , bString` → a fresh String of the two byte runs concatenated.
fn strComma(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var la: usize = 0;
    var lb: usize = 0;
    const pa = g_api.stringBytes(self, &la);
    const pb = g_api.stringBytes(a[0], &lb);
    const buf = ca.alloc(u8, la + lb) catch @panic("collection: oom");
    defer ca.free(buf);
    @memcpy(buf[0..la], pa[0..la]);
    @memcpy(buf[la .. la + lb], pb[0..lb]);
    return g_api.mkString(buf.ptr, buf.len);
}

// -- bringup -----------------------------------------------------------------

pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    s_value1 = api.intern("value:");
    s_value2 = api.intern("value:value:");
    s_printString = api.intern("printString");
    s_asVector = api.intern("asVector");

    const reloading = api.isExported("Vector") != 0;
    if (reloading) {
        vnode_vt = api.import("VectorNode");
        vector_vt = api.import("Vector");
        range_vt = api.import("Range");
    } else {
        vnode_vt = api.proto(api.object_vt);
        vector_vt = api.proto(api.object_vt);
        range_vt = api.proto(api.object_vt);
    }

    // Vector protocol.
    api.method(vector_vt, api.intern("size"), &vSize);
    api.method(vector_vt, api.intern("isEmpty"), &vIsEmpty);
    api.method(vector_vt, api.intern("at:"), &vAt);
    api.method(vector_vt, api.intern("add:"), &vAdd);
    api.method(vector_vt, api.intern("do:"), &vDo);
    api.method(vector_vt, api.intern("select:"), &vSelect);
    api.method(vector_vt, api.intern("collect:"), &vCollect);
    api.method(vector_vt, api.intern("detect:"), &vDetect);
    api.method(vector_vt, api.intern("inject:into:"), &vInject);
    api.method(vector_vt, api.intern("asVector"), &vAsVector);
    api.method(vector_vt, api.intern("printString"), &vPrintString);
    api.method(vector_vt, api.intern(","), &vComma);

    // Range protocol.
    api.method(range_vt, api.intern("size"), &rSize);
    api.method(range_vt, api.intern("includes:"), &rIncludes);
    api.method(range_vt, api.intern("do:"), &rDo);
    api.method(range_vt, api.intern("select:"), &rSelect);
    api.method(range_vt, api.intern("collect:"), &rCollect);
    api.method(range_vt, api.intern("detect:"), &rDetect);
    api.method(range_vt, api.intern("inject:into:"), &rInject);
    api.method(range_vt, api.intern("asVector"), &rAsVector);
    api.method(range_vt, api.intern("printString"), &rPrintString);
    api.method(range_vt, api.intern(","), &rComma);

    // Integer `to:` → Range.
    api.method(api.smallint_vt, api.intern("to:"), &iTo);

    // isNil / notNil: the default on Object, overridden on nil.
    api.method(api.object_vt, api.intern("isNil"), &falseAns);
    api.method(api.object_vt, api.intern("notNil"), &trueAns);
    const nil_vt = api.vtableOf(api.nilObj);
    api.method(nil_vt, api.intern("isNil"), &trueAns);
    api.method(nil_vt, api.intern("notNil"), &falseAns);

    // String concatenation.
    api.method(api.string_vt, api.intern(","), &strComma);

    if (!reloading) {
        _ = api.exportObj("VectorNode", vnode_vt);
        _ = api.exportObj("Vector", vector_vt);
        _ = api.exportObj("Range", range_vt);
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
