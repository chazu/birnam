//! Image snapshot — Phase 12.1 (slice 1: fact layer).
//!
//! Serializes the *current-value* fact set of a `FactStore` to a flat
//! byte buffer and replays it into a fresh store. This is the semantic
//! baseline for `.bim` (ADR-11 #5 D3 hybrid); the mmap-segment fast
//! path (12.2 / 12.5) is a later optimization layered over the same
//! logical content.
//!
//! SLICE 1 SCOPE — deliberately partial, tracked against Phase 12:
//!   - Only facts whose VALUE is a self-describing immediate (eid,
//!     smallint, keyword, nil) round-trip. Pointer-tagged values
//!     (strings, boxed floats, RRB vectors, capture-spec arrays,
//!     closures) are NOT serialized — they are *counted* and reported
//!     via `WriteResult.skipped_pointer_values`, never silently dropped.
//!   - Heap-Ref value serialization → slice 2+ (strings / floats / rrb).
//!   - Closure rematerialization from facts (Approach B, ROADMAP
//!     Framing decision 2026-06-02) → its own slice with the loader.
//!   - History (per-tx ValueHist) is not preserved; only current values.
//!     `next_tx` is round-tripped so future txs continue monotonically.
//!   - Module reload / bootstrap partition (which facts are user state vs
//!     reconstructed-on-load) → loader slice. Slice 1 replays whatever it
//!     was given into a fresh store verbatim.

const std = @import("std");
const ref_mod = @import("ref.zig");
const fact_store = @import("fact_store.zig");
const strings = @import("strings.zig");
const boxed = @import("boxed.zig");
const kernel_eids = @import("kernel_eids.zig");
const object_table = @import("object_table.zig");
const object = @import("object.zig");
const closure_mod = @import("closure.zig");
const dispatch = @import("dispatch.zig");
const bytecode = @import("bytecode.zig");
const interner_mod = @import("interner.zig");
const vm_mod = @import("vm.zig");

pub const Ref = ref_mod.Ref;
pub const FactStore = fact_store.FactStore;
pub const ObjectTable = object_table.ObjectTable;
pub const Interner = interner_mod.Interner;

/// Bump on any incompatible change to the on-disk layout below.
/// v2 (slice 2): per-fact value-kind tag; string values serialize bytes.
/// v3 (slice 3): boxed float + RRB vector (recursive element values).
/// v4: embedded manifest section (single self-describing file — no
///     sidecar manifest.txt; refines ADR-11 #5's directory archive).
/// v5: interner section (name↔eid bijection) so user names keep their eids
///     across reload — required for binding/method-name identity.
/// v6: VK_MAP — ASSOC_VT persistent maps serialize their entries (recursive),
///     so map literals in constant pools / AST values round-trip instead of
///     being skipped.
pub const FORMAT_VERSION: u32 = 6;

/// v7: FULL image (W1, docs/19c) — carries the *whole* fact-set, not just
/// `M_USER` data. Byte-identical section layout to v6 (same fact/value
/// encoding); the version is purely a LOAD-time signal: v7 = "self-contained,
/// do not re-init modules to reconstruct CODE — replay all facts + relink the
/// native residue" (the Stage-2/3 loader branches on `version >= 7`). A v6
/// (M_USER-only) image still loads via the hybrid path. Written by `writeFull`.
pub const FULL_FORMAT_VERSION: u32 = 7;

/// Oldest on-disk version this loader still accepts. v5..v7 share every section
/// layout — later versions only ADD value kinds / more facts — so a newer loader
/// reads an older image verbatim. Keeps pre-v6 images (e.g. a baked
/// `aot_embed.bim`) loadable.
pub const MIN_LOAD_VERSION: u32 = 5;

/// Single-writer transactor shape (ADR-11 #7). Recorded in the manifest so
/// a load knows whether the image expects the Datomic multi-thread shape.
pub const ThreadMode = enum(u8) { single = 0, datomic = 1 };

/// One module the image depends on. The loader must `dlopen` each by name
/// and verify `abi_hash` before replaying facts (the module's CODE is
/// reconstructed by its init; its DATA was persisted under M_USER).
pub const ModuleDep = struct {
    name: []const u8,
    abi_hash: u64,
};

/// Self-describing header embedded at the front of every `.bim`.
/// Carries everything a load needs before touching facts: which VM the
/// bytecode targets, the transactor shape, and the module dependency set.
pub const Manifest = struct {
    bytecode_format_version: u32 = bytecode.FORMAT_VERSION,
    thread_mode: ThreadMode = .single,
    modules: []const ModuleDep = &.{},
};

/// Value encoding discriminant. Recursive: an RRB element is itself one
/// of these kinds.
const VK_IMMEDIATE: u8 = 0; // self-describing Ref stored verbatim (u64)
const VK_STRING: u8 = 1; // u32 len + len bytes, rebuilt on load
const VK_FLOAT: u8 = 2; // f64 bit pattern (u64), re-boxed on load
const VK_RRB: u8 = 3; // u32 count + `count` recursively-encoded values
const VK_CAPTURES: u8 = 4; // CaptureSpecArray (u32 len + len*(u8 parent, u8 local))
// emitted only for :method-captures-ptr facts (attribute-aware — the value
// is a bare vt-less pointer that can't be discriminated by tag).
const VK_MAP: u8 = 5; // ASSOC_VT persistent map: u32 count + count*(key value), recursive

const MAGIC: [8]u8 = .{ 'B', 'R', 'N', 'I', 'M', 'G', 0, 1 };

pub const WriteError = error{OutOfMemory};
pub const ReadError = error{ BadMagic, BadVersion, BadManifest, Truncated, BadValueKind, StoreError, OutOfMemory };

pub const WriteResult = struct {
    bytes: []u8,
    facts_written: u64,
    /// Facts skipped because their value is a heap pointer (slice 1 can't
    /// serialize those yet). Surfaced so coverage is never silently capped.
    skipped_pointer_values: u64,
};

