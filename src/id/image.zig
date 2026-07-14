//! The image — the running system made durable (docs/40).
//!
//! An image is a live native system made durable: its compiled units are mapped
//! back as the primary executable layer, while durable store state and selected
//! reflection facts restore the live system around them. Source remains attached
//! for from-zero bootstrap and explicit recovery, never as the normal boot path.
//! The store is a meta-level capability surface and durable-data substrate, not
//! a second authority for compiled behavior.
//!
//! Layering (docs/40 §3): the STORE owns its facts and yields/accepts them raw
//! (`imageFactsInto:` / `imageLoadBegin`+`imageAddFact:…`+`imageLoadEnd:raw:`);
//! the FLOOR (id) exposes the heap primitives (`objectPayloadSize`,
//! `exportedNameOf`, `slotKind`); and THIS module owns the codec (leaf + symbol
//! + heap-ref), the relocation map, the heap walk, and the container format.
//!
//! On-disk format — a header then one sealed generation's framed records:
//!   header: magic · format · endian · arch · abi_hash        (28 bytes)
//!   record: u32 kind · u64 length · payload
//! Record kinds this slice writes, HEAP BEFORE FACTS (facts reference heap ids):
//!   .authority 64-byte current generation digest
//!   .heap   u32 n · n×(name · u32 nwords · words)  — the object graph
//!   .facts  u64 tx · u32 kept · kept×(u8 op · u64 tx · e · a · v)
//!   .code   u64 unit_hash · 64-byte generation digest · dylib bytes
//! A payload field is a tagged value: nil/true/false, int, float, symbol (by
//! name — the durable anchor), string (bytes), heapref (a u32 relocation id), or
//! a null Oop slot. A durable fact that cannot encode MUST have a registered
//! restoration story (mapped code or recorded global source) or the save rejects
//! the image before publication. Vtables relink BY NAME on load (the D5.1 spine):
//! a native module's freshly-loaded vtable re-adopts its image objects.

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
pub const FORMAT_VERSION: u32 = 4; // v4: one generation authority bound to one code unit
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
const RUNTIME_ABI_VERSION: u32 = 2;
const shim_names = [_][]const u8{
    "birnam_send",                       "birnam_intern",        "birnam_mkSmallInt",
    "birnam_smallIntValue",              "birnam_mkFloat",       "birnam_floatValue",
    "birnam_mkString",                   "birnam_nil",           "birnam_true",
    "birnam_false",                      "birnam_objectVt",      "birnam_proto",
    "birnam_method",                     "birnam_alloc",         "birnam_cstr",
    "birnam_free_cstr",                  "birnam_layoutAllOops", "birnam_global",
    "birnam_candidate_validate_binding",
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
    code = 4, // compiled units (primary executable layer)
    authority = 5, // canonical current-generation digest
    _,
};

/// A serialized value. `heapref` is a relocation id into the heap section;
/// `nulloop` is a null payload slot (distinct from the `nil` singleton);
/// `exportref` is a by-name reference to an EXPORTED object (a prototype, a
/// vtable like `StringVt`, a module facade) — the same relink-by-name spine
/// vtables ride (docs/40 §3c). Serializing these by name instead of by copy
/// keeps their identity: the loaded fact resolves to the LIVE native object,
/// so `[[Reflect vtableOf: x] == StringExemplarVt]`-style comparisons hold
/// across a save/boot, and a well-known object is never shadowed by a
/// materialized copy of itself.
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
    exportref = 9,
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

/// Publish a complete image in one rename. A rejected or failed save never
/// truncates a previously valid image at `path`.
fn writeFileAtomic(path: []const u8, bytes: []const u8) bool {
    const tmp = std.fmt.allocPrint(ca, "{s}.tmp-{d}", .{ path, std.c.getpid() }) catch return false;
    defer ca.free(tmp);
    std.fs.cwd().deleteFile(tmp) catch {};
    const file = std.fs.cwd().createFile(tmp, .{}) catch return false;
    var keep = false;
    defer {
        file.close();
        if (!keep) std.fs.cwd().deleteFile(tmp) catch {};
    }
    file.writeAll(bytes) catch return false;
    std.fs.cwd().rename(tmp, path) catch return false;
    keep = true;
    return true;
}

/// Append `bytes` to the end of an existing file (the journal write; docs/40 §7.4).
/// The format is self-delimiting records read to EOF, so this needs no rewrite.
fn appendToFile(path: []const u8, bytes: []const u8) bool {
    const file = std.fs.cwd().openFile(path, .{ .mode = .write_only }) catch return false;
    defer file.close();
    file.seekFromEnd(0) catch return false;
    file.writeAll(bytes) catch return false;
    return true;
}

