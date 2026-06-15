//! Phase 9.5.1: Module wrapper.
//!
//! Wraps Phase 9's flat `ffi.registerWrap` primitive in a send-dispatchable
//! Module surface. Each loaded library is an entity:
//!
//!   `[mod_eid :vtable <instance_vt>]`     - per-instance delegate vt
//!   `[mod_eid :ffi-handle <FfiPtr Ref>]`  - raw dlopen handle
//!   `[mod_eid :ffi-path <string Ref>]`    - filesystem path
//!
//! The instance vt is a fresh user-allocated vtable that delegates to
//! `MODULE_VT`. Shared methods (`wrap:as:sig:`, `path`, `handle`) live on
//! `MODULE_VT`; FFI-call dispatch closures installed by `wrap:` live on
//! the instance vt so they're scoped to a single Module.
//!
//! Failure modes return a diagnostic entity with vtable = `ERROR_VT` and
//! `:error-kind <interned-name-EID-Ref>` plus context facts. Caller checks
//! `object.vtableOf(result) == mkEid(ERROR_VT)` to discriminate.

const std = @import("std");
const ref_mod = @import("ref.zig");
const object = @import("object.zig");
const strings = @import("strings.zig");
const boxed = @import("boxed.zig");
const closure_mod = @import("closure.zig");
const fact_store_mod = @import("fact_store.zig");
const object_table_mod = @import("object_table.zig");
const interner_mod = @import("interner.zig");
const kernel_eids = @import("kernel_eids.zig");
const send_mod = @import("send.zig");
const ffi = @import("ffi.zig");
const id_api_mod = @import("id_api.zig");
const macho = @import("macho.zig");
const loader = @import("loader.zig");
const promotion_mod = @import("promotion.zig");

const Ref = ref_mod.Ref;
const Send = send_mod.Send;

// -- Module load: -----------------------------------------------------------

/// Mint a new per-Module delegate vt that delegates to `MODULE_VT`.
/// Returns the new vt EID.
fn mintInstanceVt(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
) !u64 {
    const vt_obj = try object.allocObject(allocator, ref_mod.mkEid(kernel_eids.VTABLE_VT), 0);
    const vt_eid = table.alloc();
    try table.put(vt_eid, object.toRef(vt_obj));
    _ = try store.assert(vt_eid, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.VTABLE_VT), kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(vt_eid, kernel_eids.ATTR_DELEGATE, ref_mod.mkEid(kernel_eids.MODULE_VT), kernel_eids.M_BOOTSTRAP);
    return vt_eid;
}

/// Top-level `Module load:` — mints a Module object recording only the
/// DECLARATIVE `:ffi-path` (M_USER, persisted). NO `dlopen` here: the
/// side-effecting open is deferred to `resolveModule` (run at REPL line-commit,
/// file-in commit, image load). The fresh module still gets a per-instance
/// vtable now so `wrap:`/`path`/`symbols` dispatch before resolution.
pub fn moduleLoad(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
    interner: *interner_mod.Interner,
    path: []const u8,
) !Ref {
    _ = interner;
    const mod_eid = table.alloc();
    // :ffi-path is the durable truth (M_USER → persisted, freezable).
    _ = try store.assert(
        mod_eid,
        kernel_eids.ATTR_FFI_PATH,
        try strings.mkString(allocator, path),
        kernel_eids.M_USER,
    );
    _ = try ensureInstanceVt(allocator, store, table, mod_eid);
    return ref_mod.mkEid(mod_eid);
}

/// Ensure `mod_eid` has a materialized per-instance vtable (delegating to
/// `MODULE_VT`) and a `:vtable` fact pointing at it. Idempotent: reuses an
/// existing materialized vt; re-mints when the vt is absent (fresh load) or
/// its object was not restored (image reload — vt facts are transient
/// M_BOOTSTRAP). Returns the instance-vt EID.
fn ensureInstanceVt(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
    mod_eid: u64,
) !u64 {
    if (store.currentValue(mod_eid, kernel_eids.ATTR_VTABLE)) |vt_ref| {
        if (ref_mod.isEid(vt_ref)) {
            const vt_eid = ref_mod.untagEid(vt_ref);
            // Reuse the vt if it still EXISTS as a vtable entity (its own
            // `:vtable` fact is present), NOT if it is merely materialized in the
            // object table. The two are different: `mintInstanceVt` materializes
            // the vt then asserts its `:vtable`/`:delegate` facts, and the Phase-8
            // D6 hook demotes any materialized entity that an assert touches — so
            // the vt is demoted (table slot cleared) the instant it is created.
            // It stays a perfectly valid, dispatchable entity (dispatch is
            // fact-based; cold entities resolve via the fact store). Testing
            // `table.get != null` here therefore failed every time and re-minted a
            // fresh empty vt on every resolve, dropping all installed selectors.
            // The `:vtable` fact is M_BOOTSTRAP (transient) — absent after an
            // image `:load`, so reload correctly re-mints.
            if (store.currentValue(vt_eid, kernel_eids.ATTR_VTABLE) != null) return vt_eid;
        }
    }
    const instance_vt = try mintInstanceVt(allocator, store, table);
    _ = try store.assert(mod_eid, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(instance_vt), kernel_eids.M_BOOTSTRAP);
    return instance_vt;
}