pub const LoadResult = struct {
    facts_loaded: u64,
    /// Counters to restore into the live runtime after load. `image.zig`
    /// stays decoupled from object_table; the caller applies `next_eid`
    /// to its allocator.
    next_tx: u64,
    next_eid: u64,
    /// The embedded manifest. `modules` (and their names) are allocated in
    /// the store's allocator, so they outlive the `bytes` buffer.
    manifest: Manifest,
};

/// W1 Stage 2 (docs/19c): the interned attribute eid for
/// `[wrapper :closure-block-method block_method_eid]` recipe facts. The kernel
/// eid space (0..255) is full, so this is an *interned* attr (user-range);
/// bootstrap sets it deterministically via `setClosureBlockMethodAttr`. 0 = not
/// set (bare unit tests / v6 path) → the closure-wrapper logic is skipped.
var g_closure_block_method_attr: u64 = 0;
/// W1 Stage 2b: attr for a capturing block wrapper's serialized captures (RRB).
var g_closure_block_captures_attr: u64 = 0;

pub fn setClosureBlockMethodAttr(e: u64) void {
    g_closure_block_method_attr = e;
}

pub fn setClosureBlockCapturesAttr(e: u64) void {
    g_closure_block_captures_attr = e;
}

/// W1 Stage 2b part 2: the lang module's tree-walker block `Closure.method`
/// (registered via the ABI at init) + the interned recipe attrs carrying a
/// durable tree-walker block global's (AST, captured-scope) eids. All 0/null
/// until lang loads → tree-walker rebuild is simply skipped (bare kernel tests).
var g_treewalk_shim: ?*const anyopaque = null;
var g_treewalk_block_ast_attr: u64 = 0;
var g_treewalk_block_scope_attr: u64 = 0;

pub fn setTreewalkBlockShim(p: ?*const anyopaque) void {
    g_treewalk_shim = p;
}

pub fn setTreewalkBlockAttrs(ast_attr: u64, scope_attr: u64) void {
    g_treewalk_block_ast_attr = ast_attr;
    g_treewalk_block_scope_attr = scope_attr;
}

/// A lang **tree-walker block closure** decoded for serialization: the block's
/// AST node eid + its captured-scope eid. Both are already facts in the store
/// (the AST is fact-shaped; the scope is a SCOPE_VT entity with binding facts),
/// so the recipe persists them by reference and the loader rebuilds the runtime
/// wrapper. Returned by `treewalkBlockClosure`.
const TreewalkBlockInfo = struct { ast: Ref, scope: Ref };

/// If `eid` names a lang tree-walker block-closure wrapper — `CLOSURE_VT` whose
/// `method` is the registered tree-walker shim and whose `data` points at a
/// 2-slot `OBJECT_VT { ast, scope }` (the `allocBlockClosure` shape) — decode it;
/// else null. The shim-pointer identity is the precise discriminator (only the
/// closures lang minted via that shim match), so this never misfires on bytecode
/// blocks or primitive-method closures. Null when lang is not loaded.
fn treewalkBlockClosure(table: *const object_table.ObjectTable, eid: u64) ?TreewalkBlockInfo {
    const shim = g_treewalk_shim orelse return null;
    const ref = table.get(eid) orelse return null;
    if (!ref_mod.isPointer(ref)) return null;
    const vt = object.vtableOf(object.fromRef(ref));
    if (!ref_mod.isEid(vt) or ref_mod.untagEid(vt) != kernel_eids.CLOSURE_VT) return null;
    const c: *const closure_mod.Closure = @ptrCast(@alignCast(ref_mod.untagPointer(ref)));
    if (c.method != shim) return null;
    if (!ref_mod.isPointer(c.data)) return null;
    const data_obj = object.fromRef(c.data);
    const data_vt = object.vtableOf(data_obj);
    if (!ref_mod.isEid(data_vt) or ref_mod.untagEid(data_vt) != kernel_eids.OBJECT_VT) return null;
    const ast = object.slotAt(data_obj, 0);
    const scope = object.slotAt(data_obj, 1);
    if (!ref_mod.isEid(ast) or !ref_mod.isEid(scope)) return null;
    return .{ .ast = ast, .scope = scope };
}

/// A **kernel bytecode-block closure wrapper** decoded from the object table:
/// its block-method eid and its inline captured values (empty for captureless
/// blocks). Returned by `bytecodeBlockClosure`.
const BlockClosureInfo = struct { method_eid: u64, captures: []const Ref };

/// If `eid` names a kernel **bytecode-block closure wrapper** in the object
/// table (`[vt=CLOSURE_VT][method=&bytecodeDispatcherImpl][data=BytecodeClosure]`),
/// decode its method eid + captures; else null. The `method` pointer is the
/// robust discriminator: it distinguishes a kernel bytecode block from a
/// lang-module *tree-walker* block (whose `method` is `blockEvalShim` and whose
/// `data` is an OBJECT_VT wrapper — handled out-of-kernel, Stage 2b part 2) and
/// from a method-eid closure (`data` = eid, not a block). Used by the full-image
/// writer to emit a reconstruction recipe for durable block globals (W1 Stage 2).
fn bytecodeBlockClosure(table: *const object_table.ObjectTable, eid: u64) ?BlockClosureInfo {
    const ref = table.get(eid) orelse return null;
    if (!ref_mod.isPointer(ref)) return null;
    const vt = object.vtableOf(object.fromRef(ref));
    if (!ref_mod.isEid(vt) or ref_mod.untagEid(vt) != kernel_eids.CLOSURE_VT) return null;
    const c: *const closure_mod.Closure = @ptrCast(@alignCast(ref_mod.untagPointer(ref)));
    if (c.method != @as(*const anyopaque, @ptrCast(&dispatch.bytecodeDispatcherImpl))) return null;
    if (!ref_mod.isPointer(c.data)) return null; // method-eid closure (data = eid) — not a block
    return .{ .method_eid = vm_mod.blockMethodEidFromRef(c.data), .captures = vm_mod.blockCapturesFromRef(c.data) };
}