/// Write a header-only (empty but valid) image — the pure format floor, no
/// running system needed (the format-level tests build images this way).
pub fn writeHeaderOnly(path: []const u8) bool {
    var buf: Buf = .{};
    defer buf.deinit(ca);
    writeHeader(&buf);
    const at = beginRecord(&buf, .authority);
    var empty: [64]u8 = [_]u8{'0'} ** 64;
    putBytes(&buf, &empty);
    endRecord(&buf, at);
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
var g_recovered_facts: u32 = 0;
var g_rejected_facts: u32 = 0;
var g_recovered_slots: u32 = 0;
var g_rejected_slots: u32 = 0;
var g_recoveries_needing_code: u32 = 0;
var g_slots_needing_code: u32 = 0;
var g_code_captured: bool = false;
var g_collector_vt: Oop = null;
var g_globals_entity: Oop = null;
var g_globalsrc_entity: Oop = null;

const ReportStatus = enum { never, saved, rejected, io_failed };
const SaveReport = struct {
    status: ReportStatus = .never,
    saved_facts: u32 = 0,
    recovered_facts: u32 = 0,
    rejected_facts: u32 = 0,
    recovered_slots: u32 = 0,
    rejected_slots: u32 = 0,
    code_units: u32 = 0,
    detail: [256]u8 = undefined,
    detail_len: usize = 0,
};
var g_save_report: SaveReport = .{};
var g_report_vt: Oop = null;
var g_report_obj: Oop = null;

fn resetSaveReport() void {
    g_save_report = .{};
}

fn setSaveReportDetail(comptime fmt: []const u8, args: anytype) void {
    const msg = std.fmt.bufPrint(&g_save_report.detail, fmt, args) catch {
        const fallback = "image save rejected";
        @memcpy(g_save_report.detail[0..fallback.len], fallback);
        g_save_report.detail_len = fallback.len;
        return;
    };
    g_save_report.detail_len = msg.len;
}

// Journaling state (docs/40 §7.4: one format, two write modes). Survives ACROSS
// saves within a process — so an `appendImage` continues the base's object→id
// numbering (`g_reloc`/`g_objs` are NOT reset by an append), writing only the
// delta. A full `saveImage` (the compactor) resets and becomes the new base; a
// successful load also seeds these from the mapped image so a booted app can then
// append. `-1` basis = no base established in this process yet.
var g_saved_basis: i64 = -1; // basis tx captured at the last save/append/load
var g_min_tx: i64 = 0; // collector filter: skip facts with tx < this (append = delta only)
var g_delta_has_code_source: bool = false; // a delta touched a portable code-source entity → emit composing `.code`
var g_methods_entity: Oop = null; // interned `__methods__`, for that detection
var g_methodops_entity: Oop = null; // ordered post-cutover installs/removals

/// Reset the save worklist for a FULL save (a fresh base / compaction). An append
/// deliberately does NOT call this — it reuses the base's numbering.
fn resetSaveState() void {
    g_reloc.clearRetainingCapacity();
    g_objs.clearRetainingCapacity();
    g_kept = 0;
    g_recovered_facts = 0;
    g_rejected_facts = 0;
    g_recovered_slots = 0;
    g_rejected_slots = 0;
    g_recoveries_needing_code = 0;
    g_slots_needing_code = 0;
    g_code_captured = false;
}

const DeferredFactClass = enum { source, code, reject };

fn isSymbol(o: Oop) bool {
    return o != null and g_api.vtableOf(o) == g_api.symbol_vt;
}

fn isInternalGlobal(attr: Oop) bool {
    if (!isSymbol(attr)) return false;
    return std.mem.startsWith(u8, std.mem.span(g_api.symbolName(attr)), "__");
}

/// A top-level binding with recorded RHS source can be re-created after its
/// compiled image units load. This is recovery for a closure value, not the
/// normal representation of compiled methods.
fn hasGlobalRecoverySource(attr: Oop) bool {
    const store = g_api.import("Store") orelse return false;
    return g_api.send(store, g_api.intern("hasValue:attr:"), &[_]Oop{ g_globalsrc_entity, attr }, 2) == g_api.trueObj;
}

/// A module's complete current document may establish a named runtime object
/// (for example, a PEG grammar). The image compiler consumes the generation's
/// source map, so recovery classification must inspect that same input.
fn hasModuleGlobalRecoverySource(attr: Oop) bool {
    if (!isSymbol(attr)) return false;
    const codegen = g_api.import("Codegen") orelse return false;
    const name = std.mem.span(g_api.symbolName(attr));
    const marker = std.fmt.allocPrint(ca, "Globals at: #{s} put:", .{name}) catch return false;
    defer ca.free(marker);
    const sources = g_api.send(codegen, g_api.intern("generationSources"), &.{}, 0);
    const error_vt = g_api.import("ErrorVt");
    if (sources == g_api.nilObj or (error_vt != null and g_api.vtableOf(sources) == error_vt)) return false;
    const count = g_api.smallIntValue(g_api.send(sources, g_api.intern("size"), &.{}, 0));
    var i: i64 = 0;
    while (i < count) : (i += 1) {
        const src = g_api.send(sources, g_api.intern("at:"), &.{g_api.mkSmallInt(i)}, 1);
        if (src == null or src == g_api.nilObj or g_api.vtableOf(src) != g_api.string_vt) continue;
        var len: usize = 0;
        const p = g_api.stringBytes(src, &len);
        if (std.mem.indexOf(u8, p[0..len], marker) != null) return true;
    }
    return false;
}

/// Method-mirror facts are a projection of live vtable bindings. The image's
/// compiled unit re-installs that authoritative binding on boot, so this fact
/// is reconstructable when a code unit is captured; it is never a second copy
/// of executable behavior.
fn isMethodMirrorFact(e: Oop, a: Oop) bool {
    return e != null and a != null and g_api.vtableOf(e) == g_api.vtable_vt and isSymbol(a);
}

fn deferredFactClass(e: Oop, a: Oop) DeferredFactClass {
    if (e == g_globals_entity) {
        if (isInternalGlobal(a)) return .code; // __mblk / __amblk and other codegen-owned bindings
        if (hasGlobalRecoverySource(a)) return .source;
        if (hasModuleGlobalRecoverySource(a)) return .code;
        return .reject;
    }
    if (isMethodMirrorFact(e, a)) return .code;
    return .reject;
}

fn noteRejectedFact(e: Oop, a: Oop) void {
    g_rejected_facts += 1;
    if (g_save_report.detail_len != 0) return;
    if (e == g_globals_entity and isSymbol(a)) {
        setSaveReportDetail("unserializable durable global #{s}", .{std.mem.span(g_api.symbolName(a))});
    } else {
        setSaveReportDetail("unserializable durable store fact has no restoration recipe", .{});
    }
}

fn noteDeferredFact(e: Oop, a: Oop) void {
    switch (deferredFactClass(e, a)) {
        .source => g_recovered_facts += 1,
        .code => {
            g_recovered_facts += 1;
            g_recoveries_needing_code += 1;
        },
        .reject => noteRejectedFact(e, a),
    }
}

fn rejectCodeRecoveries() void {
    if (g_code_captured) return;
    if (g_recoveries_needing_code > 0) {
        g_recovered_facts -= g_recoveries_needing_code;
        g_rejected_facts += g_recoveries_needing_code;
    }
    if (g_slots_needing_code > 0) {
        g_recovered_slots -= g_slots_needing_code;
        g_rejected_slots += g_slots_needing_code;
    }
    if (g_recoveries_needing_code == 0 and g_slots_needing_code == 0) return;
    if (g_save_report.detail_len == 0)
        setSaveReportDetail(
            "{d} fact(s) and {d} heap slot(s) require a compiled image unit, but none was captured",
            .{ g_recoveries_needing_code, g_slots_needing_code },
        );
}

fn finishSaveReport(status: ReportStatus) void {
    g_save_report.status = status;
    g_save_report.saved_facts = g_kept;
    g_save_report.recovered_facts = g_recovered_facts;
    g_save_report.rejected_facts = g_rejected_facts;
    g_save_report.recovered_slots = g_recovered_slots;
    g_save_report.rejected_slots = g_rejected_slots;
}

/// The vtable to serialize `o` under, or null to defer it. Normally its own
/// (exported) vtable. An object that OWNS a private vtable — a module singleton
/// specialised by ensurePrivateVtable the moment a method landed on it — is the
/// exception: that vtable is DERIVED state, re-minted at boot by the replayed
/// method installs (`ensurePrivateVtable(birnam_global("X"))` in the mapped
/// `.code`), so the object serializes under the private vtable's exported parent
/// instead of deferring — and its Globals binding survives the image. One hop
/// suffices: ensurePrivateVtable never stacks two private vtables on one owner.
/// A vtable that is unexported and NOT owned by `o` (a block's; a clone sharing
/// a singleton's family) has no rebuild story and still defers.
fn serialVtable(o: Oop) ?Oop {
    const vt = g_api.vtableOf(o);
    if (vt == null) return null;
    if (g_api.exportedNameOf(vt) != null) return vt;
    if (g_api.vtableOwner(vt) == o) {
        const p = g_api.vtableParent(vt);
        if (p != null and g_api.exportedNameOf(p) != null) return p;
    }
    return null;
}

/// Register a compound object for serialization, assigning it a relocation id
/// (before walking its slots, so cycles terminate). Returns null — DEFERRING the
/// object — when it has no serializable vtable (image-born with no rebuild story:
/// a block vtable, slice 4's replay job) or a slot lacks a layout descriptor
/// (can't be safely swizzled).
fn internObj(o: Oop) ?u32 {
    if (g_reloc.get(@intFromPtr(o.?))) |id| return id;
    const vt = serialVtable(o) orelse return null;
    // EVERY payload word must have a KNOWN layout kind (§G3). An unknown slot is
    // dumped as a raw word by `drainHeapFrom`, and if it actually holds a live Oop
    // that raw word is a dangling pointer on load. The old guard inspected only
    // slot 0, so a partially-described layout (slot 0 known, a later slot unknown)
    // slipped through — scan the whole payload and defer the object if ANY slot is
    // unknown (slice-4 replay re-derives it).
    const nwords = g_api.objectPayloadSize(o) / WORD;
    var w: usize = 0;
    while (w < nwords) : (w += 1) {
        if (g_api.slotKind(vt, w) == idapi.slot_unknown) return null;
    }
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
    // Worker handles are process capabilities, not durable values. Reject a
    // Store-reachable handle explicitly; serializing its registry token would
    // create a stale capability on the next boot, while silently omitting the
    // fact would corrupt durable state.
    if (g_api.isExported("IsolateHandleVt") != 0 and vt == g_api.import("IsolateHandleVt")) {
        setSaveReportDetail("nonDurableIsolateHandle: durable Store value refers to a worker handle", .{});
        return false;
    }
    if (vt == g_api.symbol_vt) {
        putTag(buf, .symbol);
        putLenBytes(buf, std.mem.span(g_api.symbolName(o)));
        return true;
    }
    // An exported object serializes BY NAME (identity over copy — see Tag). This
    // must precede the string/compound paths: the String prototype is an exported
    // string, and exported vtables/no-layout singletons would otherwise defer.
    if (g_api.exportedNameOf(o)) |nm| {
        putTag(buf, .exportref);
        putLenBytes(buf, std.mem.span(nm));
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
/// fact is atomic); otherwise its recovery contract is classified. A missing
/// contract rejects the image rather than publishing a partial fact stream.
fn mCollect(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const tx = g_api.smallIntValue(args[3]);
    if (tx < g_min_tx) return g_api.nilObj; // already in the base image — not this delta
    var scratch: Buf = .{};
    defer scratch.deinit(ca);
    scratch.append(ca, @intCast(g_api.smallIntValue(args[4]))) catch @panic("image: oom"); // op
    putU64(&scratch, @intCast(tx)); // tx
    if (encodeField(&scratch, args[0]) and encodeField(&scratch, args[1]) and encodeField(&scratch, args[2])) {
        g_facts_buf.appendSlice(ca, scratch.items) catch @panic("image: oom");
        g_kept += 1;
    } else noteDeferredFact(args[0], args[1]);
    return g_api.nilObj;
}

/// Drain the object worklist into the `.heap` payload: each object as its vtable
/// NAME (to relink by), its word count, and its payload words — raw words copied
/// verbatim, Oop words encoded (which discovers more objects → the worklist
/// grows). Code words are zero-width here because primary compiled units restore
/// them. The leading u32 count is backpatched to the final total.
fn drainHeap(buf: *Buf) void {
    drainHeapFrom(buf, 0);
}

/// Drain worklist objects with id >= `start` into a `.heap` payload. A full save
/// passes 0; an append passes the base's object count, so only objects NEW to the
/// delta are written — objects already in the base keep their base ids and are
/// referenced (not re-serialized). The walk still DISCOVERS nested objects (the
/// worklist grows past its initial end), and those are new too, so they are
/// included. The leading u32 is the count written by THIS record.
fn drainHeapFrom(buf: *Buf, start: usize) void {
    const count_at = buf.items.len;
    putU32(buf, 0);
    var i: usize = start;
    while (i < g_objs.items.len) : (i += 1) {
        const o = g_objs.items[i];
        const vt = serialVtable(o).?; // interned ⇒ serializable (same walk as internObj)
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
                        g_rejected_slots += 1;
                        if (g_save_report.detail_len == 0)
                            setSaveReportDetail("a durable heap object contains an unserializable nested value", .{});
                    }
                },
                // Code pointers are restored by the primary compiled units, not
                // by swizzling a process-local address into the image heap.
                idapi.slot_code => {
                    g_recovered_slots += 1;
                    g_slots_needing_code += 1;
                },
                idapi.slot_raw => putU64(buf, word.*),
                // slot_unknown can't reach here: internObj (§G3) defers any object
                // with an unknown slot whole, so a live Oop is never dumped raw.
                else => unreachable,
            }
        }
    }
    std.mem.writeInt(u32, buf.items[count_at..][0..4], @intCast(g_objs.items.len - start), .little);
}

