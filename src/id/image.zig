//! The image — the running system made durable (docs/40).
//!
//! An image is NOT a log you replay: it is the live system's store-reachable
//! state serialized so a fresh process can map it back and start sending
//! messages (docs/40 §1). The durability line is the store boundary — the
//! store's facts (globals are facts), and the heap graph transitively reachable
//! from fact values. What the system cannot name is garbage by definition and
//! does not survive.
//!
//! Layering (docs/40 §3): the STORE owns its facts and yields/accepts them raw
//! (`imageFactsInto:` / `imageLoadBegin`+`imageAddFact:…`+`imageLoadEnd:raw:`);
//! the FLOOR (id) exposes the heap primitives (`objectPayloadSize`,
//! `exportedNameOf`, `slotKind`); and THIS module owns the codec (leaf + symbol
//! + heap-ref), the relocation map, the heap walk, and the container format.
//!
//! On-disk format — a header then a stream of framed records read until EOF, so
//! appending is just writing more records (docs/40 §2, the journal shape):
//!   header: magic · format · endian · arch · abi_hash        (28 bytes)
//!   record: u32 kind · u64 length · payload
//! Record kinds this slice writes, HEAP BEFORE FACTS (facts reference heap ids):
//!   .heap   u32 n · n×(name · u32 nwords · words)  — the object graph
//!   .facts  u64 tx · u32 kept · kept×(u8 op · u64 tx · e · a · v)
//! A payload field is a tagged value: nil/true/false, int, float, symbol (by
//! name — the durable anchor), string (bytes), heapref (a u32 relocation id), or
//! a null Oop slot. Compound objects whose vtable is image-born (a block vtable)
//! or carries no layout are DEFERRED — counted, never dropped (docs/40 §6); code
//! pointers likewise defer to slice 4. Vtables relink BY NAME on load (the D5.1
//! spine): a native module's freshly-loaded vtable re-adopts its image objects.

const std = @import("std");
const builtin = @import("builtin");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const IdApi = idapi.IdApi;
const ca = std.heap.c_allocator;
const WORD = @sizeOf(usize);

var g_api: *IdApi = undefined;

// -- Header ------------------------------------------------------------------

pub const MAGIC = "BIRNAMIM";
pub const FORMAT_VERSION: u32 = 1;
const ENDIAN_TAG: u32 = 0x01020304;

/// Architecture tag — code sections are per-arch (docs/40 §4); data is portable.
const ARCH_TAG: u32 = switch (builtin.cpu.arch) {
    .aarch64 => 1,
    .x86_64 => 2,
    else => 0,
};

/// The runtime-shim ABI validity gate (docs/40 §3a/§4: the old boot_validity
/// idea). Folds a manual ABI version with the `birnam_*` shim NAME list (kept in
/// sync with tcc.zig + codegen). On load a mismatch means compiled sections can't
/// be trusted → replay (slice 4); the data sections stay portable regardless.
const RUNTIME_ABI_VERSION: u32 = 1;
const shim_names = [_][]const u8{
    "birnam_send",          "birnam_intern",        "birnam_mkSmallInt",
    "birnam_smallIntValue", "birnam_mkFloat",       "birnam_floatValue",
    "birnam_mkString",      "birnam_nil",           "birnam_true",
    "birnam_false",         "birnam_objectVt",      "birnam_proto",
    "birnam_method",        "birnam_alloc",         "birnam_cstr",
    "birnam_free_cstr",     "birnam_layoutAllOops", "birnam_global",
};
pub fn abiHash() u64 {
    var h: u64 = 0xcbf29ce484222325; // FNV-1a
    for (shim_names) |nm| for (nm) |b| {
        h ^= b;
        h *%= 0x100000001b3;
    };
    var v = RUNTIME_ABI_VERSION;
    var i: usize = 0;
    while (i < 4) : (i += 1) {
        h ^= @as(u8, @truncate(v));
        h *%= 0x100000001b3;
        v >>= 8;
    }
    return h;
}

const RecordKind = enum(u32) {
    interner = 1, // reserved: an explicit interner section (compaction) — by-name inline suffices for now
    facts = 2, // the store's (e,a,v,tx,op) stream
    heap = 3, // the reachable object graph
    code = 4, // compiled units (slice 4)
    _,
};