/// Resolve one module: materialize its vt, `dlopen` the lib if not already
/// open this process, then for each declarative wrap lacking a resolved
/// `:ffi-addr`, `dlsym` the symbol and install a dispatch closure. Idempotent
/// and incremental (new wraps on an already-open module just get installed).
/// On a missing dylib / symbol the wrap is skipped (left unresolved); callers
/// that need hard errors should check separately.
pub fn resolveModule(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
    mod_eid: u64,
) !void {
    const instance_vt = try ensureInstanceVt(allocator, store, table, mod_eid);

    // dlopen once per process. Handle is transient (M_BOOTSTRAP, never saved).
    // RTLD_GLOBAL (not std.DynLib's default RTLD_LOCAL): a wrapped library's
    // symbols join the global flat-namespace pool, so a dependency loaded first
    // satisfies a later library's undefined refs. E.g. libtecla links only
    // libSystem yet calls terminfo's setupterm/tputs via flat namespace; loading
    // `/usr/lib/libncurses.dylib` as a Module first now provides them. The raw
    // dlopen handle (a void*) is stored directly as the FfiPtr — no DynLib
    // wrapper. LAZY matches the prior behavior.
    if (store.currentValue(mod_eid, kernel_eids.ATTR_FFI_HANDLE) == null) {
        const path_ref = store.currentValue(mod_eid, kernel_eids.ATTR_FFI_PATH) orelse return;
        if (!strings.isString(path_ref)) return;
        const path_z = allocator.dupeZ(u8, strings.bytes(path_ref)) catch return;
        defer allocator.free(path_z);
        const handle = std.c.dlopen(path_z, .{ .LAZY = true, .GLOBAL = true }) orelse return;
        _ = try store.assert(
            mod_eid,
            kernel_eids.ATTR_FFI_HANDLE,
            try boxed.mkFfiPtr(allocator, @intFromPtr(handle)),
            kernel_eids.M_BOOTSTRAP,
        );
    }

    // Install each wrap onto the current instance vt. Wraps are entities tagged
    // [wrap :ffi-owner mod] carrying :ffi-wrap-symbol / :ffi-sig-str /
    // :wrap-selector.
    //
    // Snapshot the wrap eids BEFORE installing: buildWrapClosure + the vt-method
    // assert below mutate the store, which invalidates a live `entitiesMatching`
    // iterator (HAMT path-copy) — iterating-while-asserting would install only
    // the first wrap and silently skip the rest.
    var wrap_eids = std.ArrayList(u64){};
    defer wrap_eids.deinit(allocator);
    {
        var it = store.entitiesMatching(kernel_eids.ATTR_FFI_OWNER, ref_mod.mkEid(mod_eid));
        while (it.next()) |wrap_eid| try wrap_eids.append(allocator, wrap_eid);
    }
    for (wrap_eids.items) |wrap_eid| {
        const sym_ref = store.currentValue(wrap_eid, kernel_eids.ATTR_FFI_WRAP_SYMBOL) orelse continue;
        const sig_ref = store.currentValue(wrap_eid, kernel_eids.ATTR_FFI_SIG_STR) orelse continue;
        const sel_ref = store.currentValue(wrap_eid, kernel_eids.ATTR_WRAP_SELECTOR) orelse continue;
        if (!strings.isString(sym_ref) or !strings.isString(sig_ref) or !ref_mod.isEid(sel_ref)) continue;
        const sel_eid = ref_mod.untagEid(sel_ref);

        // Self-healing: skip only if THIS instance vt already carries the
        // selector. Keying the skip on the wrap's `:ffi-addr` (resolved-once)
        // would not re-install after the instance vt is re-minted — e.g. GC
        // reclaiming the old vt object between REPL lines — silently dropping
        // every selector after the first. Re-resolving a wrap is cheap and
        // buildWrapClosure already supersedes its prior `:ffi-sig`.
        if (store.currentValue(instance_vt, sel_eid) != null) continue;

        const name_z = allocator.dupeZ(u8, strings.bytes(sym_ref)) catch continue;
        defer allocator.free(name_z);
        const addr = moduleSymbolAddr(store, mod_eid, name_z) orelse continue;
        const closure_eid = ffi.buildWrapClosure(allocator, store, table, wrap_eid, addr, strings.bytes(sig_ref)) catch continue;
        _ = store.assert(instance_vt, sel_eid, ref_mod.mkEid(closure_eid), kernel_eids.M_BOOTSTRAP) catch continue;
    }
}

/// Resolve every pending module: any entity carrying `:ffi-path`. Run at REPL
/// line-commit, file-in commit, and image load.
pub fn resolveAllPending(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
) !void {
    var it = store.entitiesWith(kernel_eids.ATTR_FFI_PATH);
    while (it.next()) |e| try resolveModule(allocator, store, table, e.e);
}

/// Look up a symbol via the raw dlopen handle stored on the Module. Returns
/// null if the symbol isn't found.
pub fn moduleSymbolAddr(
    store: *const fact_store_mod.FactStore,
    mod_eid: u64,
    name: [:0]const u8,
) ?usize {
    const handle_ref = store.currentValue(mod_eid, kernel_eids.ATTR_FFI_HANDLE) orelse return null;
    if (!boxed.isFfiPtr(handle_ref)) return null;
    const handle: *anyopaque = @ptrFromInt(boxed.untagFfiPtr(handle_ref));
    const sym = std.c.dlsym(handle, name.ptr) orelse return null;
    return @intFromPtr(sym);
}

/// `[Module load: <path-string-Ref>]` — class-side method installed on
/// `MODULE_CLASS_VT`. Decodes the boxed-string path arg and delegates to
/// `moduleLoad`. Returns the new Module entity Ref, or an `ERROR_VT` entity
/// (bad-arg / dlopen failure).
fn methodLoad(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    _ = n_args;
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    const table = send_mod.tablePtr() orelse return ref_mod.NIL;
    const alloc = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const interner = id_api_mod.internerPtr() orelse return ref_mod.NIL;

    const path_ref = args[0];
    if (!strings.isString(path_ref)) {
        return errorEntity(alloc, store, table, interner, "ffi-load-failure", "load: path not a string", &.{}) catch ref_mod.NIL;
    }
    return moduleLoad(alloc, store, table, interner, strings.bytes(path_ref)) catch ref_mod.NIL;
}

// -- Diagnostic entities ----------------------------------------------------

const ErrorAttr = struct { attr: u64, value: Ref };