/// A value Ref is a self-describing immediate (eid / smallint / keyword /
/// nil) — stored verbatim. Other supported classes (string, float, RRB)
/// serialize their contents; see `valueSupported`.
fn isImmediateValue(v: Ref) bool {
    return ref_mod.isEid(v) or ref_mod.isSmallInt(v) or ref_mod.isKeyword(v) or ref_mod.isNil(v);
}

/// Can this value round-trip? Recursive for RRB: supported iff every
/// element is supported. Remaining heap classes (closure, FFI ptr,
/// capture-spec array) are deferred and reported as skipped.
fn valueSupported(v: Ref) bool {
    if (isImmediateValue(v) or strings.isString(v) or boxed.isFloat(v)) return true;
    if (boxed.isRrbRef(v)) {
        var it = boxed.untagRrbRef(v).iter();
        while (it.next()) |elem| {
            if (!valueSupported(elem)) return false;
        }
        return true;
    }
    if (boxed.isMapRef(v)) {
        var it = boxed.untagMapRef(v).iter();
        while (it.next()) |kv| {
            if (!valueSupported(kv.key) or !valueSupported(kv.value)) return false;
        }
        return true;
    }
    return false;
}

/// W1 Stage-0 diagnostic (docs/19c). Walks the whole fact-set and reports what a
/// full-fact image would contain: by provenance and value-kind, separating the
/// facts that already serialize from the irreducible residue (unsupported
/// pointer values — native closures / FFI ptrs / struct types). No state change;
/// gated by `BIRNAM_RESIDUE_DUMP` in the shell. Caller owns the returned report.
pub fn characterizeResidue(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    interner: *const interner_mod.Interner,
) ![]u8 {
    var total: u64 = 0;
    var m_user: u64 = 0;
    var non_user: u64 = 0;
    var non_user_ser: u64 = 0;
    var residue: u64 = 0;

    var by_attr = std.AutoHashMap(u64, u64).init(allocator);
    defer by_attr.deinit();
    var by_kind = std.AutoHashMap(u64, u64).init(allocator);
    defer by_kind.deinit();

    var it = store.meatIter();
    while (it.next()) |entry| {
        total += 1;
        if (entry.m == kernel_eids.M_USER) {
            m_user += 1;
            continue;
        }
        non_user += 1;
        // Captures are pointer-valued but the writer encodes them (VK_CAPTURES),
        // so they are NOT residue.
        const serializable = valueSupported(entry.v) or
            (entry.a == kernel_eids.ATTR_METHOD_CAPTURES_PTR and ref_mod.isPointer(entry.v));
        if (serializable) {
            non_user_ser += 1;
            continue;
        }
        residue += 1;
        const ga = try by_attr.getOrPut(entry.a);
        if (!ga.found_existing) ga.value_ptr.* = 0;
        ga.value_ptr.* += 1;
        var kind: u64 = 0; // 0 = non-pointer / unknown
        if (ref_mod.isPointer(entry.v)) {
            const vt = object.vtableOf(object.fromRef(entry.v));
            if (ref_mod.isEid(vt)) kind = ref_mod.untagEid(vt);
        }
        const gk = try by_kind.getOrPut(kind);
        if (!gk.found_existing) gk.value_ptr.* = 0;
        gk.value_ptr.* += 1;
    }

    var buf = std.ArrayList(u8){};
    errdefer buf.deinit(allocator);
    const w = buf.writer(allocator);
    try w.writeAll("\n=== W1 Stage-0 residue characterization (docs/19c) ===\n");
    try w.print("total facts:                {d}\n", .{total});
    try w.print("  M_USER (written today):   {d}\n", .{m_user});
    try w.print("  non-M_USER (filtered):    {d}\n", .{non_user});
    try w.print("    would serialize:        {d}\n", .{non_user_ser});
    try w.print("    RESIDUE (native ptrs):  {d}\n", .{residue});
    try w.writeAll("\nresidue by attribute:\n");
    var ait = by_attr.iterator();
    while (ait.next()) |kv| {
        const name = interner.nameOf(kv.key_ptr.*) orelse "?";
        try w.print("  {s} (eid {d}): {d}\n", .{ name, kv.key_ptr.*, kv.value_ptr.* });
    }
    try w.writeAll("\nresidue by value vtable (kind):\n");
    var kit = by_kind.iterator();
    while (kit.next()) |kv| {
        const name = if (kv.key_ptr.* == 0) "<non-pointer or vtable-less native box>" else (interner.nameOf(kv.key_ptr.*) orelse "?");
        try w.print("  vt {s} (eid {d}): {d}\n", .{ name, kv.key_ptr.*, kv.value_ptr.* });
    }
    try w.writeAll("=== end residue characterization ===\n\n");
    return buf.toOwnedSlice(allocator);
}

/// Encode one value (assumes `valueSupported(v)`). Recursive for RRB.
fn writeValue(allocator: std.mem.Allocator, buf: *std.ArrayList(u8), v: Ref) WriteError!void {
    if (isImmediateValue(v)) {
        try buf.append(allocator, VK_IMMEDIATE);
        try appendU64(allocator, buf, v);
    } else if (strings.isString(v)) {
        const s = strings.bytes(v);
        try buf.append(allocator, VK_STRING);
        try appendU32(allocator, buf, @intCast(s.len));
        try buf.appendSlice(allocator, s);
    } else if (boxed.isFloat(v)) {
        try buf.append(allocator, VK_FLOAT);
        try appendU64(allocator, buf, @bitCast(boxed.untagFloat(v)));
    } else if (boxed.isMapRef(v)) {
        const m = boxed.untagMapRef(v);
        try buf.append(allocator, VK_MAP);
        try appendU32(allocator, buf, @intCast(m.count()));
        var it = m.iter();
        while (it.next()) |kv| {
            try writeValue(allocator, buf, kv.key); // key is a Ref (u64 bit pattern)
            try writeValue(allocator, buf, kv.value);
        }
    } else { // RRB (guaranteed by valueSupported)
        const r = boxed.untagRrbRef(v);
        try buf.append(allocator, VK_RRB);
        try appendU32(allocator, buf, @intCast(r.count()));
        var it = r.iter();
        while (it.next()) |elem| try writeValue(allocator, buf, elem);
    }
}