/// A serialized value. `heapref` is a relocation id into the heap section;
/// `nulloop` is a null payload slot (distinct from the `nil` singleton).
const Tag = enum(u8) {
    nil = 0,
    true_ = 1,
    false_ = 2,
    int = 3,
    float = 4,
    symbol = 5,
    string = 6,
    heapref = 7,
    nulloop = 8,
};

// -- Byte writer -------------------------------------------------------------

const Buf = std.ArrayListUnmanaged(u8);

fn putU32(b: *Buf, v: u32) void {
    var t: [4]u8 = undefined;
    std.mem.writeInt(u32, &t, v, .little);
    b.appendSlice(ca, &t) catch @panic("image: oom");
}
fn putU64(b: *Buf, v: u64) void {
    var t: [8]u8 = undefined;
    std.mem.writeInt(u64, &t, v, .little);
    b.appendSlice(ca, &t) catch @panic("image: oom");
}
fn putBytes(b: *Buf, s: []const u8) void {
    b.appendSlice(ca, s) catch @panic("image: oom");
}
fn putTag(b: *Buf, t: Tag) void {
    b.append(ca, @intFromEnum(t)) catch @panic("image: oom");
}
fn putLenBytes(b: *Buf, s: []const u8) void {
    putU32(b, @intCast(s.len));
    putBytes(b, s);
}

fn writeHeader(buf: *Buf) void {
    putBytes(buf, MAGIC);
    putU32(buf, FORMAT_VERSION);
    putU32(buf, ENDIAN_TAG);
    putU32(buf, ARCH_TAG);
    putU64(buf, abiHash());
}

/// Begin a framed record (kind + length placeholder); `endRecord` backpatches
/// the length once the payload is appended. Self-delimiting, no section table.
fn beginRecord(buf: *Buf, kind: RecordKind) usize {
    putU32(buf, @intFromEnum(kind));
    const len_at = buf.items.len;
    putU64(buf, 0);
    return len_at;
}
fn endRecord(buf: *Buf, len_at: usize) void {
    const payload_len = buf.items.len - (len_at + 8);
    std.mem.writeInt(u64, buf.items[len_at..][0..8], @intCast(payload_len), .little);
}

fn writeFile(path: []const u8, bytes: []const u8) bool {
    const file = std.fs.cwd().createFile(path, .{}) catch return false;
    defer file.close();
    file.writeAll(bytes) catch return false;
    return true;
}

/// Write a header-only (empty but valid) image — the pure format floor, no
/// running system needed (the format-level tests build images this way).
pub fn writeHeaderOnly(path: []const u8) bool {
    var buf: Buf = .{};
    defer buf.deinit(ca);
    writeHeader(&buf);
    return writeFile(path, buf.items);
}

// -- Save: the field codec + heap walk ---------------------------------------

// Save-time state, reset per save. `g_reloc` maps a live object's address to its
// relocation id; `g_objs` is the id→object worklist (index = id), which grows as
// the walk discovers nested objects. Nothing here is a GC root — but auto-GC is
// disabled across a save, and everything reachable is live in the store graph
// anyway, so no walked object can be collected mid-save.
var g_reloc: std.AutoHashMapUnmanaged(usize, u32) = .{};
var g_objs: std.ArrayListUnmanaged(Oop) = .{};
var g_facts_buf: *Buf = undefined; // the collector appends per-fact records here
var g_kept: u32 = 0;
var g_deferred_facts: u32 = 0;
var g_deferred_slots: u32 = 0;
var g_collector_vt: Oop = null;

fn resetSaveState() void {
    g_reloc.clearRetainingCapacity();
    g_objs.clearRetainingCapacity();
    g_kept = 0;
    g_deferred_facts = 0;
    g_deferred_slots = 0;
}

/// Register a compound object for serialization, assigning it a relocation id
/// (before walking its slots, so cycles terminate). Returns null — DEFERRING the
/// object — when its vtable is image-born (unexported: a block vtable, slice 4's
/// replay job) or carries no layout descriptor (can't be safely swizzled).
fn internObj(o: Oop) ?u32 {
    if (g_reloc.get(@intFromPtr(o.?))) |id| return id;
    const vt = g_api.vtableOf(o);
    if (vt == null) return null;
    if (g_api.exportedNameOf(vt) == null) return null; // image-born vtable → slice 4
    const psize = g_api.objectPayloadSize(o);
    if (psize > 0 and g_api.slotKind(vt, 0) == idapi.slot_unknown) return null; // no layout
    const id: u32 = @intCast(g_objs.items.len);
    g_reloc.put(ca, @intFromPtr(o.?), id) catch @panic("image: oom");
    g_objs.append(ca, o) catch @panic("image: oom");
    return id;
}