/// Mint a diagnostic entity with `:vtable ERROR_VT`, `:error-kind` (interned
/// name), `:error-message`, plus optional extra facts.
pub fn errorEntity(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
    interner: *interner_mod.Interner,
    kind: []const u8,
    message: []const u8,
    extras: []const ErrorAttr,
) !Ref {
    const err_eid = table.alloc();
    _ = try store.assert(err_eid, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.ERROR_VT), kernel_eids.M_BOOTSTRAP);
    const kind_eid = try interner.intern(store, table, kind);
    _ = try store.assert(err_eid, kernel_eids.ATTR_ERROR_KIND, ref_mod.mkEid(kind_eid), kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(
        err_eid,
        kernel_eids.ATTR_ERROR_MESSAGE,
        try strings.mkString(allocator, message),
        kernel_eids.M_BOOTSTRAP,
    );
    for (extras) |e| {
        _ = try store.assert(err_eid, e.attr, e.value, kernel_eids.M_BOOTSTRAP);
    }
    return ref_mod.mkEid(err_eid);
}

/// Returns true iff `r` is an EID-tagged Ref pointing at a diagnostic
/// entity (i.e., its `:vtable` is `ERROR_VT`).
pub fn isErrorEntity(store: *const fact_store_mod.FactStore, r: Ref) bool {
    if (!ref_mod.isEid(r)) return false;
    const e = ref_mod.untagEid(r);
    const vt = store.currentValue(e, kernel_eids.ATTR_VTABLE) orelse return false;
    return ref_mod.isEid(vt) and ref_mod.untagEid(vt) == kernel_eids.ERROR_VT;
}

// -- Method impls installed on MODULE_VT ------------------------------------

/// `[mod symbols]` — return an RRB-Ref of symbol-entity Refs for this Module.
/// First call walks Mach-O via the bundled reader and asserts symbol
/// entities + `:ffi-has-symbol` facts (`:many` cardinality). Subsequent
/// calls are pure queries via `currentValue` thanks to the cardinality
/// machinery returning the materialized RRB directly.
fn methodSymbols(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    const table = send_mod.tablePtr() orelse return ref_mod.NIL;
    const alloc = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    if (!ref_mod.isEid(s.receiver)) return ref_mod.NIL;
    const mod_eid = ref_mod.untagEid(s.receiver);

    // Cache check.
    const already = store.currentValue(mod_eid, kernel_eids.ATTR_FFI_SYMBOLS_ENUMERATED) != null;
    if (!already) {
        enumerateSymbols(alloc, store, table, mod_eid) catch {
            // On any failure (file read, parse, OOM), surface as NIL.
            // Tests that exercise the failure path use moduleLoad's error
            // path; symbols enumeration on a successfully-loaded lib is
            // expected to succeed.
            return ref_mod.NIL;
        };
    }

    // After enumeration, currentValue on a :many attr returns RRB-Ref.
    return store.currentValue(mod_eid, kernel_eids.ATTR_FFI_HAS_SYMBOL) orelse ref_mod.NIL;
}

/// Walk the Module's on-disk image via `macho.Reader.symbols()`, allocate
/// one symbol entity per nlist, assert facts, and mark the Module's
/// enumeration cache flag.
fn enumerateSymbols(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
    mod_eid: u64,
) !void {
    const path_ref = store.currentValue(mod_eid, kernel_eids.ATTR_FFI_PATH) orelse return error.NoPath;
    if (!strings.isString(path_ref)) return error.NoPath;
    const path = strings.bytes(path_ref);

    const bytes_buf = try macho.readFile(allocator, path);
    defer allocator.free(bytes_buf);

    // Format-agnostic: loader.openSymbols sniffs magic + dispatches via
    // the fact-shaped reader registry. Mach-O on darwin, ELF on linux.
    var it = try loader.openSymbols(allocator, store, bytes_buf);
    defer it.deinit();
    while (it.next()) |sym| {
        const sym_eid = table.alloc();
        _ = try store.assert(
            sym_eid,
            kernel_eids.ATTR_FFI_SYMBOL_NAME,
            try strings.mkString(allocator, sym.name),
            kernel_eids.M_BOOTSTRAP,
        );
        _ = try store.assert(
            sym_eid,
            kernel_eids.ATTR_FFI_SYMBOL_ADDR,
            try boxed.mkFfiPtr(allocator, @intCast(sym.addr)),
            kernel_eids.M_BOOTSTRAP,
        );
        // Kind: store as SmallInt enum tag so it's queryable.
        const kind_tag: i64 = switch (sym.kind) {
            .section => 1,
            .absolute => 2,
            .undefined_ => 3,
            .other => 4,
        };
        _ = try store.assert(
            sym_eid,
            kernel_eids.ATTR_FFI_SYMBOL_KIND,
            ref_mod.mkSmallInt(kind_tag).?,
            kernel_eids.M_BOOTSTRAP,
        );
        _ = try store.assert(
            sym_eid,
            kernel_eids.ATTR_FFI_SYMBOL_EXTERNAL,
            ref_mod.mkSmallInt(if (sym.external) 1 else 0).?,
            kernel_eids.M_BOOTSTRAP,
        );
        _ = try store.assert(
            sym_eid,
            kernel_eids.ATTR_FFI_SYMBOL_MODULE,
            ref_mod.mkEid(mod_eid),
            kernel_eids.M_BOOTSTRAP,
        );
        _ = try store.assert(
            mod_eid,
            kernel_eids.ATTR_FFI_HAS_SYMBOL,
            ref_mod.mkEid(sym_eid),
            kernel_eids.M_BOOTSTRAP,
        );
    }

    _ = try store.assert(
        mod_eid,
        kernel_eids.ATTR_FFI_SYMBOLS_ENUMERATED,
        ref_mod.mkSmallInt(1).?,
        kernel_eids.M_BOOTSTRAP,
    );
}

/// `[mod path]` — return the boxed string Ref stored on the Module.
fn methodPath(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    if (!ref_mod.isEid(s.receiver)) return ref_mod.NIL;
    return store.currentValue(ref_mod.untagEid(s.receiver), kernel_eids.ATTR_FFI_PATH) orelse ref_mod.NIL;
}