/// Encode a `:method-captures-ptr` value: the pointed-to CaptureSpecArray
/// (header.len + inline CaptureSpec records).
fn writeCaptures(allocator: std.mem.Allocator, buf: *std.ArrayList(u8), v: Ref) WriteError!void {
    const base: [*]const u8 = @ptrCast(@alignCast(ref_mod.untagPointer(v)));
    const header: *const vm_mod.CaptureSpecArrayHeader = @ptrCast(@alignCast(base));
    const tail: [*]const vm_mod.CaptureSpec = @ptrFromInt(@intFromPtr(base) + @sizeOf(vm_mod.CaptureSpecArrayHeader));
    try buf.append(allocator, VK_CAPTURES);
    try appendU32(allocator, buf, header.len);
    var i: u32 = 0;
    while (i < header.len) : (i += 1) {
        try buf.append(allocator, tail[i].parent_slot);
        try buf.append(allocator, tail[i].local_slot);
    }
}

/// Decode one value, rebuilding heap objects in the store's allocator so
/// the Ref outlives load. Recursive for RRB.
fn readValue(store: *FactStore, cur: *Cursor) ReadError!Ref {
    const kind = (try cur.take(1))[0];
    switch (kind) {
        VK_IMMEDIATE => return try cur.readU64(),
        VK_CAPTURES => {
            const len = try cur.readU32();
            const header_sz = @sizeOf(vm_mod.CaptureSpecArrayHeader);
            const total = header_sz + @sizeOf(vm_mod.CaptureSpec) * len;
            // 8-align (Ref) so mkPointer's tag invariant holds — the header
            // itself only needs 4, but a tagged pointer needs the low 3 bits clear.
            const out = store.allocator.alignedAlloc(u8, .of(Ref), total) catch return error.OutOfMemory;
            const header: *vm_mod.CaptureSpecArrayHeader = @ptrCast(@alignCast(out.ptr));
            header.* = .{ .len = len };
            const tail: [*]vm_mod.CaptureSpec = @ptrFromInt(@intFromPtr(out.ptr) + header_sz);
            var i: u32 = 0;
            while (i < len) : (i += 1) {
                const p = (try cur.take(1))[0];
                const l = (try cur.take(1))[0];
                tail[i] = .{ .parent_slot = p, .local_slot = l };
            }
            return ref_mod.mkPointer(@ptrCast(out.ptr));
        },
        VK_STRING => {
            const len = try cur.readU32();
            const s = try cur.take(len);
            return strings.mkString(store.allocator, s) catch return error.OutOfMemory;
        },
        VK_FLOAT => {
            const bits = try cur.readU64();
            return boxed.mkFloat(store.allocator, @bitCast(bits)) catch return error.OutOfMemory;
        },
        VK_RRB => {
            const n = try cur.readU32();
            const elems = store.allocator.alloc(Ref, n) catch return error.OutOfMemory;
            defer store.allocator.free(elems);
            var i: u32 = 0;
            while (i < n) : (i += 1) elems[i] = try readValue(store, cur);
            return boxed.mkRrb(store.allocator, elems) catch return error.OutOfMemory;
        },
        VK_MAP => {
            const n = try cur.readU32();
            var map = boxed.mkMap(store.allocator) catch return error.OutOfMemory;
            var i: u32 = 0;
            while (i < n) : (i += 1) {
                const k = try readValue(store, cur);
                const val = try readValue(store, cur);
                map = boxed.mapPut(store.allocator, map, k, val) catch return error.OutOfMemory;
            }
            return map;
        },
        else => return error.BadValueKind,
    }
}

// -- Writer -----------------------------------------------------------------

/// Serialize the store's current facts, the embedded manifest, and the
/// runtime counters. The caller supplies `next_eid` (read from its object
/// table) so this module need not depend on the allocator, and `manifest`
/// (module deps + versions). Returns owned bytes; caller frees.
/// Serialize the M_USER fact-set (DATA only) — the hybrid v6 image. CODE is
/// reconstructed on load by re-running module init.
pub fn write(
    allocator: std.mem.Allocator,
    store: *const FactStore,
    next_eid: u64,
    manifest: Manifest,
    interner: ?*const Interner,
) WriteError!WriteResult {
    return writeImpl(allocator, store, next_eid, manifest, interner, false, null);
}

/// Serialize the WHOLE fact-set (W1, docs/19c) — a self-contained v7 image.
/// Carries CODE (vtables, methods, bytecode, AST, grammars) so load needs no
/// module recompile; only the tiny native-ptr residue (function-impl /
/// reader-impl, ~9 facts) is skipped and relinked by name on load (Stage 2).
pub fn writeFull(
    allocator: std.mem.Allocator,
    store: *const FactStore,
    next_eid: u64,
    manifest: Manifest,
    interner: ?*const Interner,
    table: ?*const object_table.ObjectTable,
) WriteError!WriteResult {
    return writeImpl(allocator, store, next_eid, manifest, interner, true, table);
}