/// Encode one Oop field. Leaves serialize inline; symbols by name; compound
/// objects as a heap-ref (interning them). Returns false WITHOUT writing when the
/// value is a compound object that must be deferred (caller handles the skip).
fn encodeField(buf: *Buf, o: Oop) bool {
    if (o == null) {
        putTag(buf, .nulloop);
        return true;
    }
    if (o == g_api.nilObj) {
        putTag(buf, .nil);
        return true;
    }
    if (o == g_api.trueObj) {
        putTag(buf, .true_);
        return true;
    }
    if (o == g_api.falseObj) {
        putTag(buf, .false_);
        return true;
    }
    if (g_api.isSmallInt(o) != 0) {
        putTag(buf, .int);
        putU64(buf, @bitCast(g_api.smallIntValue(o)));
        return true;
    }
    if (g_api.isFloat(o) != 0) {
        putTag(buf, .float);
        putU64(buf, @bitCast(g_api.floatValue(o)));
        return true;
    }
    const vt = g_api.vtableOf(o);
    if (vt == g_api.symbol_vt) {
        putTag(buf, .symbol);
        putLenBytes(buf, std.mem.span(g_api.symbolName(o)));
        return true;
    }
    if (vt == g_api.string_vt) {
        putTag(buf, .string);
        var len: usize = 0;
        const p = g_api.stringBytes(o, &len);
        putLenBytes(buf, p[0..len]);
        return true;
    }
    const id = internObj(o) orelse return false; // compound; may defer
    putTag(buf, .heapref);
    putU32(buf, id);
    return true;
}

/// `[collector e: e a: a v: v tx: t op: o]` — the store drives this per fact
/// during `imageFactsInto:`. A fact commits only if ALL its fields encode (a
/// fact is atomic); a deferred field skips the whole fact (counted).
fn mCollect(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    var scratch: Buf = .{};
    defer scratch.deinit(ca);
    scratch.append(ca, @intCast(g_api.smallIntValue(args[4]))) catch @panic("image: oom"); // op
    putU64(&scratch, @intCast(g_api.smallIntValue(args[3]))); // tx
    if (encodeField(&scratch, args[0]) and encodeField(&scratch, args[1]) and encodeField(&scratch, args[2])) {
        g_facts_buf.appendSlice(ca, scratch.items) catch @panic("image: oom");
        g_kept += 1;
    } else g_deferred_facts += 1;
    return g_api.nilObj;
}

/// Drain the object worklist into the `.heap` payload: each object as its vtable
/// NAME (to relink by), its word count, and its payload words — raw words copied
/// verbatim, Oop words encoded (which discovers more objects → the worklist
/// grows). Code words are zero-width here (deferred to slice 4). The leading u32
/// count is backpatched to the final total.
fn drainHeap(buf: *Buf) void {
    const count_at = buf.items.len;
    putU32(buf, 0);
    var i: usize = 0;
    while (i < g_objs.items.len) : (i += 1) {
        const o = g_objs.items[i];
        const vt = g_api.vtableOf(o);
        putLenBytes(buf, std.mem.span(g_api.exportedNameOf(vt).?));
        const nwords = g_api.objectPayloadSize(o) / WORD;
        putU32(buf, @intCast(nwords));
        const base = @intFromPtr(o.?);
        var w: usize = 0;
        while (w < nwords) : (w += 1) {
            const word: *const usize = @ptrFromInt(base + w * WORD);
            switch (g_api.slotKind(vt, w)) {
                idapi.slot_oop => {
                    const slot: Oop = @ptrFromInt(word.*);
                    if (!encodeField(buf, slot)) {
                        putTag(buf, .nulloop);
                        g_deferred_slots += 1;
                    }
                },
                idapi.slot_code => g_deferred_slots += 1, // zero-width; slice 4
                else => putU64(buf, word.*), // slot_raw
            }
        }
    }
    std.mem.writeInt(u32, buf.items[count_at..][0..4], @intCast(g_objs.items.len), .little);
}

fn reportDeferred() void {
    if (g_deferred_facts > 0) std.debug.print(
        "[image] deferred {d} fact(s) with un-serializable values (image-born/no-layout; slice 4 replay)\n",
        .{g_deferred_facts},
    );
    if (g_deferred_slots > 0) std.debug.print(
        "[image] deferred {d} object slot(s) (image-born/code; slice 4)\n",
        .{g_deferred_slots},
    );
}