/// A per-process temp path for the code section (pid-tagged so concurrent boots
/// and dlopen's path cache don't collide). `suffix` distinguishes save vs load.
fn codeTempPath(bufz: []u8, suffix: []const u8) [:0]const u8 {
    return std.fmt.bufPrintZ(bufz, "/tmp/birnam-imgcode-{d}-{s}.dylib", .{ std.c.getpid(), suffix }) catch unreachable;
}

const CodeCapture = enum { absent, captured, failed };

fn reconcileCompiledUnits() bool {
    const codegen = g_api.import("Codegen") orelse return true;
    const result = g_api.send(codegen, g_api.intern("reconcileAllCandidates"), &[_]Oop{}, 0);
    const error_vt = g_api.import("ErrorVt");
    return result != g_api.nilObj and (error_vt == null or g_api.vtableOf(result) != error_vt);
}

fn generationDigest(codegen: Oop, out: *[64]u8) bool {
    const digest = g_api.send(codegen, g_api.intern("currentGenerationDigest"), &.{}, 0);
    const error_vt = g_api.import("ErrorVt");
    if (digest == null or digest == g_api.nilObj or
        (error_vt != null and g_api.vtableOf(digest) == error_vt) or
        g_api.vtableOf(digest) != g_api.string_vt) return false;
    var len: usize = 0;
    const bytes = g_api.stringBytes(digest, &len);
    if (len != out.len) return false;
    @memcpy(out, bytes[0..out.len]);
    return true;
}