fn writeImpl(
    allocator: std.mem.Allocator,
    store: *const FactStore,
    next_eid: u64,
    manifest: Manifest,
    interner: ?*const Interner,
    full: bool,
    table: ?*const object_table.ObjectTable,
) WriteError!WriteResult {
    var buf = std.ArrayList(u8){};
    errdefer buf.deinit(allocator);

    try buf.appendSlice(allocator, &MAGIC);
    try appendU32(allocator, &buf, if (full) FULL_FORMAT_VERSION else FORMAT_VERSION);

    // Embedded manifest section (self-describing: no sidecar file).
    try appendU32(allocator, &buf, manifest.bytecode_format_version);
    try buf.append(allocator, @intFromEnum(manifest.thread_mode));
    try appendU32(allocator, &buf, @intCast(manifest.modules.len));
    for (manifest.modules) |dep| {
        try appendU32(allocator, &buf, @intCast(dep.name.len));
        try buf.appendSlice(allocator, dep.name);
        try appendU64(allocator, &buf, dep.abi_hash);
    }

    // Interner section: the name↔eid bijection. Backfill the count slot.
    const intern_count_slot = buf.items.len;
    try appendU32(allocator, &buf, 0);
    var intern_count: u32 = 0;
    if (interner) |intr| {
        var it = intr.eid_to_str.iter();
        while (it.next()) |kv| {
            try appendU64(allocator, &buf, kv.key);
            try appendU32(allocator, &buf, @intCast(kv.value.len));
            try buf.appendSlice(allocator, kv.value);
            intern_count += 1;
        }
        std.mem.writeInt(u32, buf.items[intern_count_slot..][0..4], intern_count, .little);
    }

    try appendU64(allocator, &buf, store.currentTx() + 1); // == next_tx
    try appendU64(allocator, &buf, next_eid);

    // Reserve the fact-count slot; backfill once we know the total.
    const count_slot = buf.items.len;
    try appendU64(allocator, &buf, 0);

    var facts_written: u64 = 0;
    var skipped: u64 = 0;

    // W1 Stage 2: collect eids referenced as durable fact VALUES, so after the
    // loop we can emit closure-wrapper reconstruction recipes for the ones that
    // are captureless block globals (e.g. `let jStr := [block]`). Only used in
    // the full path with a table + the interned recipe attr available.
    const want_closures = full and table != null and g_closure_block_method_attr != 0;
    var referenced = std.AutoHashMap(u64, void).init(allocator);
    defer referenced.deinit();

    // `meat` keys facts by module first, giving (m, e, a, v) directly.
    // v6 (hybrid): persist DATA facts only — CODE is reconstructed on load by
    // re-running module init. v7 (full, W1): persist EVERY fact regardless of
    // provenance; CODE round-trips, so load needs no recompile. `m` stays a
    // provenance TAG either way — it is only a persistence GATE in the v6 path.
    var m_it = store.meatIter();
    while (m_it.next()) |entry| {
        if (!full and entry.m != kernel_eids.M_USER) continue;

        // Attribute-aware: a :method-captures-ptr value is a bare vt-less
        // heap pointer to a CaptureSpecArray — serialize its contents so
        // capturing methods round-trip (else they rebuild as captureless).
        if (entry.a == kernel_eids.ATTR_METHOD_CAPTURES_PTR and ref_mod.isPointer(entry.v)) {
            try appendU64(allocator, &buf, entry.m);
            try appendU64(allocator, &buf, entry.e);
            try appendU64(allocator, &buf, entry.a);
            try writeCaptures(allocator, &buf, entry.v);
            facts_written += 1;
            continue;
        }

        if (!valueSupported(entry.v)) {
            skipped += 1;
            continue;
        }
        try appendU64(allocator, &buf, entry.m);
        try appendU64(allocator, &buf, entry.e);
        try appendU64(allocator, &buf, entry.a);
        try writeValue(allocator, &buf, entry.v);
        facts_written += 1;
        if (want_closures and ref_mod.isEid(entry.v)) {
            try referenced.put(ref_mod.untagEid(entry.v), {});
        }
    }

    // W1 Stage 2/2b: emit `[wrapper :closure-block-method bm]` recipe facts for
    // bytecode block-closure wrappers referenced by durable values, so
    // `rematerializeClosures` can rebuild them on load. A *capturing* wrapper
    // (Stage 2b — its block captured an enclosing-method local) additionally
    // gets `[wrapper :closure-block-captures <rrb>]` carrying the serialized
    // capture values. A capture may itself be a block wrapper (`mkEid`), so we
    // process a worklist: discovering a captured wrapper enqueues it for its own
    // recipe. Skip any that already carry the recipe fact (a re-saved image —
    // idempotent) or whose captures are not serializable (left to recompile).
    if (want_closures) {
        const m_eid = g_closure_block_method_attr;
        const cap_eid = g_closure_block_captures_attr;
        var work = std.ArrayList(u64){};
        defer work.deinit(allocator);
        var visited = std.AutoHashMap(u64, void).init(allocator);
        defer visited.deinit();
        var seed = referenced.keyIterator();
        while (seed.next()) |k| try work.append(allocator, k.*);

        while (work.pop()) |wrapper_eid| {
            if (visited.contains(wrapper_eid)) continue;
            try visited.put(wrapper_eid, {});

            // Tree-walker block global (Stage 2b part 2) — emit its (AST, scope)
            // recipe; both are already facts, so the loader rebuilds the wrapper
            // via the registered shim. Checked before the bytecode case: a
            // tree-walker closure is never a bytecode block (distinct shim).
            if (treewalkBlockClosure(table.?, wrapper_eid)) |tw| {
                if (store.currentValue(wrapper_eid, g_treewalk_block_ast_attr) != null) continue;
                try appendU64(allocator, &buf, kernel_eids.M_USER);
                try appendU64(allocator, &buf, wrapper_eid);
                try appendU64(allocator, &buf, g_treewalk_block_ast_attr);
                try writeValue(allocator, &buf, tw.ast);
                try appendU64(allocator, &buf, kernel_eids.M_USER);
                try appendU64(allocator, &buf, wrapper_eid);
                try appendU64(allocator, &buf, g_treewalk_block_scope_attr);
                try writeValue(allocator, &buf, tw.scope);
                facts_written += 2;
                continue;
            }

            const info = bytecodeBlockClosure(table.?, wrapper_eid) orelse continue;
            if (store.currentValue(wrapper_eid, m_eid) != null) continue; // already a fact

            // Capturing: bail out wholesale if any capture can't round-trip —
            // emitting the method recipe without its captures would silently
            // produce a wrong (captureless) block on load.
            if (info.captures.len != 0) {
                for (info.captures) |cv| {
                    if (!valueSupported(cv)) {
                        skipped += 1;
                        break;
                    }
                } else {
                    try appendU64(allocator, &buf, kernel_eids.M_USER);
                    try appendU64(allocator, &buf, wrapper_eid);
                    try appendU64(allocator, &buf, m_eid);
                    try writeValue(allocator, &buf, ref_mod.mkEid(info.method_eid));
                    facts_written += 1;
                    const rrb = boxed.mkRrb(allocator, info.captures) catch return error.OutOfMemory;
                    try appendU64(allocator, &buf, kernel_eids.M_USER);
                    try appendU64(allocator, &buf, wrapper_eid);
                    try appendU64(allocator, &buf, cap_eid);
                    try writeValue(allocator, &buf, rrb);
                    facts_written += 1;
                    // A captured value that is itself a block wrapper needs its
                    // own recipe — enqueue.
                    for (info.captures) |cv| {
                        if (ref_mod.isEid(cv)) try work.append(allocator, ref_mod.untagEid(cv));
                    }
                }
                continue;
            }

            try appendU64(allocator, &buf, kernel_eids.M_USER);
            try appendU64(allocator, &buf, wrapper_eid);
            try appendU64(allocator, &buf, m_eid);
            try writeValue(allocator, &buf, ref_mod.mkEid(info.method_eid));
            facts_written += 1;
        }
    }

    std.mem.writeInt(u64, buf.items[count_slot..][0..8], facts_written, .little);

    return .{
        .bytes = try buf.toOwnedSlice(allocator),
        .facts_written = facts_written,
        .skipped_pointer_values = skipped,
    };
}