/// `[mod handle]` — return the boxed FfiPtr Ref stored on the Module.
fn methodHandle(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    if (!ref_mod.isEid(s.receiver)) return ref_mod.NIL;
    return store.currentValue(ref_mod.untagEid(s.receiver), kernel_eids.ATTR_FFI_HANDLE) orelse ref_mod.NIL;
}

/// `[mod wrap: <sel-Ref> as: <name-string-Ref> signature: <sig-string-Ref>]`
/// DECLARATIVE: mints a wrap entity carrying the durable, relinkable facts
/// (`:ffi-owner` / `:ffi-wrap-symbol` / `:ffi-sig-str` / `:wrap-selector`) — all
/// M_USER so they persist into images and survive cross-process freeze. NO
/// `dlsym`, NO closure install here; the actual binding happens in
/// `resolveModule` (deferred to commit). Returns `self` (chainable). Sig is
/// validated lazily at resolve time, so a bad symbol/sig surfaces there.
fn methodWrapAsSig(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = n_args;
    const sel_ref = args[0];
    const name_ref = args[1];
    const sig_ref = args[2];
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    const table = send_mod.tablePtr() orelse return ref_mod.NIL;
    const alloc = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const interner = id_api_mod.internerPtr() orelse return ref_mod.NIL;

    if (!ref_mod.isEid(s.receiver)) return ref_mod.NIL;
    if (!ref_mod.isEid(sel_ref)) return ref_mod.NIL;
    const mod_eid = ref_mod.untagEid(s.receiver);

    if (!strings.isString(name_ref)) {
        return errorEntity(alloc, store, table, interner, "ffi-sig-parse-failure", "wrap: name not a string", &.{}) catch ref_mod.NIL;
    }
    if (!strings.isString(sig_ref)) {
        return errorEntity(alloc, store, table, interner, "ffi-sig-parse-failure", "wrap: sig not a string", &.{}) catch ref_mod.NIL;
    }

    // Mint the declarative wrap. Dup the strings into kernel-owned storage
    // (the source Refs may be arena-scoped to the calling form).
    const wrap_eid = table.alloc();
    const name_dup = strings.mkString(alloc, strings.bytes(name_ref)) catch return ref_mod.NIL;
    const sig_dup = strings.mkString(alloc, strings.bytes(sig_ref)) catch return ref_mod.NIL;
    _ = store.assert(wrap_eid, kernel_eids.ATTR_FFI_OWNER, ref_mod.mkEid(mod_eid), kernel_eids.M_USER) catch return ref_mod.NIL;
    _ = store.assert(wrap_eid, kernel_eids.ATTR_FFI_WRAP_SYMBOL, name_dup, kernel_eids.M_USER) catch return ref_mod.NIL;
    _ = store.assert(wrap_eid, kernel_eids.ATTR_FFI_SIG_STR, sig_dup, kernel_eids.M_USER) catch return ref_mod.NIL;
    _ = store.assert(wrap_eid, kernel_eids.ATTR_WRAP_SELECTOR, sel_ref, kernel_eids.M_USER) catch return ref_mod.NIL;

    return s.receiver; // chainable
}

/// `[Module resolve]` (class-side, MODULE_PROTO_VT) — resolve all pending
/// modules (open libs + bind declarative wraps). Returns `self`.
fn methodResolve(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    const table = send_mod.tablePtr() orelse return ref_mod.NIL;
    const alloc = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    resolveAllPending(alloc, store, table) catch {};
    return s.receiver;
}

/// `[mod unload]` — refuse while `ffi_depth > 0`; otherwise walk all
/// boxed FFI pointers minted by this Module (`[?p :ffi-owner mod]`),
/// stale them by asserting `[p :ffi-stale? mkSmallInt(1)]`, retract wrap
/// closures so subsequent sends miss method lookup, then `dlclose` the
/// handle. The Module entity itself is retained for walk-back ("this
/// path was loaded at tx N, unloaded at tx M").
fn methodUnload(s: *send_mod.Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    const alloc = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const interner = id_api_mod.internerPtr() orelse return ref_mod.NIL;
    const table = send_mod.tablePtr() orelse return ref_mod.NIL;
    _ = table;

    if (!ref_mod.isEid(s.receiver)) return ref_mod.NIL;
    const mod_eid = ref_mod.untagEid(s.receiver);

    // Refuse if any FFI call is currently active.
    if (promotion_mod.ffiDepth() > 0) {
        return errorEntity(
            alloc,
            store,
            send_mod.tablePtr().?,
            interner,
            "ffi-unload-blocked-active-call",
            "Module unload refused: ffi_depth > 0",
            &.{},
        ) catch ref_mod.NIL;
    }

    // Walk owned entities and stale all of them. The owned set is whatever
    // has `[?e :ffi-owner mkEid(mod)]` — covers boxed FFI pointers AND
    // wrap entities (both get the fact at allocation time).
    const owner_ref = ref_mod.mkEid(mod_eid);
    var it = store.entitiesMatching(kernel_eids.ATTR_FFI_OWNER, owner_ref);
    const stale_true = ref_mod.mkSmallInt(1).?;
    while (it.next()) |owned_eid| {
        // Phase 15.E: free the wrap's heap Sig holder before staling. No-op for
        // owned boxed-ffi-ptr entities (no :ffi-sig). ffi_depth==0 here (checked
        // above), so no in-flight call holds the Sig.
        ffi.freeSig(store, alloc, owned_eid);
        _ = store.assert(owned_eid, kernel_eids.ATTR_FFI_STALE, stale_true, kernel_eids.M_BOOTSTRAP) catch {};
    }

    // dlclose the raw handle (the FfiPtr IS the dlopen handle now).
    if (store.currentValue(mod_eid, kernel_eids.ATTR_FFI_HANDLE)) |handle_ref| {
        if (boxed.isFfiPtr(handle_ref)) {
            const handle: *anyopaque = @ptrFromInt(boxed.untagFfiPtr(handle_ref));
            _ = std.c.dlclose(handle);
        }
    }

    // Mark the Module itself as unloaded (a stale flag at Module scope).
    _ = store.assert(mod_eid, kernel_eids.ATTR_FFI_STALE, stale_true, kernel_eids.M_BOOTSTRAP) catch {};

    return s.receiver; // self
}

