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
//! Also provides the HAMT sibling of the RRB:
//!   Map (map_vt)        a persistent hash-array-mapped map over bitmap-indexed
//!                         nodes that are themselves libid objects (same
//!                         id-native discipline as the Vector trie):
//!                         at:put: at: at:ifAbsent: includesKey: size isEmpty
//!                         keys values do: keysAndValuesDo: printString
//!   Map factory         `[Map new]` → an empty map (exported "Map")
//! Keys compare by VALUE for the floor types — SmallInt by value, String by
//! bytes, Symbol/other by identity — so equal string keys deduplicate (better
//! than the old kernel's identity-only maps). `at:put:` is persistent (returns a
//! new map sharing structure), so it folds with `inject:into:`.
//!
//! A `{k -> v}` map literal + `#[…]` vector literal are a grammar/codegen concern
//! (the lang front-end has no collection literals yet) — until then both
//! collections are reachable from birnam by message: `[Map new]`, `[Vector …]`.

const std = @import("std");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ca = std.heap.c_allocator;

var g_api: *idapi.IdApi = undefined;

// The vtables this module owns (created on first load, re-imported on reload so
// instances minted by a previous load keep dispatching).
pub var vnode_vt: Oop = null; // vector trie node: payload = [WIDTH]Oop
pub var vector_vt: Oop = null; // persistent ordered vector
pub var range_vt: Oop = null; // lazy integer range
pub var hnode_vt: Oop = null; // HAMT node: payload = header + [2*k]Oop entry slots
pub var map_vt: Oop = null; // persistent hash-array-mapped map
pub var map_factory_vt: Oop = null;
pub var map_factory_obj: Oop = null; // the "Map" factory ([Map new])
pub var vector_factory_vt: Oop = null;
pub var vector_factory_obj: Oop = null; // the "Vector" factory ([Vector new])

// Cached selectors for invoking caller blocks (value / value: / value:value:).
var s_value: Oop = null;
var s_value1: Oop = null;
var s_value2: Oop = null;
var s_printString: Oop = null;
var s_asVector: Oop = null;
var s_lt: Oop = null; // "<" — the sort comparator (sortBy:)

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

// Stable sort by a precomputed key per element. The comparator sends `<` between
// keys, so any key type with a `<` protocol works (SmallInt via the prelude is
// the common case); ties keep input order (insertion sort is stable).
const SortCtx = struct { keys: []const Oop };
fn keyLessThan(ctx: SortCtx, lhs: usize, rhs: usize) bool {
    return isTrue(g_api.send(ctx.keys[lhs], s_lt, &[_]Oop{ctx.keys[rhs]}, 1));
}