/// Serialize the store-reachable system to `path`: header, then the heap record
/// (if any objects were reached), then the facts record. Auto-GC is off for the
/// span (a quiescent save); the walk never collects what it is serializing.
pub fn saveImage(path: []const u8) bool {
    g_api.gcSetAuto(0);
    defer g_api.gcSetAuto(1);
    resetSaveState();

    var facts_recs: Buf = .{};
    defer facts_recs.deinit(ca);
    var heap_buf: Buf = .{};
    defer heap_buf.deinit(ca);

    // 1. Facts — interns the top-level compound fact values into the worklist.
    const store = g_api.import("Store");
    var tx: i64 = 0;
    if (store) |st| {
        tx = g_api.smallIntValue(g_api.send(st, g_api.intern("basis"), &[_]Oop{}, 0));
        g_facts_buf = &facts_recs;
        const collector = g_api.alloc(g_collector_vt, 0);
        _ = g_api.send(st, g_api.intern("imageFactsInto:"), &[_]Oop{collector}, 1);
    }
    // 2. Heap — drain the worklist, discovering nested objects.
    drainHeap(&heap_buf);

    // 3. Assemble the file: heap BEFORE facts (facts reference heap ids).
    var buf: Buf = .{};
    defer buf.deinit(ca);
    writeHeader(&buf);
    if (g_objs.items.len > 0) {
        const at = beginRecord(&buf, .heap);
        putBytes(&buf, heap_buf.items);
        endRecord(&buf, at);
    }
    if (store != null) {
        const at = beginRecord(&buf, .facts);
        putU64(&buf, @bitCast(tx));
        putU32(&buf, g_kept);
        putBytes(&buf, facts_recs.items);
        endRecord(&buf, at);
    }
    reportDeferred();
    return writeFile(path, buf.items);
}

// -- Byte reader -------------------------------------------------------------

const Reader = struct {
    b: []const u8,
    i: usize = 0,

    fn byte(r: *Reader) ?u8 {
        if (r.i + 1 > r.b.len) return null;
        const v = r.b[r.i];
        r.i += 1;
        return v;
    }
    fn u32v(r: *Reader) ?u32 {
        if (r.i + 4 > r.b.len) return null;
        const v = std.mem.readInt(u32, r.b[r.i..][0..4], .little);
        r.i += 4;
        return v;
    }
    fn u64v(r: *Reader) ?u64 {
        if (r.i + 8 > r.b.len) return null;
        const v = std.mem.readInt(u64, r.b[r.i..][0..8], .little);
        r.i += 8;
        return v;
    }
    fn take(r: *Reader, n: usize) ?[]const u8 {
        if (r.i + n > r.b.len) return null;
        const s = r.b[r.i .. r.i + n];
        r.i += n;
        return s;
    }
    fn skip(r: *Reader, n: usize) bool {
        if (r.i + n > r.b.len) return false;
        r.i += n;
        return true;
    }
    fn lenBytes(r: *Reader) ?[]const u8 {
        const len = r.u32v() orelse return null;
        return r.take(len);
    }
};

// -- Load: materialize + swizzle ---------------------------------------------

// Load-time relocation: id → materialized object. Objects are allocated in pass
// 1 (empty, in id order) so pass 2 can resolve heap-refs — including forward and
// cyclic ones — against a complete table.
var g_load_reloc: std.ArrayListUnmanaged(Oop) = .{};

const ObjRec = struct { obj: Oop, vt: Oop, nwords: u32, off: usize };
var g_load_objs: std.ArrayListUnmanaged(ObjRec) = .{};

fn importName(name: []const u8) Oop {
    const z = ca.dupeZ(u8, name) catch @panic("image: oom");
    defer ca.free(z);
    return g_api.import(z.ptr);
}

/// Advance past one encoded field without materializing it (pass 1). Sizes are
/// derivable from the tag alone, so no allocation is needed to find the next word.
fn skipField(r: *Reader) bool {
    const tag = r.byte() orelse return false;
    return switch (@as(Tag, @enumFromInt(tag))) {
        .nulloop, .nil, .true_, .false_ => true,
        .int, .float => r.skip(8),
        .heapref => r.skip(4),
        .symbol, .string => (r.lenBytes() != null),
    };
}