// -- Bootstrap-time method install ------------------------------------------

const Closure = closure_mod.Closure;

fn allocClosure(allocator: std.mem.Allocator, method_fn: *const anyopaque) !*Closure {
    const total = 3 * @sizeOf(Ref);
    const buf = try allocator.alignedAlloc(u8, .of(Ref), total);
    @memset(buf, 0);
    const vt_slot: *Ref = @ptrCast(@alignCast(buf.ptr));
    vt_slot.* = ref_mod.mkEid(kernel_eids.CLOSURE_VT);
    const c: *Closure = @ptrCast(@alignCast(buf.ptr + @sizeOf(Ref)));
    c.method = method_fn;
    c.data = ref_mod.NIL;
    return c;
}

// -- Phase 18 prelude: FFI memory primitives --------------------------------
//
// `alloc:` on MODULE_PROTO_VT; peek/poke/free/address/isNull on FFI_PTR_VT.
// All addressing is a base (the receiver FfiPtr) + a byte offset arg. Reads
// and writes use align(1) pointers so unaligned struct fields are safe.

fn ffiBase(s: *Send) ?usize {
    if (!boxed.isFfiPtr(s.receiver)) return null;
    return boxed.untagFfiPtr(s.receiver);
}

fn argOffset(args: [*]const Ref, idx: usize) ?usize {
    const v = args[idx];
    if (!ref_mod.isSmallInt(v)) return null;
    const n = ref_mod.untagSmallInt(v);
    if (n < 0) return null;
    return @intCast(n);
}

/// `Module alloc: n` — libc-malloc `n` zeroed bytes; return a boxed FfiPtr.
/// Raw (not arena/GC) so the address is stable for C and freed via `[p free]`.
fn methodAlloc(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    if (n_args < 1 or !ref_mod.isSmallInt(args[0])) return ref_mod.NIL;
    const n = ref_mod.untagSmallInt(args[0]);
    if (n <= 0) return ref_mod.NIL;
    const alloc = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const raw = std.c.malloc(@intCast(n)) orelse return ref_mod.NIL;
    @memset(@as([*]u8, @ptrCast(raw))[0..@intCast(n)], 0);
    return boxed.mkFfiPtr(alloc, @intFromPtr(raw)) catch ref_mod.NIL;
}

/// `Module argv: aVector` (Phase 19.3) — build a NULL-terminated C `char**`
/// from a birnam vector of strings, returned as a boxed FfiPtr (e.g. for an
/// `execv`/`getopt`-style `char* argv[]` arg). One `std.c.malloc` holds the
/// whole thing: the pointer array (n + 1 slots, last = NULL) followed by the
/// NUL-terminated string bytes, each slot pointing into the bytes region. So a
/// single `[p free]` reclaims everything — no per-string leak. Returns nil if
/// the arg isn't a vector of strings.
fn methodArgv(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = s;
    if (n_args < 1 or !boxed.isRrbRef(args[0])) return ref_mod.NIL;
    const vec = args[0];
    const n = boxed.rrbLen(vec);
    const ptr_sz = @sizeOf(usize);

    // First pass: validate every element is a string + sum the byte region.
    var bytes_total: usize = 0;
    var i: usize = 0;
    while (i < n) : (i += 1) {
        const e = boxed.rrbNth(vec, i);
        if (!strings.isString(e)) return ref_mod.NIL;
        bytes_total += strings.bytes(e).len + 1; // + NUL
    }

    const ptr_region = (n + 1) * ptr_sz; // + 1 for the NULL terminator slot
    const total = ptr_region + bytes_total;
    const alloc = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const raw = std.c.malloc(total) orelse return ref_mod.NIL;
    const base: usize = @intFromPtr(raw);
    const slots: [*]usize = @ptrCast(@alignCast(raw));

    var cursor = ptr_region;
    i = 0;
    while (i < n) : (i += 1) {
        const sb = strings.bytes(boxed.rrbNth(vec, i));
        const dst: [*]u8 = @ptrFromInt(base + cursor);
        @memcpy(dst[0..sb.len], sb);
        dst[sb.len] = 0;
        slots[i] = base + cursor;
        cursor += sb.len + 1;
    }
    slots[n] = 0; // NULL terminator

    return boxed.mkFfiPtr(alloc, base) catch ref_mod.NIL;
}

// Phase 19.7 — out-param cell width tags (boxed FfiPtr slot 2). The tag encodes
// both the scratch size and how `[cell value]` reinterprets the bytes.
const CELL_W_INT: u64 = 1; // C int    — 4 bytes, read i32 (sign-extended) -> SmallInt
const CELL_W_LONG: u64 = 2; // C long   — 8 bytes, read i64               -> SmallInt
const CELL_W_PTR: u64 = 3; // pointer   — 8 bytes, read usize             -> FfiPtr
const CELL_W_DOUBLE: u64 = 4; // double  — 8 bytes, read f64               -> Float

fn cellWidthSize(tag: u64) usize {
    return switch (tag) {
        CELL_W_INT => 4,
        else => 8, // long / ptr / double
    };
}

/// Mint a zeroed scratch buffer of the tag's width and box it as an out-param
/// cell. Shared by the four `out*` constructors.
fn makeCell(tag: u64) Ref {
    const alloc = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const sz = cellWidthSize(tag);
    const raw = std.c.malloc(sz) orelse return ref_mod.NIL;
    @memset(@as([*]u8, @ptrCast(raw))[0..sz], 0);
    return boxed.mkFfiPtrCell(alloc, @intFromPtr(raw), tag) catch ref_mod.NIL;
}