// -- Reader -----------------------------------------------------------------

/// Replay a snapshot into a fresh store. Facts are asserted verbatim with
/// their original (e, a, v, module). Returns the counters to restore;
/// the caller sets `next_tx` on the store and `next_eid` on its table
/// AFTER this returns (replay itself burns tx numbers we then overwrite).
pub fn load(store: *FactStore, bytes: []const u8, interner: ?*Interner) ReadError!LoadResult {
    return loadImpl(false, store, bytes, interner);
}

/// Frozen load (Phase 14 `--strip=history`): replay facts through the lean
/// `frozenInsert` bulk path (no tx churn / history / mutate hooks) instead of
/// `assert`. Under a comptime-frozen build this is the only load referenced,
/// so the tx-writer path stays out of the hot replay. Caller `freeze()`s the
/// store afterward (once FFI resolve, which legitimately writes, has run).
pub fn loadFrozen(store: *FactStore, bytes: []const u8, interner: ?*Interner) ReadError!LoadResult {
    return loadImpl(true, store, bytes, interner);
}

fn loadImpl(comptime frozen: bool, store: *FactStore, bytes: []const u8, interner: ?*Interner) ReadError!LoadResult {
    var cur = Cursor{ .bytes = bytes };

    const magic = try cur.take(8);
    if (!std.mem.eql(u8, magic, &MAGIC)) return error.BadMagic;
    const version = try cur.readU32();
    if (version < MIN_LOAD_VERSION or version > FULL_FORMAT_VERSION) return error.BadVersion;

    // Embedded manifest. Module names + the dep slice are allocated in the
    // store's allocator so they outlive `bytes`.
    const bc_version = try cur.readU32();
    const thread_mode_raw = (try cur.take(1))[0];
    if (thread_mode_raw > @intFromEnum(ThreadMode.datomic)) return error.BadManifest;
    const module_count = try cur.readU32();
    const deps = store.allocator.alloc(ModuleDep, module_count) catch return error.OutOfMemory;
    var d: u32 = 0;
    while (d < module_count) : (d += 1) {
        const name_len = try cur.readU32();
        const name_src = try cur.take(name_len);
        const name = store.allocator.dupe(u8, name_src) catch return error.OutOfMemory;
        deps[d] = .{ .name = name, .abi_hash = try cur.readU64() };
    }
    const manifest = Manifest{
        .bytecode_format_version = bc_version,
        .thread_mode = @enumFromInt(thread_mode_raw),
        .modules = deps,
    };

    // Interner section: restore the name↔eid bijection so user names keep
    // their original eids. Entries boot already seeded are overwritten with
    // identical data; user names are re-added.
    const intern_count = try cur.readU32();
    var ic: u32 = 0;
    while (ic < intern_count) : (ic += 1) {
        const eid = try cur.readU64();
        const name_len = try cur.readU32();
        const name = try cur.take(name_len);
        if (interner) |intr| intr.restoreEntry(eid, name) catch return error.OutOfMemory;
    }

    const next_tx = try cur.readU64();
    const next_eid = try cur.readU64();
    const fact_count = try cur.readU64();

    var i: u64 = 0;
    while (i < fact_count) : (i += 1) {
        const m = try cur.readU64();
        const e = try cur.readU64();
        const a = try cur.readU64();
        const v = try readValue(store, &cur);
        if (frozen) {
            store.frozenInsert(e, a, v, m) catch return error.StoreError;
        } else {
            _ = store.assert(e, a, v, m) catch return error.StoreError;
        }
    }

    // Overwrite the tx counter burned during replay so live txs continue
    // from the saved high-water mark. (Frozen load never bumped it.)
    store.next_tx.store(next_tx, .monotonic);

    // Phase 15.R — keep basis_t a consistent monotonic floor across load. It is
    // re-derived (snapshots are in-memory pins that don't cross a save, so the
    // exact pre-save basis need not persist), but it must stay >= next_tx so the
    // basis-T timeline never reuses a coordinate below the tx floor. The replay
    // above bumped basis_t per assert (non-frozen) or left it at init (frozen);
    // raise it to the tx high-water mark.
    if (store.basis_t.load(.monotonic) < next_tx) store.basis_t.store(next_tx, .monotonic);

    return .{ .facts_loaded = fact_count, .next_tx = next_tx, .next_eid = next_eid, .manifest = manifest };
}