/// Capture the one primary compiled unit and bind it to the exact generation
/// authority from which it was built.
fn captureCode(buf: *Buf, digest: *[64]u8) CodeCapture {
    const codegen = g_api.import("Codegen") orelse return .absent;
    if (!generationDigest(codegen, digest)) return .absent;
    var pbuf: [128]u8 = undefined;
    const tmp = codeTempPath(&pbuf, "save");
    const r = g_api.send(codegen, g_api.intern("compileImageCode:"), &[_]Oop{g_api.mkString(tmp.ptr, tmp.len)}, 1);
    if (r == null or r == g_api.nilObj) return .absent;
    defer std.fs.cwd().deleteFile(tmp) catch {};
    const bytes = std.fs.cwd().readFileAlloc(ca, tmp, 1 << 30) catch return .failed;
    defer ca.free(bytes);
    const at = beginRecord(buf, .code);
    putU64(buf, std.hash.Wyhash.hash(0, bytes));
    putBytes(buf, digest);
    putBytes(buf, bytes);
    endRecord(buf, at);
    return .captured;
}

/// Serialize the live system to `path`: first capture its primary compiled unit,
/// then collect durable data and selected meta-state. The completed image is
/// published with one rename only after every durable value has an encoding or a
/// registered recovery path.
pub fn saveImage(path: []const u8) bool {
    g_api.gcSetAuto(0);
    defer g_api.gcSetAuto(1);
    resetSaveState();
    resetSaveReport();
    if (!reconcileCompiledUnits()) {
        setSaveReportDetail("managed dispatch has unlogged non-portable drift", .{});
        finishSaveReport(.rejected);
        return false;
    }
    g_min_tx = 0; // a full save carries EVERY fact (the compacted base)
    g_delta_has_code_source = false;

    // Capture code BEFORE classifying reconstructed facts: method mirrors and
    // codegen-owned globals are admissible only when this unit exists.
    var code_buf: Buf = .{};
    defer code_buf.deinit(ca);
    var authority_digest: [64]u8 = [_]u8{'0'} ** 64;
    const code_capture = captureCode(&code_buf, &authority_digest);
    g_code_captured = code_capture == .captured;
    if (g_code_captured) g_save_report.code_units = 1;
    if (code_capture == .failed) {
        g_rejected_facts += 1;
        setSaveReportDetail("failed to capture the primary compiled image unit", .{});
    }

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
    rejectCodeRecoveries();

    if (g_rejected_facts > 0 or g_rejected_slots > 0) {
        finishSaveReport(.rejected);
        // The existing append baseline may no longer describe the current heap
        // after a rejected compaction attempt. Refuse later appendImage: calls
        // until a complete save establishes a new base.
        g_saved_basis = -1;
        resetSaveState();
        return false;
    }

    // 3. Assemble the file: heap BEFORE facts (facts reference heap ids).
    var buf: Buf = .{};
    defer buf.deinit(ca);
    writeHeader(&buf);
    const authority_at = beginRecord(&buf, .authority);
    putBytes(&buf, &authority_digest);
    endRecord(&buf, authority_at);
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
    // 4. Primary compiled unit — mapped on normal boot, never replayed here.
    putBytes(&buf, code_buf.items);
    if (!writeFileAtomic(path, buf.items)) {
        if (g_save_report.detail_len == 0)
            setSaveReportDetail("could not atomically publish image at {s}", .{path});
        finishSaveReport(.io_failed);
        g_saved_basis = -1;
        resetSaveState();
        return false;
    }
    finishSaveReport(.saved);
    return true;
}