/// Materialize one encoded field into `out` (pass 2). Symbols re-intern by name;
/// heap-refs resolve against the relocation table. False on malformed input.
fn decodeField(r: *Reader, out: *Oop) bool {
    const tag = r.byte() orelse return false;
    switch (@as(Tag, @enumFromInt(tag))) {
        .nulloop => out.* = null,
        .nil => out.* = g_api.nilObj,
        .true_ => out.* = g_api.trueObj,
        .false_ => out.* = g_api.falseObj,
        .int => out.* = g_api.mkSmallInt(@bitCast(r.u64v() orelse return false)),
        .float => out.* = g_api.mkFloat(@bitCast(r.u64v() orelse return false)),
        .symbol => {
            const s = r.lenBytes() orelse return false;
            const z = ca.dupeZ(u8, s) catch @panic("image: oom");
            defer ca.free(z);
            out.* = g_api.intern(z.ptr);
        },
        .string => {
            const s = r.lenBytes() orelse return false;
            out.* = g_api.mkString(s.ptr, s.len);
        },
        .heapref => {
            const id = r.u32v() orelse return false;
            if (id >= g_load_reloc.items.len) return false;
            out.* = g_load_reloc.items[id];
        },
    }
    return true;
}

/// Rebuild the object graph from a `.heap` record. Pass 1 allocates every object
/// (empty, in id order, vtable relinked by name) and records its word offset;
/// pass 2 fills payloads — raw words copied, Oop words decoded, code words zeroed
/// (slice 4) — resolving heap-refs against the now-complete table.
fn loadHeap(payload: []const u8) bool {
    g_load_reloc.clearRetainingCapacity();
    g_load_objs.clearRetainingCapacity();
    var r = Reader{ .b = payload };
    const n = r.u32v() orelse return false;

    var idx: u32 = 0;
    while (idx < n) : (idx += 1) {
        const name = r.lenBytes() orelse return false;
        const nwords = r.u32v() orelse return false;
        const vt = importName(name);
        if (vt == null) return false; // a vtable the running system doesn't provide
        const obj = g_api.alloc(vt, nwords * WORD);
        g_load_reloc.append(ca, obj) catch @panic("image: oom");
        g_load_objs.append(ca, .{ .obj = obj, .vt = vt, .nwords = nwords, .off = r.i }) catch @panic("image: oom");
        var w: usize = 0;
        while (w < nwords) : (w += 1) {
            switch (g_api.slotKind(vt, w)) {
                idapi.slot_oop => if (!skipField(&r)) return false,
                idapi.slot_code => {},
                else => if (!r.skip(WORD)) return false,
            }
        }
    }

    for (g_load_objs.items) |rec| {
        var rr = Reader{ .b = payload, .i = rec.off };
        const base = @intFromPtr(rec.obj.?);
        var w: usize = 0;
        while (w < rec.nwords) : (w += 1) {
            const slot: *usize = @ptrFromInt(base + w * WORD);
            switch (g_api.slotKind(rec.vt, w)) {
                idapi.slot_oop => {
                    var val: Oop = undefined;
                    if (!decodeField(&rr, &val)) return false;
                    slot.* = @intFromPtr(val);
                },
                idapi.slot_code => slot.* = 0,
                else => slot.* = rr.u64v() orelse return false,
            }
        }
    }
    return true;
}

/// Rebuild the store's fact stream from a `.facts` record, resolving heap-refs
/// against the (already-loaded) heap. Wholesale replacement (docs/40 §3b).
fn loadFacts(payload: []const u8) bool {
    const store = g_api.import("Store") orelse return false;
    var r = Reader{ .b = payload };
    const tx = r.u64v() orelse return false;
    const kept = r.u32v() orelse return false;
    _ = g_api.send(store, g_api.intern("imageLoadBegin"), &[_]Oop{}, 0);
    const s_add = g_api.intern("imageAddFact:attr:value:tx:op:");
    var i: u32 = 0;
    while (i < kept) : (i += 1) {
        const op = r.byte() orelse return false;
        const ftx = r.u64v() orelse return false;
        var e: Oop = undefined;
        var a: Oop = undefined;
        var v: Oop = undefined;
        if (!decodeField(&r, &e) or !decodeField(&r, &a) or !decodeField(&r, &v)) return false;
        _ = g_api.send(store, s_add, &[_]Oop{ e, a, v, g_api.mkSmallInt(@intCast(ftx)), g_api.mkSmallInt(op) }, 5);
    }
    _ = g_api.send(store, g_api.intern("imageLoadEnd:raw:"), &[_]Oop{ g_api.mkSmallInt(@intCast(tx)), g_api.falseObj }, 2);
    return true;
}