/// `[Module outInt]` / `outLong` / `outPtr` / `outDouble` (Phase 19.7) — mint an
/// out-param cell over a fresh, zeroed scratch buffer. Pass it where C expects
/// `T*`, then `[cell value]` reads the written value and `[cell free]` releases it.
fn methodOutInt(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = .{ s, args, n_args };
    return makeCell(CELL_W_INT);
}
fn methodOutLong(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = .{ s, args, n_args };
    return makeCell(CELL_W_LONG);
}
fn methodOutPtr(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = .{ s, args, n_args };
    return makeCell(CELL_W_PTR);
}
fn methodOutDouble(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = .{ s, args, n_args };
    return makeCell(CELL_W_DOUBLE);
}

/// `[cell value]` (Phase 19.7, reuses SEL_VALUE) — peek the cell's buffer at the
/// width its tag remembers. Returns nil if the receiver is a plain FfiPtr (tag 0,
/// not a cell), so a bare pointer doesn't silently deref as a random width.
fn methodCellValue(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = .{ args, n_args };
    const base = ffiBase(s) orelse return ref_mod.NIL;
    const alloc = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    switch (boxed.ffiPtrWidth(s.receiver)) {
        CELL_W_INT => {
            const p: *align(1) const i32 = @ptrFromInt(base);
            return ref_mod.mkSmallInt(@intCast(p.*)) orelse ref_mod.NIL;
        },
        CELL_W_LONG => {
            const p: *align(1) const i64 = @ptrFromInt(base);
            return ref_mod.mkSmallInt(p.*) orelse ref_mod.NIL;
        },
        CELL_W_PTR => {
            const p: *align(1) const usize = @ptrFromInt(base);
            return boxed.mkFfiPtr(alloc, p.*) catch ref_mod.NIL;
        },
        CELL_W_DOUBLE => {
            const p: *align(1) const f64 = @ptrFromInt(base);
            return boxed.mkFloat(alloc, p.*) catch ref_mod.NIL;
        },
        else => return ref_mod.NIL, // not an out-param cell
    }
}

/// `[p free]` — libc-free the address. Only valid for `alloc:`-minted pointers.
fn methodFfiFree(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const base = ffiBase(s) orelse return ref_mod.NIL;
    if (base != 0) std.c.free(@ptrFromInt(base));
    return ref_mod.NIL;
}

/// `[p address]` — raw address as a SmallInt (debugging / arithmetic).
fn methodFfiAddress(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const base = ffiBase(s) orelse return ref_mod.NIL;
    return ref_mod.mkSmallInt(@intCast(base)) orelse ref_mod.NIL;
}

/// `[p isNull]` — true iff the address is 0.
fn methodFfiIsNull(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    _ = args;
    _ = n_args;
    const base = ffiBase(s) orelse return ref_mod.NIL;
    return ref_mod.mkEid(if (base == 0) kernel_eids.TRUE_EID else kernel_eids.FALSE_EID);
}

fn methodPeekInt(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    if (n_args < 1) return ref_mod.NIL;
    const base = ffiBase(s) orelse return ref_mod.NIL;
    const off = argOffset(args, 0) orelse return ref_mod.NIL;
    const p: *align(1) const i64 = @ptrFromInt(base + off);
    return ref_mod.mkSmallInt(p.*) orelse ref_mod.NIL;
}

fn methodPeekByte(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    if (n_args < 1) return ref_mod.NIL;
    const base = ffiBase(s) orelse return ref_mod.NIL;
    const off = argOffset(args, 0) orelse return ref_mod.NIL;
    const p: *const u8 = @ptrFromInt(base + off);
    return ref_mod.mkSmallInt(p.*) orelse ref_mod.NIL;
}

fn methodPeekDouble(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    if (n_args < 1) return ref_mod.NIL;
    const base = ffiBase(s) orelse return ref_mod.NIL;
    const off = argOffset(args, 0) orelse return ref_mod.NIL;
    const alloc = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const p: *align(1) const f64 = @ptrFromInt(base + off);
    return boxed.mkFloat(alloc, p.*) catch ref_mod.NIL;
}

fn methodPeekPtr(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    if (n_args < 1) return ref_mod.NIL;
    const base = ffiBase(s) orelse return ref_mod.NIL;
    const off = argOffset(args, 0) orelse return ref_mod.NIL;
    const alloc = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const p: *align(1) const usize = @ptrFromInt(base + off);
    return boxed.mkFfiPtr(alloc, p.*) catch ref_mod.NIL;
}

fn methodPeekString(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    if (n_args < 1) return ref_mod.NIL;
    const base = ffiBase(s) orelse return ref_mod.NIL;
    const off = argOffset(args, 0) orelse return ref_mod.NIL;
    const alloc = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const cptr: [*:0]const u8 = @ptrFromInt(base + off);
    return strings.mkString(alloc, std.mem.span(cptr)) catch ref_mod.NIL;
}

/// `[p peekStringArray: count]` (Phase 19.3) — read a C `char**` at the receiver
/// into a birnam vector of strings. A SmallInt `count` reads exactly N slots (a
/// NULL slot becomes `nil`); `nil` reads until the NULL terminator (argv/environ
/// style). Slots are read with align(1) since the array may be unaligned. The
/// receiver is unchanged; the strings are copied, so the C buffer can be freed
/// after. Returns nil on a bad receiver/arg.
fn methodPeekStringArray(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    if (n_args < 1) return ref_mod.NIL;
    const base = ffiBase(s) orelse return ref_mod.NIL;
    const alloc = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const slots: [*]align(1) const usize = @ptrFromInt(base);

    var list = std.ArrayList(Ref){};
    defer list.deinit(alloc);

    const cnt_ref = args[0];
    if (ref_mod.isNil(cnt_ref)) {
        var i: usize = 0;
        while (true) : (i += 1) {
            const p = slots[i];
            if (p == 0) break;
            const cptr: [*:0]const u8 = @ptrFromInt(p);
            const str = strings.mkString(alloc, std.mem.span(cptr)) catch return ref_mod.NIL;
            list.append(alloc, str) catch return ref_mod.NIL;
        }
    } else {
        if (!ref_mod.isSmallInt(cnt_ref)) return ref_mod.NIL;
        const cnt = ref_mod.untagSmallInt(cnt_ref);
        if (cnt < 0) return ref_mod.NIL;
        var i: usize = 0;
        while (i < @as(usize, @intCast(cnt))) : (i += 1) {
            const p = slots[i];
            if (p == 0) {
                list.append(alloc, ref_mod.NIL) catch return ref_mod.NIL;
                continue;
            }
            const cptr: [*:0]const u8 = @ptrFromInt(p);
            const str = strings.mkString(alloc, std.mem.span(cptr)) catch return ref_mod.NIL;
            list.append(alloc, str) catch return ref_mod.NIL;
        }
    }
    return boxed.mkRrb(alloc, list.items) catch ref_mod.NIL;
}