/// `[System appendImage: path]` — the journal write mode (docs/40 §7.4). Append to
/// an EXISTING image only the state changed since the last save/append/load in this
/// process: facts with `tx >= g_saved_basis`, plus a `.heap` record for objects
/// newly reachable from them (base objects keep their ids and are referenced, not
/// rewritten). The on-disk format is a record stream read to EOF, so appending is
/// literally writing more records — no rewrite. Returns false if no base is known
/// (call `saveImage:` first) or on an I/O error. A delta that (re)defines methods
/// re-emits a composing `.code` record (§G2) so the appended image is self-consistent
/// — the appended install overrides the base's stale method on load, no `--rebuild`
/// or compacting `saveImage:` required.
fn appendImageLegacy(path: []const u8) bool {
    if (g_saved_basis < 0) {
        resetSaveReport();
        setSaveReportDetail("append requires a successful full save or image load first", .{});
        finishSaveReport(.rejected);
        return false;
    }
    g_api.gcSetAuto(0);
    defer g_api.gcSetAuto(1);
    // Deliberately NOT resetSaveState — reuse the base's object→id numbering.
    const base_obj_count = g_objs.items.len;
    g_kept = 0;
    g_recovered_facts = 0;
    g_rejected_facts = 0;
    g_recovered_slots = 0;
    g_rejected_slots = 0;
    g_recoveries_needing_code = 0;
    g_slots_needing_code = 0;
    g_code_captured = false;
    resetSaveReport();
    if (!reconcileCompiledUnits()) {
        setSaveReportDetail("managed dispatch has unlogged non-portable drift", .{});
        finishSaveReport(.rejected);
        rollbackAppend(base_obj_count);
        return false;
    }
    g_delta_has_code_source = false;
    g_min_tx = g_saved_basis;

    var facts_recs: Buf = .{};
    defer facts_recs.deinit(ca);
    var heap_buf: Buf = .{};
    defer heap_buf.deinit(ca);

    const store = g_api.import("Store");
    var tx: i64 = g_saved_basis;
    if (store) |st| {
        tx = g_api.smallIntValue(g_api.send(st, g_api.intern("basis"), &[_]Oop{}, 0));
        g_facts_buf = &facts_recs;
        const collector = g_api.alloc(g_collector_vt, 0);
        _ = g_api.send(st, g_api.intern("imageFactsInto:"), &[_]Oop{collector}, 1);
    }
    drainHeapFrom(&heap_buf, base_obj_count); // only objects new to this delta

    // A delta that changes recorded method or module source gets a composing
    // primary unit. Its mirror facts may rely on that unit, so capture it before
    // validating recoverability.
    var code_buf: Buf = .{};
    defer code_buf.deinit(ca);
    if (g_delta_has_code_source) {
        var ignored_digest: [64]u8 = undefined;
        const code_capture = captureCode(&code_buf, &ignored_digest);
        g_code_captured = code_capture == .captured;
        if (g_code_captured) g_save_report.code_units = 1 else if (code_capture == .failed) {
            g_rejected_facts += 1;
            setSaveReportDetail("failed to capture the primary compiled unit for an image delta", .{});
        }
    }
    rejectCodeRecoveries();
    if (g_rejected_facts > 0 or g_rejected_slots > 0) {
        finishSaveReport(.rejected);
        rollbackAppend(base_obj_count);
        return false;
    }

    // Assemble the delta records (heap before facts, same as a base generation).
    var buf: Buf = .{};
    defer buf.deinit(ca);
    if (g_objs.items.len > base_obj_count) {
        const at = beginRecord(&buf, .heap);
        putBytes(&buf, heap_buf.items);
        endRecord(&buf, at);
    }
    if (g_kept > 0) {
        const at = beginRecord(&buf, .facts);
        putU64(&buf, @bitCast(tx));
        putU32(&buf, g_kept);
        putBytes(&buf, facts_recs.items);
        endRecord(&buf, at);
    }
    // A code-source-changing delta carries a composing primary unit. On load,
    // units compose in stream order and the latest install wins.
    putBytes(&buf, code_buf.items);
    if (buf.items.len > 0 and !appendToFile(path, buf.items)) {
        if (g_save_report.detail_len == 0)
            setSaveReportDetail("could not append image delta at {s}", .{path});
        finishSaveReport(.io_failed);
        rollbackAppend(base_obj_count);
        return false;
    }
    g_saved_basis = tx; // advance the watermark for the next append
    finishSaveReport(.saved);
    return true;
}

fn rollbackAppend(base_obj_count: usize) void {
    while (g_objs.items.len > base_obj_count) {
        const o = g_objs.pop().?;
        if (o) |p| _ = g_reloc.remove(@intFromPtr(p));
    }
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

// Set by the most recent load: whether a `.code` section was MAPPED (methods
// installed with no recompile). The header's arch + ABI-validity hash, captured
// by readHeader, gate that — a mismatch (or --rebuild) leaves it false so the
// caller falls back to replay/stdlib (docs/40 §4). Reset per load.
var g_mapped_code: bool = false;
var g_loaded_code_units: u32 = 0;
var g_code_composition: u64 = 0;
var g_img_format: u32 = FORMAT_VERSION;
var g_img_arch: u32 = 0;
var g_img_abi: u64 = 0;
var g_force_rebuild: bool = false; // set by [System loadImage:rebuild:] / --rebuild
var g_authority_digest: [64]u8 = undefined;
var g_has_authority: bool = false;

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
        .symbol, .string, .exportref => (r.lenBytes() != null),
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
        .exportref => {
            // Resolve to the LIVE exported object of that name — same failure
            // contract as an unknown vtable name in loadHeap: a runtime that no
            // longer provides the export can't load the image.
            const s = r.lenBytes() orelse return false;
            const o = importName(s);
            if (o == null) return false;
            out.* = o;
        },
    }
    return true;
}