/// Map an image back into the running system: validate the header, then dispatch
/// each record — heap precedes facts in the stream, so fact heap-refs resolve.
/// Auto-GC is off across the load (freshly materialized objects aren't rooted
/// until the store references them).
pub fn loadImage(path: []const u8) bool {
    const bytes = std.fs.cwd().readFileAlloc(ca, path, 1 << 30) catch return false;
    defer ca.free(bytes);
    g_api.gcSetAuto(0);
    defer g_api.gcSetAuto(1);
    var r = Reader{ .b = bytes };
    if (!readHeader(&r)) return false;
    while (r.i < r.b.len) {
        const kind = r.u32v() orelse return false;
        const len = r.u64v() orelse return false;
        const payload = r.take(@intCast(len)) orelse return false;
        switch (@as(RecordKind, @enumFromInt(kind))) {
            .heap => if (!loadHeap(payload)) return false,
            .facts => if (!loadFacts(payload)) return false,
            else => {}, // unknown/not-yet-handled record: skip by its framed length
        }
    }
    return true;
}

/// Validate an image's STRUCTURE — header + record framing — without mapping it
/// into a running system (no dispatch, no allocation, no `g_api`). The
/// format-level tests use this; production boot uses `loadImage`.
pub fn validate(path: []const u8) bool {
    const bytes = std.fs.cwd().readFileAlloc(ca, path, 1 << 30) catch return false;
    defer ca.free(bytes);
    var r = Reader{ .b = bytes };
    if (!readHeader(&r)) return false;
    while (r.i < r.b.len) {
        _ = r.u32v() orelse return false; // kind
        const len = r.u64v() orelse return false;
        _ = r.take(@intCast(len)) orelse return false;
    }
    return true;
}

/// Validate the header: magic, format version, byte order. The ABI/arch fields
/// are recorded but not fatal here (slice 4 uses them to arbitrate map-vs-replay);
/// step-3 images carry no compiled sections, so any same-format header loads.
fn readHeader(r: *Reader) bool {
    const magic = r.take(MAGIC.len) orelse return false;
    if (!std.mem.eql(u8, magic, MAGIC)) return false;
    if ((r.u32v() orelse return false) != FORMAT_VERSION) return false;
    if ((r.u32v() orelse return false) != ENDIAN_TAG) return false;
    _ = r.u32v() orelse return false; // arch — checked in slice 4
    _ = r.u64v() orelse return false; // abi hash — checked in slice 4
    return true;
}

// -- birnam surface: [System saveImage:] / [System loadImage:] ---------------

fn pathArg(o: Oop, len: *usize) ?[*]const u8 {
    if (o == null or g_api.vtableOf(o) != g_api.string_vt) return null;
    return g_api.stringBytes(o, len);
}

/// `[System saveImage: path]` — serialize the running system to `path`; self on
/// success, nil on failure (bad path arg or write error).
fn sysSaveImage(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var len: usize = 0;
    const p = pathArg(a[0], &len) orelse return g_api.nilObj;
    return if (saveImage(p[0..len])) self else g_api.nilObj;
}

/// `[System loadImage: path]` — map an image back in; self on success, nil on
/// failure. `birnam-id foo.image` uses this on boot.
fn sysLoadImage(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var len: usize = 0;
    const p = pathArg(a[0], &len) orelse return g_api.nilObj;
    return if (loadImage(p[0..len])) self else g_api.nilObj;
}

// -- bringup -----------------------------------------------------------------

pub fn bringUp(api: *IdApi) void {
    g_api = api;
    const system_vt = api.import("SystemVt");
    api.method(system_vt, api.intern("saveImage:"), &sysSaveImage);
    api.method(system_vt, api.intern("loadImage:"), &sysLoadImage);
    // The fact collector: a proto (permanent → safe to hold module-globally) whose
    // one method the store drives during `imageFactsInto:`.
    g_collector_vt = api.proto(api.object_vt);
    api.method(g_collector_vt, api.intern("e:a:v:tx:op:"), &mCollect);
}

fn moduleInit(api: *idapi.IdApi, state: ?*anyopaque) callconv(.c) ?*anyopaque {
    _ = state;
    bringUp(api);
    return null;
}
comptime {
    if (!@import("builtin").is_test) @export(&moduleInit, .{ .name = "module_init" });
}