/// `aVector sortBy: aBlock` → a new Vector of the elements ordered ascending by
/// `[aBlock value: element]` (the sort key, computed once per element). Descending
/// is the negated key. Original vector unchanged.
fn vSortBy(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const v = vec(self);
    const cnt = v.count;
    if (cnt < 2) return self;

    const elems = ca.alloc(Oop, cnt) catch @panic("collection: oom");
    defer ca.free(elems);
    const keys = ca.alloc(Oop, cnt) catch @panic("collection: oom");
    defer ca.free(keys);
    const idx = ca.alloc(usize, cnt) catch @panic("collection: oom");
    defer ca.free(idx);
    var i: usize = 0;
    while (i < cnt) : (i += 1) {
        elems[i] = vecAt(v, i);
        keys[i] = call1(a[0], elems[i]);
        idx[i] = i;
    }

    std.sort.insertion(usize, idx, SortCtx{ .keys = keys }, keyLessThan);

    var out = emptyVec();
    for (idx) |j| out = vecCons(out, elems[j]);
    return out;
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

// -- the HAMT (hash array-mapped trie) ---------------------------------------
// A bitmap-indexed persistent hash map, the unordered sibling of the vector
// trie. A node is a libid object whose payload is a fixed header followed by a
// compact `[2*k]Oop` array of slot pairs — `k` = popcount(bitmap) for a bitmap
// node, or the entry count for a hash-collision node. Each slot pair is either
// `(key, value)` or, when `key == null` (the Zig absence Oop, never a real key),
// `(null, subnode)`. Path-copying on insert gives structural sharing (F4); the
// Oop slots keep it traceable by the future GC's layout descriptor (F3).

const HNODE_BITMAP: u32 = 0;
const HNODE_COLLISION: u32 = 1;

const HNode = extern struct {
    tag: u32, // HNODE_BITMAP | HNODE_COLLISION
    n: u32, // bitmap node: the 32-bit occupancy bitmap; collision node: entry count
    hash: u32, // collision node: the shared full hash; bitmap node: 0
    _pad: u32 = 0,
};

/// `g_added` is set by `assoc` when an insert grows the key set (vs replacing a
/// value), so `at:put:` can keep an exact `count`. Single-writer (F4), so a
/// module-global flag is sound — no box threaded through the recursion.
var g_added: bool = false;

fn hnode(o: Oop) *HNode {
    return @ptrCast(@alignCast(o.?));
}
fn hslots(o: Oop) [*]Oop {
    const base: [*]u8 = @ptrCast(o.?);
    return @ptrCast(@alignCast(base + @sizeOf(HNode)));
}
fn hSlotCount(o: Oop) usize {
    const h = hnode(o);
    return if (h.tag == HNODE_BITMAP) 2 * @as(usize, @popCount(h.n)) else 2 * @as(usize, h.n);
}
fn newHNode(tag: u32, n: u32, hash: u32, nslots: usize) Oop {
    const o = g_api.alloc(hnode_vt, @sizeOf(HNode) + nslots * @sizeOf(Oop)); // zeroed
    hnode(o).* = .{ .tag = tag, .n = n, .hash = hash };
    return o;
}
fn copyHNode(o: Oop) Oop {
    const nslots = hSlotCount(o);
    const total = @sizeOf(HNode) + nslots * @sizeOf(Oop);
    const nn = g_api.alloc(hnode_vt, total);
    const src: [*]const u8 = @ptrCast(o.?);
    const dst: [*]u8 = @ptrCast(nn.?);
    @memcpy(dst[0..total], src[0..total]);
    return nn;
}

fn chunk(hash: u32, shift: usize) u5 {
    return @intCast((hash >> @as(u5, @intCast(shift))) & MASK);
}
fn bitposOf(hash: u32, shift: usize) u32 {
    return @as(u32, 1) << chunk(hash, shift);
}
fn sparseIndex(bitmap: u32, bit: u32) usize {
    return @popCount(bitmap & (bit - 1));
}

// -- key hashing + equality (the F6 seam, for the floor's value types) -------

fn mix(x: u64) u32 {
    // A cheap finaliser (splitmix64-style) folded to 32 bits.
    var z = x +% 0x9E3779B97F4A7C15;
    z = (z ^ (z >> 30)) *% 0xBF58476D1CE4E5B9;
    z = (z ^ (z >> 27)) *% 0x94D049BB133111EB;
    return @truncate(z ^ (z >> 31));
}
fn keyHash(k: Oop) u32 {
    if (g_api.isSmallInt(k) != 0) return mix(@bitCast(g_api.smallIntValue(k)));
    if (g_api.vtableOf(k) == g_api.string_vt) {
        var len: usize = 0;
        const p = g_api.stringBytes(k, &len);
        var h: u32 = 2166136261; // FNV-1a
        for (p[0..len]) |b| {
            h ^= b;
            h *%= 16777619;
        }
        return h;
    }
    return mix(@intFromPtr(k.?)); // symbols (interned) + other objects: identity
}
fn keyEq(a: Oop, b: Oop) bool {
    if (a == b) return true; // SmallInt immediates + interned symbols + identity
    if (g_api.vtableOf(a) == g_api.string_vt and g_api.vtableOf(b) == g_api.string_vt) {
        var la: usize = 0;
        var lb: usize = 0;
        const pa = g_api.stringBytes(a, &la);
        const pb = g_api.stringBytes(b, &lb);
        return std.mem.eql(u8, pa[0..la], pb[0..lb]);
    }
    return false;
}

// -- HAMT core (find / assoc), all path-copying ------------------------------

/// The value bound to `key`, or Zig `null` for absent (distinct from a stored
/// `nilObj` value). `node` may be null (an empty map).
fn hFind(node: Oop, shift: usize, hash: u32, key: Oop) Oop {
    if (node == null) return null;
    const h = hnode(node);
    const s = hslots(node);
    if (h.tag == HNODE_BITMAP) {
        const bit = bitposOf(hash, shift);
        if (h.n & bit == 0) return null;
        const idx = 2 * sparseIndex(h.n, bit);
        const k = s[idx];
        if (k == null) return hFind(s[idx + 1], shift + BITS, hash, key);
        return if (keyEq(k, key)) s[idx + 1] else null;
    }
    // collision node
    if (hash != h.hash) return null;
    var i: usize = 0;
    while (i < h.n) : (i += 1) {
        if (keyEq(s[2 * i], key)) return s[2 * i + 1];
    }
    return null;
}

/// Build a node holding two distinct-keyed entries (their hashes differ, or they
/// share a hash → a collision node). Terminates: distinct 32-bit hashes diverge
/// within 7 chunks, so `shift` never exceeds 30 here.
fn mergeTwo(shift: usize, k1: Oop, h1: u32, v1: Oop, k2: Oop, h2: u32, v2: Oop) Oop {
    if (h1 == h2) {
        const nn = newHNode(HNODE_COLLISION, 2, h1, 4);
        const s = hslots(nn);
        s[0] = k1;
        s[1] = v1;
        s[2] = k2;
        s[3] = v2;
        return nn;
    }
    const c1 = chunk(h1, shift);
    const c2 = chunk(h2, shift);
    if (c1 == c2) {
        const sub = mergeTwo(shift + BITS, k1, h1, v1, k2, h2, v2);
        const bit = @as(u32, 1) << c1;
        const nn = newHNode(HNODE_BITMAP, bit, 0, 2);
        const s = hslots(nn);
        s[0] = null;
        s[1] = sub;
        return nn;
    }
    const nn = newHNode(HNODE_BITMAP, (@as(u32, 1) << c1) | (@as(u32, 1) << c2), 0, 4);
    const s = hslots(nn);
    // entries must be ordered by chunk so sparseIndex addresses them correctly.
    if (c1 < c2) {
        s[0] = k1;
        s[1] = v1;
        s[2] = k2;
        s[3] = v2;
    } else {
        s[0] = k2;
        s[1] = v2;
        s[2] = k1;
        s[3] = v1;
    }
    return nn;
}

/// Return a new node with `key`→`val` associated; sets `g_added` if the key was
/// not already present. `node` may be null.
fn hAssoc(node: Oop, shift: usize, hash: u32, key: Oop, val: Oop) Oop {
    if (node == null) {
        g_added = true;
        const nn = newHNode(HNODE_BITMAP, bitposOf(hash, shift), 0, 2);
        const s = hslots(nn);
        s[0] = key;
        s[1] = val;
        return nn;
    }
    const h = hnode(node);
    const s = hslots(node);
    if (h.tag == HNODE_BITMAP) {
        const bit = bitposOf(hash, shift);
        const idx = 2 * sparseIndex(h.n, bit);
        if (h.n & bit != 0) {
            const k = s[idx];
            if (k == null) { // descend into the subnode
                const sub = s[idx + 1];
                const nsub = hAssoc(sub, shift + BITS, hash, key, val);
                if (nsub == sub) return node;
                const nn = copyHNode(node);
                hslots(nn)[idx + 1] = nsub;
                return nn;
            }
            if (keyEq(k, key)) { // replace this key's value
                if (s[idx + 1] == val) return node;
                const nn = copyHNode(node);
                hslots(nn)[idx + 1] = val;
                return nn;
            }
            // two distinct keys in one slot → push both into a fresh subnode.
            g_added = true;
            const sub = mergeTwo(shift + BITS, k, keyHash(k), s[idx + 1], key, hash, val);
            const nn = copyHNode(node);
            hslots(nn)[idx] = null;
            hslots(nn)[idx + 1] = sub;
            return nn;
        }
        // empty slot: splice a new (key,val) pair in, keeping bitmap order.
        g_added = true;
        const cnt = @popCount(h.n);
        const nn = newHNode(HNODE_BITMAP, h.n | bit, 0, 2 * (@as(usize, cnt) + 1));
        const ns = hslots(nn);
        var i: usize = 0;
        while (i < idx) : (i += 1) ns[i] = s[i];
        ns[idx] = key;
        ns[idx + 1] = val;
        i = idx;
        while (i < 2 * cnt) : (i += 1) ns[i + 2] = s[i];
        return nn;
    }
    // collision node
    if (hash == h.hash) {
        var i: usize = 0;
        while (i < h.n) : (i += 1) {
            if (keyEq(s[2 * i], key)) {
                if (s[2 * i + 1] == val) return node;
                const nn = copyHNode(node);
                hslots(nn)[2 * i + 1] = val;
                return nn;
            }
        }
        g_added = true;
        const nn = newHNode(HNODE_COLLISION, h.n + 1, hash, 2 * (@as(usize, h.n) + 1));
        const ns = hslots(nn);
        var j: usize = 0;
        while (j < 2 * h.n) : (j += 1) ns[j] = s[j];
        ns[2 * h.n] = key;
        ns[2 * h.n + 1] = val;
        return nn;
    }
    // different hash than the collision bucket: wrap it in a bitmap node, recurse.
    const wrapper = newHNode(HNODE_BITMAP, bitposOf(h.hash, shift), 0, 2);
    const ws = hslots(wrapper);
    ws[0] = null;
    ws[1] = node;
    return hAssoc(wrapper, shift, hash, key, val);
}

/// Walk every entry, threading an accumulator. `want_keys`=true collects keys,
/// otherwise values; both produce a Vector via the (persistent) vector cons.
fn collectInto(node: Oop, acc: Oop, want_keys: bool) Oop {
    if (node == null) return acc;
    const s = hslots(node);
    const ns = hSlotCount(node);
    var out = acc;
    var i: usize = 0;
    while (i < ns) : (i += 2) {
        if (s[i] == null) {
            out = collectInto(s[i + 1], out, want_keys); // subnode
        } else {
            out = vecCons(out, if (want_keys) s[i] else s[i + 1]);
        }
    }
    return out;
}

// -- Map object --------------------------------------------------------------

const Map = extern struct {
    root: Oop, // an HNode, or null when empty
    count: usize,
};

fn mp(o: Oop) *Map {
    return @ptrCast(@alignCast(o.?));
}
fn mkMap(root: Oop, count: usize) Oop {
    const o = g_api.alloc(map_vt, @sizeOf(Map));
    mp(o).* = .{ .root = root, .count = count };
    return o;
}

fn mAtPut(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const m = mp(self);
    g_added = false;
    const nroot = hAssoc(m.root, 0, keyHash(a[0]), a[0], a[1]);
    if (nroot == m.root) return self;
    return mkMap(nroot, m.count + @intFromBool(g_added));
}
fn mAt(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const v = hFind(mp(self).root, 0, keyHash(a[0]), a[0]);
    return v orelse g_api.nilObj;
}
fn mAtIfAbsent(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const v = hFind(mp(self).root, 0, keyHash(a[0]), a[0]);
    return v orelse g_api.send(a[1], s_value, &[_]Oop{}, 0);
}
fn mIncludesKey(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return boolean(hFind(mp(self).root, 0, keyHash(a[0]), a[0]) != null);
}
fn mSize(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return mki(@intCast(mp(self).count));
}
fn mIsEmpty(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return boolean(mp(self).count == 0);
}
fn mKeys(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return collectInto(mp(self).root, emptyVec(), true);
}
fn mValues(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return collectInto(mp(self).root, emptyVec(), false);
}
fn mDo(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return vDo(mValues(self, undefined, 0), a, 1); // iterate over values
}
fn mKeysAndValuesDo(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    kvDo(mp(self).root, a[0]);
    return self;
}
fn kvDo(node: Oop, blk: Oop) void {
    if (node == null) return;
    const s = hslots(node);
    const ns = hSlotCount(node);
    var i: usize = 0;
    while (i < ns) : (i += 2) {
        if (s[i] == null) kvDo(s[i + 1], blk) else _ = call2(blk, s[i], s[i + 1]);
    }
}
fn mPrintString(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    // Buffer + first-flag are LOCAL (threaded through), not module globals — a
    // value that is itself a Map re-enters this function safely (nested records
    // from pull); a shared global would dangle when the inner call frees it.
    var buf: std.ArrayList(u8) = .{};
    defer buf.deinit(ca);
    var first = true;
    buf.append(ca, '{') catch @panic("collection: oom");
    kvPrint(&buf, &first, mp(self).root);
    buf.append(ca, '}') catch @panic("collection: oom");
    return g_api.mkString(buf.items.ptr, buf.items.len);
}
fn kvPrint(buf: *std.ArrayList(u8), first: *bool, node: Oop) void {
    if (node == null) return;
    const s = hslots(node);
    const ns = hSlotCount(node);
    var i: usize = 0;
    while (i < ns) : (i += 2) {
        if (s[i] == null) {
            kvPrint(buf, first, s[i + 1]);
            continue;
        }
        if (!first.*) buf.appendSlice(ca, ", ") catch @panic("collection: oom");
        first.* = false;
        appendPrint(buf, s[i]);
        buf.appendSlice(ca, " -> ") catch @panic("collection: oom");
        appendPrint(buf, s[i + 1]);
    }
}
fn appendPrint(buf: *std.ArrayList(u8), o: Oop) void {
    const str = g_api.send(o, s_printString, &[_]Oop{}, 0);
    var len: usize = 0;
    const p = g_api.stringBytes(str, &len);
    buf.appendSlice(ca, p[0..len]) catch @panic("collection: oom");
}

fn mapNew(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return mkMap(null, 0);
}

/// `[Vector new]` → an empty persistent vector (the seed for `#[…]` literals and
/// `add:` chains). The vector factory is the birnam-reachable constructor; the
/// instance vtable is exported separately as "VectorVt" (an extension target).
fn vecNew(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return emptyVec();
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

/// `aString printString` → the string itself (so a String element renders in a
/// Vector/Map without dispatching to the panicking default — the floor's String
/// has no printString of its own).
fn strPrintString(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return self;
}

fn isString(o: Oop) bool {
    return o != null and g_api.vtableOf(o) == g_api.string_vt;
}

/// `aString , x` → a fresh String concatenating the receiver with `x`. A non-String
/// argument is first coerced via `printString` (so `"n=" , 5` → `"n=5"`), and if
/// that still is not a String it contributes nothing — `,` never untags a
/// non-string as bytes (which read a garbage ptr/len and crashed).
fn strComma(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var arg = a[0];
    if (!isString(arg)) arg = g_api.send(arg, s_printString, &[_]Oop{}, 0);
    const sa = strBytes(self); // the receiver is always a String (dispatched here)
    const sb = if (isString(arg)) strBytes(arg) else "";
    const buf = ca.alloc(u8, sa.len + sb.len) catch @panic("collection: oom");
    defer ca.free(buf);
    @memcpy(buf[0..sa.len], sa);
    @memcpy(buf[sa.len..], sb);
    return g_api.mkString(buf.ptr, buf.len);
}

fn strBytes(o: Oop) []const u8 {
    var len: usize = 0;
    const p = g_api.stringBytes(o, &len);
    return p[0..len];
}

/// `aString asLowercase` → a fresh String with ASCII A–Z folded to lowercase.
fn strAsLowercase(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const src = strBytes(self);
    const buf = ca.alloc(u8, src.len) catch @panic("collection: oom");
    defer ca.free(buf);
    for (src, 0..) |ch, i| buf[i] = std.ascii.toLower(ch);
    return g_api.mkString(buf.ptr, buf.len);
}

/// `aString replace: needle with: repl` → a fresh String with every non-
/// overlapping occurrence of `needle` replaced by `repl`. An empty needle is a
/// no-op (returns self) — there is nothing to find and it would never terminate.
fn strReplaceWith(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const hay = strBytes(self);
    const needle = strBytes(a[0]);
    const repl = strBytes(a[1]);
    if (needle.len == 0) return self;
    var out: std.ArrayList(u8) = .{};
    defer out.deinit(ca);
    var i: usize = 0;
    while (i < hay.len) {
        if (i + needle.len <= hay.len and std.mem.eql(u8, hay[i .. i + needle.len], needle)) {
            out.appendSlice(ca, repl) catch @panic("collection: oom");
            i += needle.len;
        } else {
            out.append(ca, hay[i]) catch @panic("collection: oom");
            i += 1;
        }
    }
    return g_api.mkString(out.items.ptr, out.items.len);
}

/// `aString words` → a Vector of the whitespace-separated runs (empty runs
/// dropped). Splits on ASCII whitespace only — punctuation stays attached, so
/// callers strip it first (e.g. via `replace:with:`).
fn strWords(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    var out = emptyVec();
    var it = std.mem.tokenizeAny(u8, strBytes(self), " \t\r\n\x0b\x0c");
    while (it.next()) |word| {
        out = vecCons(out, g_api.mkString(word.ptr, word.len));
    }
    return out;
}

/// `aString asSymbol` → the interned symbol for these bytes (a stable identity,
/// so equal strings yield the SAME symbol — usable as an identity map key).
fn strAsSymbol(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const src = strBytes(self);
    const buf = ca.allocSentinel(u8, src.len, 0) catch @panic("collection: oom");
    defer ca.free(buf);
    @memcpy(buf[0..src.len], src);
    return g_api.intern(buf.ptr);
}

// -- bringup -----------------------------------------------------------------

pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    s_value = api.intern("value");
    s_value1 = api.intern("value:");
    s_value2 = api.intern("value:value:");
    s_printString = api.intern("printString");
    s_asVector = api.intern("asVector");
    s_lt = api.intern("<");

    const reloading = api.isExported("Vector") != 0;
    if (reloading) {
        vnode_vt = api.import("VectorNode");
        vector_vt = api.import("VectorVt");
        vector_factory_vt = api.import("VectorFactoryVt");
        vector_factory_obj = api.import("Vector");
        range_vt = api.import("Range");
        hnode_vt = api.import("MapNode");
        map_vt = api.import("MapVt");
        map_factory_vt = api.import("MapFactoryVt");
        map_factory_obj = api.import("Map");
    } else {
        vnode_vt = api.proto(api.object_vt);
        vector_vt = api.proto(api.object_vt);
        vector_factory_vt = api.proto(api.object_vt);
        vector_factory_obj = api.alloc(vector_factory_vt, 0);
        range_vt = api.proto(api.object_vt);
        hnode_vt = api.proto(api.object_vt);
        map_vt = api.proto(api.object_vt);
        map_factory_vt = api.proto(api.object_vt);
        map_factory_obj = api.alloc(map_factory_vt, 0);
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
    api.method(vector_vt, api.intern("sortBy:"), &vSortBy);
    api.method(vector_factory_vt, api.intern("new"), &vecNew);

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

    // Map protocol (over the HAMT) + the `[Map new]` factory.
    api.method(map_vt, api.intern("at:put:"), &mAtPut);
    api.method(map_vt, api.intern("at:"), &mAt);
    api.method(map_vt, api.intern("at:ifAbsent:"), &mAtIfAbsent);
    api.method(map_vt, api.intern("includesKey:"), &mIncludesKey);
    api.method(map_vt, api.intern("size"), &mSize);
    api.method(map_vt, api.intern("isEmpty"), &mIsEmpty);
    api.method(map_vt, api.intern("keys"), &mKeys);
    api.method(map_vt, api.intern("values"), &mValues);
    api.method(map_vt, api.intern("do:"), &mDo);
    api.method(map_vt, api.intern("keysAndValuesDo:"), &mKeysAndValuesDo);
    api.method(map_vt, api.intern("printString"), &mPrintString);
    api.method(map_factory_vt, api.intern("new"), &mapNew);

    // Integer `to:` → Range.
    api.method(api.smallint_vt, api.intern("to:"), &iTo);

    // isNil / notNil: the default on Object, overridden on nil.
    api.method(api.object_vt, api.intern("isNil"), &falseAns);
    api.method(api.object_vt, api.intern("notNil"), &trueAns);
    const nil_vt = api.vtableOf(api.nilObj);
    api.method(nil_vt, api.intern("isNil"), &trueAns);
    api.method(nil_vt, api.intern("notNil"), &falseAns);

    // String: concatenation + a self-returning printString (for Vector/Map print).
    api.method(api.string_vt, api.intern(","), &strComma);
    api.method(api.string_vt, api.intern("printString"), &strPrintString);
    api.method(api.string_vt, api.intern("asLowercase"), &strAsLowercase);
    api.method(api.string_vt, api.intern("replace:with:"), &strReplaceWith);
    api.method(api.string_vt, api.intern("words"), &strWords);
    api.method(api.string_vt, api.intern("asSymbol"), &strAsSymbol);

    if (!reloading) {
        _ = api.exportObj("VectorNode", vnode_vt);
        _ = api.exportObj("Vector", vector_factory_obj); // `[Vector new]`
        _ = api.exportObj("VectorVt", vector_vt);
        _ = api.exportObj("VectorFactoryVt", vector_factory_vt);
        _ = api.exportObj("Range", range_vt);
        _ = api.exportObj("MapNode", hnode_vt);
        _ = api.exportObj("Map", map_factory_obj); // `[Map new]`
        _ = api.exportObj("MapVt", map_vt);
        _ = api.exportObj("MapFactoryVt", map_factory_vt);
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