/// `[p peekBytes: offset len: n]` (Phase 19.4) — copy `n` bytes at `base +
/// offset` into a fresh birnam string (strings are byte sequences). `offset`/`n`
/// are caller-trusted (a raw FfiPtr carries no bounds). Returns nil on a bad
/// receiver / non-SmallInt / negative len.
fn methodPeekBytes(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    if (n_args < 2) return ref_mod.NIL;
    const base = ffiBase(s) orelse return ref_mod.NIL;
    const off = argOffset(args, 0) orelse return ref_mod.NIL;
    if (!ref_mod.isSmallInt(args[1])) return ref_mod.NIL;
    const len_i = ref_mod.untagSmallInt(args[1]);
    if (len_i < 0) return ref_mod.NIL;
    const alloc = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const src: [*]const u8 = @ptrFromInt(base + off);
    return strings.mkString(alloc, src[0..@intCast(len_i)]) catch ref_mod.NIL;
}

/// `[p pokeBytes: bytes at: offset]` (Phase 19.4) — memcpy the bytes of a birnam
/// string (or a vector of 0..255 SmallInts) to `base + offset`. Returns self
/// (chainable), nil on a bad receiver / source type / non-SmallInt element.
fn methodPokeBytes(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    if (n_args < 2) return ref_mod.NIL;
    const base = ffiBase(s) orelse return ref_mod.NIL;
    const off = argOffset(args, 1) orelse return ref_mod.NIL;
    const dst: [*]u8 = @ptrFromInt(base + off);
    const src = args[0];
    if (strings.isString(src)) {
        const sb = strings.bytes(src);
        @memcpy(dst[0..sb.len], sb);
        return s.receiver;
    }
    if (boxed.isRrbRef(src)) {
        const n = boxed.rrbLen(src);
        var i: usize = 0;
        while (i < n) : (i += 1) {
            const e = boxed.rrbNth(src, i);
            if (!ref_mod.isSmallInt(e)) return ref_mod.NIL;
            dst[i] = @truncate(@as(u64, @bitCast(ref_mod.untagSmallInt(e))));
        }
        return s.receiver;
    }
    return ref_mod.NIL;
}

fn methodPokeInt(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    if (n_args < 2 or !ref_mod.isSmallInt(args[0])) return ref_mod.NIL;
    const base = ffiBase(s) orelse return ref_mod.NIL;
    const off = argOffset(args, 1) orelse return ref_mod.NIL;
    const p: *align(1) i64 = @ptrFromInt(base + off);
    p.* = ref_mod.untagSmallInt(args[0]);
    return s.receiver;
}

fn methodPokeByte(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    if (n_args < 2 or !ref_mod.isSmallInt(args[0])) return ref_mod.NIL;
    const base = ffiBase(s) orelse return ref_mod.NIL;
    const off = argOffset(args, 1) orelse return ref_mod.NIL;
    const p: *u8 = @ptrFromInt(base + off);
    p.* = @truncate(@as(u64, @bitCast(ref_mod.untagSmallInt(args[0]))));
    return s.receiver;
}

fn methodPokeDouble(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    if (n_args < 2 or !boxed.isFloat(args[0])) return ref_mod.NIL;
    const base = ffiBase(s) orelse return ref_mod.NIL;
    const off = argOffset(args, 1) orelse return ref_mod.NIL;
    const p: *align(1) f64 = @ptrFromInt(base + off);
    p.* = boxed.untagFloat(args[0]);
    return s.receiver;
}

fn methodPokePtr(s: *Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    if (n_args < 2) return ref_mod.NIL;
    const base = ffiBase(s) orelse return ref_mod.NIL;
    const off = argOffset(args, 1) orelse return ref_mod.NIL;
    // The value may be an FfiPtr or nil (→ NULL).
    const val: usize = if (ref_mod.isNil(args[0]))
        0
    else if (boxed.isFfiPtr(args[0]))
        boxed.untagFfiPtr(args[0])
    else
        return ref_mod.NIL;
    const p: *align(1) usize = @ptrFromInt(base + off);
    p.* = val;
    return s.receiver;
}

fn installMethod(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
    vt: u64,
    sel: u64,
    method_fn: *const anyopaque,
) !void {
    const c = try allocClosure(allocator, method_fn);
    const closure_eid = table.alloc();
    try table.put(closure_eid, ref_mod.mkPointer(@ptrCast(c)));
    _ = try store.assert(vt, sel, ref_mod.mkEid(closure_eid), kernel_eids.M_BOOTSTRAP);
}