/// Rebuild the object graph from a `.heap` record. Pass 1 allocates every object
/// (empty, in id order, vtable relinked by name) and records its word offset;
/// pass 2 fills payloads — raw words copied, Oop words decoded, code words zeroed
/// (slice 4) — resolving heap-refs against the now-complete table.
fn loadHeap(payload: []const u8) bool {
    // `g_load_reloc` is CUMULATIVE across `.heap` records in one load (ids are
    // global — a later generation's objects reference earlier ones by their base
    // id), so it is cleared once per load in `loadImage`, not here. `g_load_objs`
    // is per-record scratch for pass 2, so it IS cleared each call.
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

/// Apply one `.facts` record to the store, resolving heap-refs against the (already
/// loaded) heap. CUMULATIVE across records (docs/40 §7.4): the store is cleared
/// once (on the first record, via `imageLoadBegin`), every record's facts are then
/// appended, and the counters are restored once after the last record — so a base
/// generation followed by appended deltas rebuilds the whole fact timeline. Tracks
/// the highest basis seen in `max_basis`; sets `began` after the first clear.
fn loadFacts(payload: []const u8, began: *bool, max_basis: *i64) bool {
    const store = g_api.import("Store") orelse return false;
    var r = Reader{ .b = payload };
    const tx: i64 = @intCast(r.u64v() orelse return false);
    const kept = r.u32v() orelse return false;
    if (!began.*) {
        _ = g_api.send(store, g_api.intern("imageLoadBegin"), &[_]Oop{}, 0);
        began.* = true;
    }
    if (tx > max_basis.*) max_basis.* = tx;
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
    return true;
}

/// Map the `.code` section (docs/40 slice 4): write the embedded relocatable
/// artifact to a temp file and `[Codegen load:]` it, which installs every method
/// with NO recompile (the default boot). Skipped when the code can't be trusted
/// (arch/ABI mismatch or --rebuild) — the caller then replays from source. Sets
/// `g_mapped_code` on success. Returns false only on an actual load error.
var g_code_load_seq: u32 = 0; // distinguishes multiple `.code` records in one load

fn loadCode(payload: []const u8) bool {
    if (!g_has_authority or payload.len < 72) return false;
    const checksum = std.mem.readInt(u64, payload[0..8], .little);
    g_code_composition = std.hash.Wyhash.hash(g_code_composition, std.mem.asBytes(&checksum));
    if (!codeMappable()) return true; // not an error — leave it to replay/stdlib
    const unit = codeUnitBytes(payload, &g_authority_digest) orelse return false;
    const codegen = g_api.import("Codegen") orelse return false;
    // Per-record temp path: an image may carry more than one `.code` record (a base
    // plus appended code); each dlopen+install composes, but they must not collide.
    var sbuf: [16]u8 = undefined;
    const suffix = std.fmt.bufPrint(&sbuf, "load{d}", .{g_code_load_seq}) catch return false;
    g_code_load_seq += 1;
    var pbuf: [128]u8 = undefined;
    const tmp = codeTempPath(&pbuf, suffix);
    if (!writeFile(tmp, unit)) return false;
    const r = g_api.send(codegen, g_api.intern("load:"), &[_]Oop{g_api.mkString(tmp.ptr, tmp.len)}, 1);
    std.fs.cwd().deleteFile(tmp) catch {};
    if (r == null or r == g_api.nilObj or
        (g_api.isExported("ErrorVt") != 0 and g_api.vtableOf(r) == g_api.import("ErrorVt"))) return false;
    g_mapped_code = true;
    g_loaded_code_units += 1;
    return true;
}

/// Code records carry both a byte-integrity checksum and the generation digest
/// which authorized their construction. There is deliberately no old-format
/// recovery path.
fn codeUnitBytes(payload: []const u8, authority: *const [64]u8) ?[]const u8 {
    if (payload.len < 72) return null;
    const want = std.mem.readInt(u64, payload[0..8], .little);
    if (!std.mem.eql(u8, payload[8..72], authority)) return null;
    const unit = payload[72..];
    if (std.hash.Wyhash.hash(0, unit) != want) return null;
    return unit;
}

fn loadedGenerationMatchesAuthority() bool {
    if (!g_has_authority) return false;
    const codegen = g_api.import("Codegen") orelse return false;
    var actual: [64]u8 = undefined;
    return generationDigest(codegen, &actual) and std.mem.eql(u8, &actual, &g_authority_digest);
}

/// Map an image back into the running system: validate the header, then dispatch
/// each record — heap precedes facts in the stream, so fact heap-refs resolve.
/// Auto-GC is off across the load (freshly materialized objects aren't rooted
/// until the store references them). `g_mapped_code` reports whether the compiled
/// `.code` section was mapped (so the caller can skip stdlib / replay).
pub fn loadImage(path: []const u8) bool {
    const bytes = std.fs.cwd().readFileAlloc(ca, path, 1 << 30) catch return false;
    defer ca.free(bytes);
    g_api.gcSetAuto(0);
    defer g_api.gcSetAuto(1);
    const codegen = g_api.import("Codegen");
    if (codegen) |cg| _ = g_api.send(cg, g_api.intern("beginImageDispatchRecovery"), &[_]Oop{}, 0);
    defer {
        if (codegen) |cg| _ = g_api.send(cg, g_api.intern("endImageDispatchRecovery"), &[_]Oop{}, 0);
    }
    g_mapped_code = false;
    g_loaded_code_units = 0;
    g_code_composition = 0;
    g_code_load_seq = 0;
    g_has_authority = false;
    g_load_reloc.clearRetainingCapacity(); // ids are global across this load's records
    var facts_began = false;
    var max_basis: i64 = 0;
    var code_records: std.ArrayList([]const u8) = .{};
    defer code_records.deinit(ca);
    var r = Reader{ .b = bytes };
    if (!readHeader(&r)) return false;
    // Verify every record boundary and compiled-unit manifest before touching
    // the store. A corrupt primary unit must not leave a partially restored live
    // system behind merely because `.facts` appears earlier in the stream.
    if (!validateRecords(&r)) return false;
    r = .{ .b = bytes };
    if (!readHeader(&r)) return false;
    while (r.i < r.b.len) {
        const kind = r.u32v() orelse return false;
        const len = r.u64v() orelse return false;
        const payload = r.take(@intCast(len)) orelse return false;
        switch (@as(RecordKind, @enumFromInt(kind))) {
            .authority => {
                if (payload.len != g_authority_digest.len or g_has_authority) return false;
                @memcpy(&g_authority_digest, payload);
                g_has_authority = true;
            },
            .heap => if (!loadHeap(payload)) return false,
            .facts => if (!loadFacts(payload, &facts_began, &max_basis)) return false,
            // Install dispatch only after the Store has completed recovery and
            // unencoded named method receivers have been materialized.
            .code => code_records.append(ca, payload) catch @panic("image: oom"),
            else => {}, // unknown/not-yet-handled record: skip by its framed length
        }
    }
    // Restore the store counters once, after every generation's facts are applied.
    if (facts_began) {
        const store = g_api.import("Store").?;
        _ = g_api.send(store, g_api.intern("imageLoadEnd:raw:"), &[_]Oop{ g_api.mkSmallInt(max_basis), g_api.falseObj }, 2);
        // The MOP mirror's native #primitive markers are a rebuilt cache excluded
        // from the saved stream (docs/42 §D1) — the load just replaced the fact list
        // wholesale, so rebuild them now, or reflection over native protocol would be
        // lost by loading an image.
        _ = g_api.send(store, g_api.intern("backfillMirror"), &[_]Oop{}, 0);
        // Datalog rules are canonical relational data (docs/42 §D2) and DID
        // serialize; rebuild the relational engine's rule cache from the loaded
        // facts (guarded: only if the relational module is loaded).
        if (g_api.isExported("db") != 0)
            _ = g_api.send(store, g_api.intern("rebuildRules"), &[_]Oop{}, 0);
    }
    if (code_records.items.len != 0) {
        const cg = codegen orelse return false;
        const derived = g_api.send(cg, g_api.intern("deriveImageMethodTargets"), &[_]Oop{}, 0);
        if (derived == null or derived == g_api.nilObj or
            (g_api.isExported("ErrorVt") != 0 and g_api.vtableOf(derived) == g_api.import("ErrorVt"))) return false;
        for (code_records.items) |payload|
            if (!loadedGenerationMatchesAuthority() or !loadCode(payload)) return false;
    }
    return true;
}

/// After a load, mirror the load relocation table into the save worklist so the
/// next `appendImage:` continues this image's object numbering.
fn seedSaveStateFromLoad(basis: i64) void {
    g_reloc.clearRetainingCapacity();
    g_objs.clearRetainingCapacity();
    for (g_load_reloc.items, 0..) |o, id| {
        if (o) |p| g_reloc.put(ca, @intFromPtr(p), @intCast(id)) catch @panic("image: oom");
        g_objs.append(ca, o) catch @panic("image: oom");
    }
    g_saved_basis = basis;
}

/// Whether the most recent `loadImage` mapped a compiled `.code` section (methods
/// installed, no recompile). The boot path uses this to decide whether it must
/// still provide the library another way (replay / stdlib re-eval).
pub fn mappedCode() bool {
    return g_mapped_code;
}

/// Validate an image's STRUCTURE — header + record framing — without mapping it
/// into a running system (no dispatch, no allocation, no `g_api`). The
/// format-level tests use this; production boot uses `loadImage`.
pub fn validate(path: []const u8) bool {
    const bytes = std.fs.cwd().readFileAlloc(ca, path, 1 << 30) catch return false;
    defer ca.free(bytes);
    var r = Reader{ .b = bytes };
    if (!readHeader(&r)) return false;
    return validateRecords(&r);
}

/// Check every framed record and the integrity manifest of each compiled unit.
/// Kept separate from `validate` so `loadImage` can run the same preflight before
/// it mutates the store or materializes heap objects.
fn validateRecords(r: *Reader) bool {
    var authority: [64]u8 = undefined;
    var has_authority = false;
    var has_code = false;
    while (r.i < r.b.len) {
        const kind = r.u32v() orelse return false;
        const len = r.u64v() orelse return false;
        const payload = r.take(@intCast(len)) orelse return false;
        switch (@as(RecordKind, @enumFromInt(kind))) {
            .authority => {
                if (has_authority or payload.len != authority.len or has_code) return false;
                @memcpy(&authority, payload);
                has_authority = true;
            },
            .code => {
                if (!has_authority or has_code or codeUnitBytes(payload, &authority) == null) return false;
                has_code = true;
            },
            else => {},
        }
    }
    return has_authority;
}

/// Validate the header: magic, format version, byte order. The ABI/arch fields
/// are recorded but not fatal here (slice 4 uses them to arbitrate map-vs-replay);
/// step-3 images carry no compiled sections, so any same-format header loads.
fn readHeader(r: *Reader) bool {
    const magic = r.take(MAGIC.len) orelse return false;
    if (!std.mem.eql(u8, magic, MAGIC)) return false;
    const version = r.u32v() orelse return false;
    if (version != FORMAT_VERSION) return false;
    g_img_format = version;
    if ((r.u32v() orelse return false) != ENDIAN_TAG) return false;
    g_img_arch = r.u32v() orelse return false; // arch tag (code sections are per-arch)
    g_img_abi = r.u64v() orelse return false; // runtime-shim ABI validity hash
    return true;
}

/// True if the image's compiled `.code` section can be trusted on this runtime:
/// same architecture and the same shim ABI (docs/40 §4). A mismatch — or an
/// explicit rebuild request — forces the replay path instead.
fn codeMappable() bool {
    return !g_force_rebuild and g_img_arch == ARCH_TAG and g_img_abi == abiHash();
}

// -- birnam surface: [System saveImage:] / [System loadImage:] ---------------

fn pathArg(o: Oop, len: *usize) ?[*]const u8 {
    if (o == null or g_api.vtableOf(o) != g_api.string_vt) return null;
    return g_api.stringBytes(o, len);
}

/// `[System saveImage: path]` — serialize the running native system to `path`.
/// Answers self only after every durable value is encoded or has a registered
/// recovery path; answers nil on rejection or I/O failure. Inspect the structured
/// outcome through `[System imageSaveReport]`.
fn sysSaveImage(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var len: usize = 0;
    const p = pathArg(a[0], &len) orelse return g_api.nilObj;
    return if (saveImage(p[0..len])) self else g_api.nilObj;
}

fn mReportStatus(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return g_api.intern(switch (g_save_report.status) {
        .never => "never",
        .saved => "saved",
        .rejected => "rejected",
        .io_failed => "ioFailed",
    });
}
fn mReportSavedFacts(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return g_api.mkSmallInt(g_save_report.saved_facts);
}
fn mReportRecoveredFacts(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return g_api.mkSmallInt(g_save_report.recovered_facts);
}
fn mReportRejectedFacts(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return g_api.mkSmallInt(g_save_report.rejected_facts);
}
fn mReportRecoveredSlots(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return g_api.mkSmallInt(g_save_report.recovered_slots);
}
fn mReportRejectedSlots(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return g_api.mkSmallInt(g_save_report.rejected_slots);
}
fn mReportCodeUnits(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return g_api.mkSmallInt(g_save_report.code_units);
}
fn mReportDetail(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return g_api.mkString(g_save_report.detail[0..g_save_report.detail_len].ptr, g_save_report.detail_len);
}

/// `[System imageSaveReport]` — the result of the latest save. Its
/// selectors are `status`, `savedFacts`, `recoveredFacts`, `rejectedFacts`,
/// `recoveredSlots`, `rejectedSlots`, `codeUnits`, and `detail`.
fn sysImageSaveReport(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return g_report_obj;
}

/// `[System loadImage: path]` — map an image back in (mapping compiled code by
/// default). Answers nil on failure, `true` if the compiled `.code` section was
/// mapped (no recompile), `false` if it loaded but code was NOT mapped (a
/// data-only image or an arch/ABI mismatch) — so the caller can supply the
/// library another way (replay / stdlib). `birnam foo.image` uses this on boot.
fn sysLoadImage(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    g_force_rebuild = false;
    var len: usize = 0;
    const p = pathArg(a[0], &len) orelse return g_api.nilObj;
    if (!loadImage(p[0..len])) return g_api.nilObj;
    return if (g_mapped_code) g_api.trueObj else g_api.falseObj;
}

/// `[System loadImage: path rebuild: aBoolean]` — like `loadImage:`, but when
/// `aBoolean` is true FORCE the replay path (rebuild code from the image's own
/// source) instead of mapping the compiled `.code`. Answers as `loadImage:`.
fn sysLoadImageRebuild(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    g_force_rebuild = (a[1] == g_api.trueObj);
    defer g_force_rebuild = false;
    var len: usize = 0;
    const p = pathArg(a[0], &len) orelse return g_api.nilObj;
    if (!loadImage(p[0..len])) return g_api.nilObj;
    return if (g_mapped_code) g_api.trueObj else g_api.falseObj;
}

fn sysImageCompositionIdentity(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    var buf: [16]u8 = undefined;
    const text = std.fmt.bufPrint(&buf, "{x:0>16}", .{g_code_composition}) catch unreachable;
    return g_api.mkString(text.ptr, text.len);
}

// -- exe bake: package the running system as a self-booting executable -------
//
// The standalone-exe bake (docs/40 §6): append the image to a copy of the running
// `birnam` runtime, after a trailer the runtime notices on startup and boots.
// This module owns only the packaging; the boot detection is in main.zig (which
// MUST keep `bake_magic` in sync). Layout, appended after the runtime binary:
//   [ image bytes ][ u64 img_len ][ 8-byte magic ]
const bake_magic = "BIRNMEXE";

/// `[System bakeExe: outPath]` — serialize the running system and append it to a
/// copy of this executable, producing a self-booting app at `outPath` (running it
/// boots the image and honors the `main`-entry convention; no `.image` file, no
/// explicit `birnam` invocation). Answers self on success, nil on any failure.
fn sysBakeExe(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    var len: usize = 0;
    const p = pathArg(a[0], &len) orelse return g_api.nilObj;
    const out_path = ca.dupe(u8, p[0..len]) catch return g_api.nilObj;
    defer ca.free(out_path);

    // Save the current system to a per-process temp image, then read it back.
    var pbuf: [64]u8 = undefined;
    const tmp = std.fmt.bufPrintZ(&pbuf, "/tmp/birnam-bakeimg-{d}.image", .{std.c.getpid()}) catch return g_api.nilObj;
    if (!saveImage(tmp)) return g_api.nilObj;
    defer std.fs.cwd().deleteFile(tmp) catch {};
    const img = std.fs.cwd().readFileAlloc(ca, tmp, 1 << 30) catch return g_api.nilObj;
    defer ca.free(img);

    // Copy the running exe, then append the image + trailer.
    const self_path = std.fs.selfExePathAlloc(ca) catch return g_api.nilObj;
    defer ca.free(self_path);
    const self_bytes = std.fs.cwd().readFileAlloc(ca, self_path, 1 << 30) catch return g_api.nilObj;
    defer ca.free(self_bytes);

    const f = std.fs.cwd().createFile(out_path, .{ .mode = 0o755 }) catch return g_api.nilObj;
    defer f.close();
    f.writeAll(self_bytes) catch return g_api.nilObj;
    f.writeAll(img) catch return g_api.nilObj;
    var lb: [8]u8 = undefined;
    std.mem.writeInt(u64, &lb, @intCast(img.len), .little);
    f.writeAll(&lb) catch return g_api.nilObj;
    f.writeAll(bake_magic) catch return g_api.nilObj;
    return self;
}

// -- bringup -----------------------------------------------------------------

pub fn bringUp(api: *IdApi) void {
    g_api = api;
    const system_vt = api.import("SystemVt");
    api.method(system_vt, api.intern("saveImage:"), &sysSaveImage);
    api.method(system_vt, api.intern("imageSaveReport"), &sysImageSaveReport);
    api.method(system_vt, api.intern("loadImage:"), &sysLoadImage);
    api.method(system_vt, api.intern("loadImage:rebuild:"), &sysLoadImageRebuild);
    api.method(system_vt, api.intern("imageCompositionIdentity"), &sysImageCompositionIdentity);
    api.method(system_vt, api.intern("bakeExe:"), &sysBakeExe);
    g_globals_entity = api.intern("__globals__");
    g_globalsrc_entity = api.intern("__globalsrc__");
    if (api.isExported("ImageSaveReport") != 0) {
        g_report_vt = api.import("ImageSaveReportVt");
        g_report_obj = api.import("ImageSaveReport");
    } else {
        g_report_vt = api.proto(api.object_vt);
        g_report_obj = api.alloc(g_report_vt, 0);
        _ = api.exportObj("ImageSaveReportVt", g_report_vt);
        _ = api.exportObj("ImageSaveReport", g_report_obj);
    }
    api.method(g_report_vt, api.intern("status"), &mReportStatus);
    api.method(g_report_vt, api.intern("savedFacts"), &mReportSavedFacts);
    api.method(g_report_vt, api.intern("recoveredFacts"), &mReportRecoveredFacts);
    api.method(g_report_vt, api.intern("rejectedFacts"), &mReportRejectedFacts);
    api.method(g_report_vt, api.intern("recoveredSlots"), &mReportRecoveredSlots);
    api.method(g_report_vt, api.intern("rejectedSlots"), &mReportRejectedSlots);
    api.method(g_report_vt, api.intern("codeUnits"), &mReportCodeUnits);
    api.method(g_report_vt, api.intern("detail"), &mReportDetail);
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