/// Parse only the embedded manifest (magic + version + manifest section),
/// without restoring the interner or replaying facts. The dynamic AOT loader
/// uses this to learn the module dependency set *before* `dlopen` + replay,
/// since modules must init before their DATA facts are replayed. Names in
/// `manifest.modules` are allocated with `allocator`.
pub fn peekManifest(allocator: std.mem.Allocator, bytes: []const u8) ReadError!Manifest {
    var cur = Cursor{ .bytes = bytes };
    const magic = try cur.take(8);
    if (!std.mem.eql(u8, magic, &MAGIC)) return error.BadMagic;
    const version = try cur.readU32();
    if (version < MIN_LOAD_VERSION or version > FULL_FORMAT_VERSION) return error.BadVersion;
    const bc_version = try cur.readU32();
    const thread_mode_raw = (try cur.take(1))[0];
    if (thread_mode_raw > @intFromEnum(ThreadMode.datomic)) return error.BadManifest;
    const module_count = try cur.readU32();
    const deps = allocator.alloc(ModuleDep, module_count) catch return error.OutOfMemory;
    var d: u32 = 0;
    while (d < module_count) : (d += 1) {
        const name_len = try cur.readU32();
        const name_src = try cur.take(name_len);
        const name = allocator.dupe(u8, name_src) catch return error.OutOfMemory;
        deps[d] = .{ .name = name, .abi_hash = try cur.readU64() };
    }
    return .{
        .bytecode_format_version = bc_version,
        .thread_mode = @enumFromInt(thread_mode_raw),
        .modules = deps,
    };
}

// -- On-disk file I/O -------------------------------------------------------

pub const FileError = error{ FileOpenFailed, FileWriteFailed, FileReadFailed };

/// Write a snapshot to `path` as a single `.bim` file. Pure data write —
/// no toolchain. Returns the WriteResult (caller frees `.bytes`).
pub fn saveToFile(
    allocator: std.mem.Allocator,
    path: []const u8,
    store: *const FactStore,
    next_eid: u64,
    manifest: Manifest,
    interner: ?*const Interner,
) (WriteError || FileError)!WriteResult {
    const res = try write(allocator, store, next_eid, manifest, interner);
    errdefer allocator.free(res.bytes);
    const f = std.fs.cwd().createFile(path, .{ .truncate = true }) catch return error.FileOpenFailed;
    defer f.close();
    f.writeAll(res.bytes) catch return error.FileWriteFailed;
    return res;
}

/// Read a `.bim` file and replay it into `store`. The file bytes are
/// read into `allocator` and freed before returning (facts/strings are
/// copied into the store's allocator during load).
pub fn loadFromFile(
    allocator: std.mem.Allocator,
    path: []const u8,
    store: *FactStore,
    interner: ?*Interner,
) (ReadError || FileError)!LoadResult {
    const f = std.fs.cwd().openFile(path, .{}) catch return error.FileOpenFailed;
    defer f.close();
    const bytes = f.readToEndAlloc(allocator, std.math.maxInt(usize)) catch return error.FileReadFailed;
    defer allocator.free(bytes);
    return load(store, bytes, interner);
}

// -- Approach B: rematerialize closures from facts --------------------------

pub const RematError = error{ OutOfMemory, StoreError };