/// Called from bootstrap after dispatch is wired. Installs `wrap:as:sig:`,
/// `path`, `handle` on `MODULE_VT`.
pub fn installMethods(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
) !void {
    try installMethod(allocator, store, table, kernel_eids.MODULE_VT, kernel_eids.SEL_PATH, @ptrCast(&methodPath));
    try installMethod(allocator, store, table, kernel_eids.MODULE_VT, kernel_eids.SEL_HANDLE, @ptrCast(&methodHandle));
    try installMethod(allocator, store, table, kernel_eids.MODULE_VT, kernel_eids.SEL_WRAP_AS_SIG, @ptrCast(&methodWrapAsSig));
    try installMethod(allocator, store, table, kernel_eids.MODULE_VT, kernel_eids.SEL_SYMBOLS, @ptrCast(&methodSymbols));
    try installMethod(allocator, store, table, kernel_eids.MODULE_VT, kernel_eids.SEL_UNLOAD, @ptrCast(&methodUnload));

    // -- Prototype side: the `Module` object + its vt with `load:`/`resolve`. -
    // MODULE_PROTO_VT is a normal vtable (its own :vtable is VTABLE_VT,
    // delegates to OBJECT_VT — NOT MODULE_VT, so the prototype doesn't inherit
    // per-instance methods). MODULE_PROTO is the object the lang frontend binds
    // the global name `Module` to. Sending `load:` mints a new module object;
    // `resolve` opens + binds all pending modules.
    const proto_vt_obj = try object.allocObject(allocator, ref_mod.mkEid(kernel_eids.VTABLE_VT), 0);
    try table.put(kernel_eids.MODULE_PROTO_VT, object.toRef(proto_vt_obj));
    _ = try store.assert(kernel_eids.MODULE_PROTO_VT, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.VTABLE_VT), kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(kernel_eids.MODULE_PROTO_VT, kernel_eids.ATTR_DELEGATE, ref_mod.mkEid(kernel_eids.OBJECT_VT), kernel_eids.M_BOOTSTRAP);
    try installMethod(allocator, store, table, kernel_eids.MODULE_PROTO_VT, kernel_eids.SEL_LOAD, @ptrCast(&methodLoad));
    try installMethod(allocator, store, table, kernel_eids.MODULE_PROTO_VT, kernel_eids.SEL_RESOLVE, @ptrCast(&methodResolve));
    try installMethod(allocator, store, table, kernel_eids.MODULE_PROTO_VT, kernel_eids.SEL_FFI_ALLOC, @ptrCast(&methodAlloc));
    try installMethod(allocator, store, table, kernel_eids.MODULE_PROTO_VT, kernel_eids.SEL_FFI_ARGV, @ptrCast(&methodArgv));
    try installMethod(allocator, store, table, kernel_eids.MODULE_PROTO_VT, kernel_eids.SEL_FFI_OUT_INT, @ptrCast(&methodOutInt));
    try installMethod(allocator, store, table, kernel_eids.MODULE_PROTO_VT, kernel_eids.SEL_FFI_OUT_LONG, @ptrCast(&methodOutLong));
    try installMethod(allocator, store, table, kernel_eids.MODULE_PROTO_VT, kernel_eids.SEL_FFI_OUT_PTR, @ptrCast(&methodOutPtr));
    try installMethod(allocator, store, table, kernel_eids.MODULE_PROTO_VT, kernel_eids.SEL_FFI_OUT_DOUBLE, @ptrCast(&methodOutDouble));

    // -- Phase 18 prelude: FFI memory primitives on FFI_PTR_VT --------------
    try installMethod(allocator, store, table, kernel_eids.FFI_PTR_VT, kernel_eids.SEL_FFI_FREE, @ptrCast(&methodFfiFree));
    try installMethod(allocator, store, table, kernel_eids.FFI_PTR_VT, kernel_eids.SEL_FFI_ADDRESS, @ptrCast(&methodFfiAddress));
    try installMethod(allocator, store, table, kernel_eids.FFI_PTR_VT, kernel_eids.SEL_FFI_IS_NULL, @ptrCast(&methodFfiIsNull));
    try installMethod(allocator, store, table, kernel_eids.FFI_PTR_VT, kernel_eids.SEL_FFI_PEEK_INT, @ptrCast(&methodPeekInt));
    try installMethod(allocator, store, table, kernel_eids.FFI_PTR_VT, kernel_eids.SEL_FFI_PEEK_BYTE, @ptrCast(&methodPeekByte));
    try installMethod(allocator, store, table, kernel_eids.FFI_PTR_VT, kernel_eids.SEL_FFI_PEEK_DOUBLE, @ptrCast(&methodPeekDouble));
    try installMethod(allocator, store, table, kernel_eids.FFI_PTR_VT, kernel_eids.SEL_FFI_PEEK_PTR, @ptrCast(&methodPeekPtr));
    try installMethod(allocator, store, table, kernel_eids.FFI_PTR_VT, kernel_eids.SEL_FFI_PEEK_STRING, @ptrCast(&methodPeekString));
    try installMethod(allocator, store, table, kernel_eids.FFI_PTR_VT, kernel_eids.SEL_FFI_PEEK_STRING_ARRAY, @ptrCast(&methodPeekStringArray));
    try installMethod(allocator, store, table, kernel_eids.FFI_PTR_VT, kernel_eids.SEL_FFI_PEEK_BYTES, @ptrCast(&methodPeekBytes));
    try installMethod(allocator, store, table, kernel_eids.FFI_PTR_VT, kernel_eids.SEL_FFI_POKE_BYTES, @ptrCast(&methodPokeBytes));
    try installMethod(allocator, store, table, kernel_eids.FFI_PTR_VT, kernel_eids.SEL_VALUE, @ptrCast(&methodCellValue));
    try installMethod(allocator, store, table, kernel_eids.FFI_PTR_VT, kernel_eids.SEL_FFI_POKE_INT, @ptrCast(&methodPokeInt));
    try installMethod(allocator, store, table, kernel_eids.FFI_PTR_VT, kernel_eids.SEL_FFI_POKE_BYTE, @ptrCast(&methodPokeByte));
    try installMethod(allocator, store, table, kernel_eids.FFI_PTR_VT, kernel_eids.SEL_FFI_POKE_DOUBLE, @ptrCast(&methodPokeDouble));
    try installMethod(allocator, store, table, kernel_eids.FFI_PTR_VT, kernel_eids.SEL_FFI_POKE_PTR, @ptrCast(&methodPokePtr));

    // The prototype object — only needs a :vtable fact (dispatch reads it).
    _ = try store.assert(kernel_eids.MODULE_PROTO, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.MODULE_PROTO_VT), kernel_eids.M_BOOTSTRAP);
}
