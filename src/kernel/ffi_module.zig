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

/// Top-level `Module load:` — opens `path` via `dlopen`, mints a Module
/// entity, returns its EID Ref. On failure, returns a diagnostic
/// entity Ref tagged with `ERROR_VT`.
pub fn moduleLoad(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
    interner: *interner_mod.Interner,
    path: []const u8,
) !Ref {
    // dlopen via std.DynLib. On failure, return error-entity.
    var lib = std.DynLib.open(path) catch {
        return try errorEntity(
            allocator,
            store,
            table,
            interner,
            "ffi-load-failure",
            "dlopen failed",
            &.{ .{ .attr = kernel_eids.ATTR_ERROR_PATH, .value = try strings.mkString(allocator, path) } },
        );
    };
    // Stash the handle pointer. std.DynLib.open returns a value with a
    // platform-specific inner; we treat its address as the foreign handle.
    // For macOS, this wraps dlopen's void* result.
    const handle_addr: usize = blk: {
        // std.DynLib has a `handle` field on darwin (DlDynLib uses void*).
        // Pull it out via the public `inner` if available; fall back to
        // a stable pointer cast.
        const lib_ptr = &lib;
        const raw: usize = @intFromPtr(lib_ptr);
        _ = raw;
        // Use a heap-allocated copy of the DynLib struct so the handle
        // survives this function frame.
        const copy = allocator.create(std.DynLib) catch unreachable;
        copy.* = lib;
        break :blk @intFromPtr(copy);
    };

    const mod_eid = table.alloc();
    const instance_vt = try mintInstanceVt(allocator, store, table);
    _ = try store.assert(mod_eid, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(instance_vt), kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(
        mod_eid,
        kernel_eids.ATTR_FFI_HANDLE,
        try boxed.mkFfiPtr(allocator, handle_addr),
        kernel_eids.M_BOOTSTRAP,
    );
    _ = try store.assert(
        mod_eid,
        kernel_eids.ATTR_FFI_PATH,
        try strings.mkString(allocator, path),
        kernel_eids.M_BOOTSTRAP,
    );
    return ref_mod.mkEid(mod_eid);
}

/// Look up a symbol via the DynLib stored on the Module. Returns null
/// if the symbol isn't found.
pub fn moduleSymbolAddr(
    store: *const fact_store_mod.FactStore,
    mod_eid: u64,
    name: [:0]const u8,
) ?usize {
    const handle_ref = store.currentValue(mod_eid, kernel_eids.ATTR_FFI_HANDLE) orelse return null;
    if (!boxed.isFfiPtr(handle_ref)) return null;
    const lib_ptr_addr = boxed.untagFfiPtr(handle_ref);
    const lib: *std.DynLib = @ptrFromInt(lib_ptr_addr);
    const sym = lib.lookup(*anyopaque, name) orelse return null;
    return @intFromPtr(sym);
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
fn methodSymbols(s: *Send) callconv(.c) Ref {
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

    var reader = try macho.Reader.init(bytes_buf);
    var it = try reader.symbols();
    while (it.next()) |sym| {
        // Skip empty / debug entries.
        if (sym.name.len == 0) continue;
        if (sym.kind == .stab) continue;

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
            .stab => 4,
            .other => 5,
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
fn methodPath(s: *Send) callconv(.c) Ref {
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    if (!ref_mod.isEid(s.receiver)) return ref_mod.NIL;
    return store.currentValue(ref_mod.untagEid(s.receiver), kernel_eids.ATTR_FFI_PATH) orelse ref_mod.NIL;
}

/// `[mod handle]` — return the boxed FfiPtr Ref stored on the Module.
fn methodHandle(s: *Send) callconv(.c) Ref {
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    if (!ref_mod.isEid(s.receiver)) return ref_mod.NIL;
    return store.currentValue(ref_mod.untagEid(s.receiver), kernel_eids.ATTR_FFI_HANDLE) orelse ref_mod.NIL;
}

/// `[mod wrap: <sel-Ref> as: <name-string-Ref> signature: <sig-string-Ref>]`
/// Looks up the symbol via dlsym, calls `ffi.registerWrap`, installs the
/// returned closure on the receiver Module's instance vt at `sel`. Returns
/// `self` on success, an `ERROR_VT` entity on failure.
fn methodWrapAsSig(s: *Send, sel_ref: Ref, name_ref: Ref, sig_ref: Ref) callconv(.c) Ref {
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    const table = send_mod.tablePtr() orelse return ref_mod.NIL;
    const alloc = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const interner = id_api_mod.internerPtr() orelse return ref_mod.NIL;

    if (!ref_mod.isEid(s.receiver)) return ref_mod.NIL;
    if (!ref_mod.isEid(sel_ref)) return ref_mod.NIL;
    const mod_eid = ref_mod.untagEid(s.receiver);

    // Decode name (boxed string) and sig (boxed string) into slices.
    if (!strings.isString(name_ref)) {
        return errorEntity(alloc, store, table, interner, "ffi-sig-parse-failure", "wrap: name not a string", &.{}) catch ref_mod.NIL;
    }
    if (!strings.isString(sig_ref)) {
        return errorEntity(alloc, store, table, interner, "ffi-sig-parse-failure", "wrap: sig not a string", &.{}) catch ref_mod.NIL;
    }
    const name_slice = strings.bytes(name_ref);
    const sig_slice = strings.bytes(sig_ref);

    // Build a null-terminated copy of the name for DynLib.lookup.
    const name_z = alloc.dupeZ(u8, name_slice) catch return ref_mod.NIL;
    defer alloc.free(name_z);
    const addr = moduleSymbolAddr(store, mod_eid, name_z) orelse {
        return errorEntity(
            alloc,
            store,
            table,
            interner,
            "ffi-symbol-not-found",
            "dlsym returned null",
            &.{.{ .attr = kernel_eids.ATTR_ERROR_NAME, .value = strings.mkString(alloc, name_slice) catch return ref_mod.NIL }},
        ) catch ref_mod.NIL;
    };

    const closure_eid = ffi.registerWrap(alloc, store, table, addr, sig_slice) catch |err| {
        const kind: []const u8 = switch (err) {
            error.ArityTooHigh => "ffi-arity-too-high",
            error.SigParseFailure => "ffi-sig-parse-failure",
            else => "ffi-wrap-failure",
        };
        return errorEntity(alloc, store, table, interner, kind, "registerWrap failed", &.{}) catch ref_mod.NIL;
    };

    // Install the closure on the receiver's instance vt at sel.
    const instance_vt_ref = store.currentValue(mod_eid, kernel_eids.ATTR_VTABLE) orelse return ref_mod.NIL;
    if (!ref_mod.isEid(instance_vt_ref)) return ref_mod.NIL;
    const instance_vt = ref_mod.untagEid(instance_vt_ref);
    _ = store.assert(instance_vt, ref_mod.untagEid(sel_ref), ref_mod.mkEid(closure_eid), kernel_eids.M_BOOTSTRAP) catch return ref_mod.NIL;

    return s.receiver; // chainable
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
}