/// Rebuild the fn-ptr Closure for every bytecode-method entity from its
/// facts (Phase 12 Framing decision 2026-06-02, Approach B). The image
/// never serializes the closure struct (it holds a raw fn ptr into the
/// module's .text); instead, after load, each `:bytecode-stream` entity's
/// object-table slot is re-minted to point at `bytecodeDispatcherImpl` of
/// the freshly-loaded kernel, with `data = mkEid(method_eid)`.
///
/// Layout mirrors compiler.materialize's install tail:
///   [vt_ref = BYTECODE_METHOD_VT][method = &bytecodeDispatcherImpl][data]
/// with `vt_ref` at offset -1 (Phase 12.8). `vtableEidOf(mkEid(method_eid))`
/// resolves BYTECODE_METHOD_VT; primary dispatch enters the VM directly via
/// `send.sendN`, and `[method value]` resolves through the vt's delegation to
/// CLOSURE_VT. `.method` is kept as the discriminant the gc/prune/dispatch
/// paths key on.
///
/// Idempotent: entities already materialized (e.g. by a prior init) are
/// left untouched. Returns the count rebuilt.
///
/// Captures: a method's `:method-captures-ptr` value (the CaptureSpecArray) IS
/// serialized — `writeCaptures`/`readValue` encode it as `VK_CAPTURES`, so the
/// spec fact reloads verbatim (see the round-trip test in `image_test.zig`,
/// "`:method-captures-ptr` (CaptureSpecArray) round-trips"). The captures are
/// applied at `runMethod` entry and at `MAKE_BLOCK` by reading that spec from
/// the store (`dispatch.zig`), so the wrapper rebuilt here only needs
/// `data = mkEid(method_eid)` — identical for capturing and captureless
/// methods. The runtime captured *values* live in transient BytecodeClosures
/// created at `MAKE_BLOCK` and are intentionally not persisted; they are
/// recreated from the live parent frame on each block creation.
pub fn rematerializeClosures(
    allocator: std.mem.Allocator,
    store: *const FactStore,
    table: *ObjectTable,
) RematError!usize {
    var count: usize = 0;
    var it = store.entitiesWith(kernel_eids.ATTR_BYTECODE_STREAM);
    while (it.next()) |entry| {
        const method_eid = entry.e;
        if (table.get(method_eid) != null) continue; // already live

        const total = 3 * @sizeOf(Ref);
        const buf = allocator.alignedAlloc(u8, .of(Ref), total) catch return error.OutOfMemory;
        @memset(buf, 0);
        const vt_slot: *Ref = @ptrCast(@alignCast(buf.ptr));
        vt_slot.* = ref_mod.mkEid(kernel_eids.BYTECODE_METHOD_VT);
        const c_ptr: *closure_mod.Closure = @ptrCast(@alignCast(buf.ptr + @sizeOf(Ref)));
        c_ptr.* = .{
            .method = @ptrCast(&dispatch.bytecodeDispatcherImpl),
            .data = ref_mod.mkEid(method_eid),
        };
        table.put(method_eid, ref_mod.mkPointer(@ptrCast(c_ptr))) catch return error.StoreError;
        count += 1;
    }

    // W1 Stage 2/2b: rebuild block-closure WRAPPERS from their recipe facts
    // (`[wrapper :closure-block-method bm]`). The block-method itself was rebuilt
    // above; here we re-mint the CLOSURE_VT wrapper at its original eid (so a
    // binding's `mkEid(wrapper)` resolves), wrapping a fresh BytecodeClosure for
    // that method. A capturing wrapper (Stage 2b) carries a companion
    // `[wrapper :closure-block-captures <rrb>]` fact whose elements we copy into
    // the closure's inline capture tail (the RRB box was already reconstructed
    // during fact replay). parent_self / parent_frame_id are NIL/0: a durable
    // global block has no live enclosing frame and references no meaningful self.
    if (g_closure_block_method_attr != 0) {
        var wit = store.entitiesWith(g_closure_block_method_attr);
        while (wit.next()) |entry| {
            const wrapper_eid = entry.e;
            if (table.get(wrapper_eid) != null) continue; // already live
            if (!ref_mod.isEid(entry.v)) continue;
            const block_method_eid = ref_mod.untagEid(entry.v);

            var n_captures: u32 = 0;
            const captures_vec: ?Ref = if (g_closure_block_captures_attr != 0)
                store.currentValue(wrapper_eid, g_closure_block_captures_attr)
            else
                null;
            if (captures_vec) |cv| {
                if (boxed.isRrbRef(cv)) n_captures = @intCast(boxed.rrbLen(cv));
            }

            const blk = vm_mod.allocBytecodeBlock(allocator, block_method_eid, ref_mod.NIL, 0, n_captures) catch return error.OutOfMemory;
            if (captures_vec) |cv| {
                if (boxed.isRrbRef(cv)) {
                    var i: usize = 0;
                    while (i < n_captures) : (i += 1) blk.captures_tail[i] = boxed.rrbNth(cv, i);
                }
            }

            const total = 3 * @sizeOf(Ref);
            const buf = allocator.alignedAlloc(u8, .of(Ref), total) catch return error.OutOfMemory;
            @memset(buf, 0);
            const vt_slot: *Ref = @ptrCast(@alignCast(buf.ptr));
            vt_slot.* = ref_mod.mkEid(kernel_eids.CLOSURE_VT);
            const c_ptr: *closure_mod.Closure = @ptrCast(@alignCast(buf.ptr + @sizeOf(Ref)));
            c_ptr.* = .{ .method = @ptrCast(&dispatch.bytecodeDispatcherImpl), .data = blk.ref };
            table.put(wrapper_eid, ref_mod.mkPointer(@ptrCast(c_ptr))) catch return error.StoreError;
            count += 1;
        }
    }

    // W1 Stage 2b part 2: rebuild lang TREE-WALKER block-closure wrappers from
    // their `[wrapper :treewalk-block-ast ast][wrapper :treewalk-block-scope
    // scope]` recipe facts. The closure's behavior lives in the module's
    // `blockEvalShim` (registered via the ABI), reapplied here. We re-mint, at
    // the original wrapper eid, the `allocBlockClosure` shape: a 2-slot
    // `OBJECT_VT { ast, scope }` data object wrapped by `Closure { shim, &data }`.
    // The AST + scope entities were replayed as ordinary facts; this only rebuilds
    // the runtime wrapper that points at them. Skipped when lang is not loaded.
    if (g_treewalk_shim != null and g_treewalk_block_ast_attr != 0) {
        const shim = g_treewalk_shim.?;
        var tit = store.entitiesWith(g_treewalk_block_ast_attr);
        while (tit.next()) |entry| {
            const wrapper_eid = entry.e;
            if (table.get(wrapper_eid) != null) continue; // already live
            if (!ref_mod.isEid(entry.v)) continue;
            const ast_ref = entry.v;
            const scope_ref = store.currentValue(wrapper_eid, g_treewalk_block_scope_attr) orelse continue;

            const data_obj = object.allocObject(allocator, ref_mod.mkEid(kernel_eids.OBJECT_VT), 2) catch return error.OutOfMemory;
            object.setSlot(data_obj, 0, ast_ref);
            object.setSlot(data_obj, 1, scope_ref);

            const total = 3 * @sizeOf(Ref);
            const buf = allocator.alignedAlloc(u8, .of(Ref), total) catch return error.OutOfMemory;
            @memset(buf, 0);
            const vt_slot: *Ref = @ptrCast(@alignCast(buf.ptr));
            vt_slot.* = ref_mod.mkEid(kernel_eids.CLOSURE_VT);
            const c_ptr: *closure_mod.Closure = @ptrCast(@alignCast(buf.ptr + @sizeOf(Ref)));
            c_ptr.* = .{ .method = shim, .data = object.toRef(data_obj) };
            table.put(wrapper_eid, ref_mod.mkPointer(@ptrCast(c_ptr))) catch return error.StoreError;
            count += 1;
        }
    }
    return count;
}

// -- byte helpers -----------------------------------------------------------

fn appendU32(allocator: std.mem.Allocator, buf: *std.ArrayList(u8), v: u32) WriteError!void {
    var tmp: [4]u8 = undefined;
    std.mem.writeInt(u32, &tmp, v, .little);
    try buf.appendSlice(allocator, &tmp);
}

fn appendU64(allocator: std.mem.Allocator, buf: *std.ArrayList(u8), v: u64) WriteError!void {
    var tmp: [8]u8 = undefined;
    std.mem.writeInt(u64, &tmp, v, .little);
    try buf.appendSlice(allocator, &tmp);
}

const Cursor = struct {
    bytes: []const u8,
    pos: usize = 0,

    fn take(self: *Cursor, n: usize) ReadError![]const u8 {
        if (self.pos + n > self.bytes.len) return error.Truncated;
        const s = self.bytes[self.pos .. self.pos + n];
        self.pos += n;
        return s;
    }

    fn readU32(self: *Cursor) ReadError!u32 {
        const s = try self.take(4);
        return std.mem.readInt(u32, s[0..4], .little);
    }

    fn readU64(self: *Cursor) ReadError!u64 {
        const s = try self.take(8);
        return std.mem.readInt(u64, s[0..8], .little);
    }
};
