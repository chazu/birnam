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

/// Oldest on-disk version this loader still accepts. v5 and v6 share every
/// section layout — v6 only ADDS the VK_MAP value kind in the facts stream — so
/// a v6 loader reads a v5 image verbatim (a v5 image just has no VK_MAP bytes).
/// Keeps pre-v6 images (e.g. a baked `aot_embed.bim`) loadable.
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
pub fn write(
    allocator: std.mem.Allocator,
    store: *const FactStore,
    next_eid: u64,
    manifest: Manifest,
    interner: ?*const Interner,
) WriteError!WriteResult {
    var buf = std.ArrayList(u8){};
    errdefer buf.deinit(allocator);

    try buf.appendSlice(allocator, &MAGIC);
    try appendU32(allocator, &buf, FORMAT_VERSION);

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

    // `meat` keys facts by module first, giving (m, e, a, v) directly.
    // Provenance split (Phase 12.0): persist DATA facts only. CODE facts
    // (bootstrap + module-init installs) are reconstructed on load by
    // re-running module init, so they are never serialized.
    var m_it = store.meatIter();
    while (m_it.next()) |entry| {
        if (entry.m != kernel_eids.M_USER) continue;

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
    if (version < MIN_LOAD_VERSION or version > FORMAT_VERSION) return error.BadVersion;

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
    if (version < MIN_LOAD_VERSION or version > FORMAT_VERSION) return error.BadVersion;
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
