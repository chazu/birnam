//! Phase 9: dyncall-backed foreign function dispatch.
//!
//! Two layers:
//!   1. Thin Zig extern decls for the dyncall C API.
//!   2. Sig-string parser + invoke(addr, sig, args) Ref marshaler.
//!
//! Sig strings follow dyncall convention (DCsigchar):
//!   B = bool, c = char, s = short, i = int, l = long, L = long long,
//!   f = float, d = double, p = pointer, Z = c-string (alias of p),
//!   v = void. `)` separates args from return type. `e` at the call site
//!   marks the start of variadic args.
//!
//! Phase 9 scope: pass-by-value scalars + pointer + c-string. Aggregates
//! (structs by value) deferred to 9.5+.

const std = @import("std");
const ref_mod = @import("ref.zig");
const boxed = @import("boxed.zig");

pub const Ref = ref_mod.Ref;

// -- dyncall C API extern decls --------------------------------------------

pub const DCCallVM = opaque {};

pub const DC_CALL_C_DEFAULT: c_int = 0;
pub const DC_CALL_C_ELLIPSIS: c_int = 100;
pub const DC_CALL_C_ELLIPSIS_VARARGS: c_int = 101;

pub extern "c" fn dcNewCallVM(size: usize) ?*DCCallVM;
pub extern "c" fn dcFree(vm: *DCCallVM) void;
pub extern "c" fn dcReset(vm: *DCCallVM) void;
pub extern "c" fn dcMode(vm: *DCCallVM, mode: c_int) void;

pub extern "c" fn dcArgBool(vm: *DCCallVM, v: c_int) void;
pub extern "c" fn dcArgChar(vm: *DCCallVM, v: u8) void;
pub extern "c" fn dcArgShort(vm: *DCCallVM, v: c_short) void;
pub extern "c" fn dcArgInt(vm: *DCCallVM, v: c_int) void;
pub extern "c" fn dcArgLong(vm: *DCCallVM, v: c_long) void;
pub extern "c" fn dcArgLongLong(vm: *DCCallVM, v: c_longlong) void;
pub extern "c" fn dcArgFloat(vm: *DCCallVM, v: f32) void;
pub extern "c" fn dcArgDouble(vm: *DCCallVM, v: f64) void;
pub extern "c" fn dcArgPointer(vm: *DCCallVM, v: ?*const anyopaque) void;

pub extern "c" fn dcCallVoid(vm: *DCCallVM, fnp: *const anyopaque) void;
pub extern "c" fn dcCallBool(vm: *DCCallVM, fnp: *const anyopaque) c_int;
pub extern "c" fn dcCallChar(vm: *DCCallVM, fnp: *const anyopaque) u8;
pub extern "c" fn dcCallShort(vm: *DCCallVM, fnp: *const anyopaque) c_short;
pub extern "c" fn dcCallInt(vm: *DCCallVM, fnp: *const anyopaque) c_int;
pub extern "c" fn dcCallLong(vm: *DCCallVM, fnp: *const anyopaque) c_long;
pub extern "c" fn dcCallLongLong(vm: *DCCallVM, fnp: *const anyopaque) c_longlong;
pub extern "c" fn dcCallFloat(vm: *DCCallVM, fnp: *const anyopaque) f32;
pub extern "c" fn dcCallDouble(vm: *DCCallVM, fnp: *const anyopaque) f64;
pub extern "c" fn dcCallPointer(vm: *DCCallVM, fnp: *const anyopaque) ?*anyopaque;

// -- Sig-string parser ------------------------------------------------------

pub const Kind = enum {
    void_,
    bool_,
    char,
    short,
    int,
    long,
    long_long,
    float,
    double,
    pointer,
    cstring,
};

pub const Sig = struct {
    args: []Kind,
    ret: Kind,
    /// Position in `args` (0-indexed) where variadic varargs begin. null
    /// means the signature is not variadic. Marker char in source: `e`.
    variadic_at: ?usize = null,

    pub fn deinit(self: *Sig, allocator: std.mem.Allocator) void {
        allocator.free(self.args);
    }
};

pub const SigParseError = error{
    UnknownSigchar,
    MissingReturnSeparator,
    EmptySig,
    OutOfMemory,
};

fn kindOf(c: u8) ?Kind {
    return switch (c) {
        'v' => .void_,
        'B' => .bool_,
        'c' => .char,
        's' => .short,
        'i' => .int,
        'l' => .long,
        'L' => .long_long,
        'f' => .float,
        'd' => .double,
        'p' => .pointer,
        'Z' => .cstring,
        else => null,
    };
}

pub fn parseSig(allocator: std.mem.Allocator, s: []const u8) SigParseError!Sig {
    if (s.len == 0) return error.EmptySig;
    var args = std.ArrayList(Kind){};
    defer args.deinit(allocator);
    var variadic_at: ?usize = null;
    var i: usize = 0;
    while (i < s.len) : (i += 1) {
        const c = s[i];
        if (c == ')') {
            if (i + 1 >= s.len) return error.MissingReturnSeparator;
            const ret_kind = kindOf(s[i + 1]) orelse return error.UnknownSigchar;
            const owned = try args.toOwnedSlice(allocator);
            return .{ .args = owned, .ret = ret_kind, .variadic_at = variadic_at };
        }
        if (c == 'e') {
            variadic_at = args.items.len;
            continue;
        }
        const k = kindOf(c) orelse return error.UnknownSigchar;
        try args.append(allocator, k);
    }
    return error.MissingReturnSeparator;
}

// -- VM lifecycle ----------------------------------------------------------

/// Default VM stack size — generous enough for the Phase 9 test plan.
pub const DEFAULT_VM_SIZE: usize = 4096;

/// Single kernel-global VM pointer. dyncall is single-threaded; single VM
/// is enough until concurrency lands.
var g_vm: ?*DCCallVM = null;

pub fn ensureVm() ?*DCCallVM {
    if (g_vm) |v| return v;
    g_vm = dcNewCallVM(DEFAULT_VM_SIZE);
    return g_vm;
}

pub fn shutdown() void {
    if (g_vm) |v| {
        dcFree(v);
        g_vm = null;
    }
}

// -- Ref → dyncall arg marshaling -----------------------------------------

/// Push `value` into the VM as `kind`. Returns false on type mismatch OR
/// on a stale FFI pointer (Phase 9.5.5: pointer Refs whose owning Module
/// has been unloaded carry `[box_eid :ffi-stale? ?]` facts). Caller
/// distinguishes the two via `out_stale_box_eid` (0 = type mismatch,
/// non-zero = stale box).
fn pushArgChecked(
    store: *fact_store_mod.FactStore,
    vm: *DCCallVM,
    kind: Kind,
    value: Ref,
    out_stale_box_eid: *u64,
) bool {
    out_stale_box_eid.* = 0;
    if (kind == .pointer or kind == .cstring) {
        if (boxed.isFfiPtr(value)) {
            const box_eid = boxed.ffiPtrBoxEid(value);
            if (box_eid != 0) {
                if (store.currentValue(box_eid, kernel_eids.ATTR_FFI_STALE)) |_| {
                    out_stale_box_eid.* = box_eid;
                    return false;
                }
            }
        }
    }
    return pushArg(vm, kind, value);
}

/// Push `value` into the VM as `kind`. Returns false on type mismatch so the
/// caller can abort + return NIL.
fn pushArg(vm: *DCCallVM, kind: Kind, value: Ref) bool {
    switch (kind) {
        .void_ => return false,
        .bool_ => {
            if (!ref_mod.isSmallInt(value)) return false;
            dcArgBool(vm, @intCast(@as(c_int, @intCast(ref_mod.untagSmallInt(value)))));
        },
        .char => {
            if (!ref_mod.isSmallInt(value)) return false;
            dcArgChar(vm, @intCast(@as(u8, @truncate(@as(u64, @bitCast(ref_mod.untagSmallInt(value)))))));
        },
        .short => {
            if (!ref_mod.isSmallInt(value)) return false;
            dcArgShort(vm, @intCast(ref_mod.untagSmallInt(value)));
        },
        .int => {
            if (!ref_mod.isSmallInt(value)) return false;
            dcArgInt(vm, @intCast(ref_mod.untagSmallInt(value)));
        },
        .long => {
            if (!ref_mod.isSmallInt(value)) return false;
            dcArgLong(vm, @intCast(ref_mod.untagSmallInt(value)));
        },
        .long_long => {
            if (!ref_mod.isSmallInt(value)) return false;
            dcArgLongLong(vm, @intCast(ref_mod.untagSmallInt(value)));
        },
        .float => {
            if (!boxed.isFloat(value)) return false;
            dcArgFloat(vm, @floatCast(boxed.untagFloat(value)));
        },
        .double => {
            if (!boxed.isFloat(value)) return false;
            dcArgDouble(vm, boxed.untagFloat(value));
        },
        .pointer, .cstring => {
            if (!boxed.isFfiPtr(value)) return false;
            const addr = boxed.untagFfiPtr(value);
            dcArgPointer(vm, @ptrFromInt(addr));
        },
    }
    return true;
}

/// Dispatch the appropriate dcCallX based on return kind. Returns the raw
/// C-side value boxed/tagged into a birnam Ref.
///
/// `owner_mod_eid` (Phase 9.5.5): when non-zero, returned pointer values
/// (`.pointer` / `.cstring`) are boxed via `mkFfiPtrOwned`, asserting
/// `[box :ffi-owner mod]` so Module unload can find + stale them. When
/// zero, fall back to unowned `mkFfiPtr` (Phase 9 behavior).
fn callAndWrap(
    allocator: std.mem.Allocator,
    store: ?*fact_store_mod.FactStore,
    table: ?*object_table_mod.ObjectTable,
    vm: *DCCallVM,
    ret: Kind,
    addr: usize,
    owner_mod_eid: u64,
) Ref {
    const fnp: *const anyopaque = @ptrFromInt(addr);
    switch (ret) {
        .void_ => {
            dcCallVoid(vm, fnp);
            return ref_mod.NIL;
        },
        .bool_ => {
            const r = dcCallBool(vm, fnp);
            return ref_mod.mkSmallInt(@intCast(r)) orelse ref_mod.NIL;
        },
        .char => {
            const r = dcCallChar(vm, fnp);
            return ref_mod.mkSmallInt(@intCast(r)) orelse ref_mod.NIL;
        },
        .short => {
            const r = dcCallShort(vm, fnp);
            return ref_mod.mkSmallInt(@intCast(r)) orelse ref_mod.NIL;
        },
        .int => {
            const r = dcCallInt(vm, fnp);
            return ref_mod.mkSmallInt(@intCast(r)) orelse ref_mod.NIL;
        },
        .long => {
            const r = dcCallLong(vm, fnp);
            return ref_mod.mkSmallInt(@intCast(r)) orelse ref_mod.NIL;
        },
        .long_long => {
            const r = dcCallLongLong(vm, fnp);
            return ref_mod.mkSmallInt(@intCast(r)) orelse ref_mod.NIL;
        },
        .float => {
            const r = dcCallFloat(vm, fnp);
            return boxed.mkFloat(allocator, @floatCast(r)) catch ref_mod.NIL;
        },
        .double => {
            const r = dcCallDouble(vm, fnp);
            return boxed.mkFloat(allocator, r) catch ref_mod.NIL;
        },
        .pointer, .cstring => {
            const r = dcCallPointer(vm, fnp);
            const addr_ret: usize = @intFromPtr(r);
            if (owner_mod_eid != 0 and store != null and table != null) {
                return boxed.mkFfiPtrOwned(allocator, store.?, table.?, addr_ret, owner_mod_eid) catch ref_mod.NIL;
            }
            return boxed.mkFfiPtr(allocator, addr_ret) catch ref_mod.NIL;
        },
    }
}

/// Top-level invocation. Resolves the kernel-global VM, configures variadic
/// mode if the sig is variadic, marshals args, calls, wraps return.
///
/// Phase 9.5.5: `store`/`owner_mod_eid` are threaded through so per-arg
/// stale-pointer checks (`:ffi-stale?`) and per-return owned-pointer
/// boxing both work. Passing `store = null` or `owner_mod_eid = 0`
/// preserves Phase 9 ownerless behavior.
pub fn invoke(
    allocator: std.mem.Allocator,
    addr: usize,
    sig: Sig,
    args: []const Ref,
) Ref {
    return invokeWithOwner(allocator, null, null, addr, sig, args, 0);
}

pub fn invokeWithOwner(
    allocator: std.mem.Allocator,
    store: ?*fact_store_mod.FactStore,
    table: ?*object_table_mod.ObjectTable,
    addr: usize,
    sig: Sig,
    args: []const Ref,
    owner_mod_eid: u64,
) Ref {
    if (args.len != sig.args.len) return ref_mod.NIL;
    const vm = ensureVm() orelse return ref_mod.NIL;
    dcReset(vm);
    const is_variadic = sig.variadic_at != null;
    if (is_variadic) dcMode(vm, DC_CALL_C_ELLIPSIS);
    for (sig.args, args, 0..) |kind, value, i| {
        if (is_variadic and i == sig.variadic_at.?) {
            dcMode(vm, DC_CALL_C_ELLIPSIS_VARARGS);
        }
        if (store) |s| {
            var stale_box: u64 = 0;
            if (!pushArgChecked(s, vm, kind, value, &stale_box)) return ref_mod.NIL;
        } else {
            if (!pushArg(vm, kind, value)) return ref_mod.NIL;
        }
    }
    return callAndWrap(allocator, store, table, vm, sig.ret, addr, owner_mod_eid);
}

// -- Wrap registry + per-arity dispatch shims -------------------------------

const send_mod = @import("send.zig");
const fact_store_mod = @import("fact_store.zig");
const object_table_mod = @import("object_table.zig");
const closure_mod = @import("closure.zig");
const kernel_eids = @import("kernel_eids.zig");

/// Resolve a wrap_eid (carried in `s.closure.data`) back to its addr +
/// parsed Sig. Returns null on any fact-store lookup miss.
fn resolveWrap(
    store: *fact_store_mod.FactStore,
    wrap_eid: u64,
) ?struct { addr: usize, sig: *const Sig } {
    const addr_ref = store.currentValue(wrap_eid, kernel_eids.ATTR_FFI_ADDR) orelse return null;
    if (!boxed.isFfiPtr(addr_ref)) return null;
    const addr = boxed.untagFfiPtr(addr_ref);
    const sig_ref = store.currentValue(wrap_eid, kernel_eids.ATTR_FFI_SIG) orelse return null;
    if (!ref_mod.isPointer(sig_ref)) return null;
    const sig: *const Sig = @ptrCast(@alignCast(ref_mod.untagPointer(sig_ref)));
    return .{ .addr = addr, .sig = sig };
}

/// Phase 15.E — free a wrap's heap `Sig` holder (struct + its `args` slice).
/// Safe targeted free, not arena-sweep: a Sig holds an internal slice that is
/// not Ref-reachable (so a sweep couldn't keep it), and it is referenced ONLY
/// via `[wrap :ffi-sig ptr]`, never captured. Callers invoke this exactly when
/// the wrap's Sig is being superseded (re-resolve) or torn down (module unload)
/// — both quiescent points (unload refuses while `ffi_depth > 0`). No-op when
/// the wrap has no current `:ffi-sig`. Uses the same allocator that created it.
pub fn freeSig(
    store: *fact_store_mod.FactStore,
    allocator: std.mem.Allocator,
    wrap_eid: u64,
) void {
    const sig_ref = store.currentValue(wrap_eid, kernel_eids.ATTR_FFI_SIG) orelse return;
    if (!ref_mod.isPointer(sig_ref)) return;
    const sig: *Sig = @ptrCast(@alignCast(ref_mod.untagPointer(sig_ref)));
    sig.deinit(allocator);
    allocator.destroy(sig);
    // Retract the now-dangling fact so `resolveWrap` returns null (the shim has
    // no stale-guard and would otherwise read freed memory on a post-unload
    // call). On re-resolve the caller asserts a fresh `:ffi-sig` right after.
    _ = store.retract(wrap_eid, kernel_eids.ATTR_FFI_SIG, sig_ref, kernel_eids.M_BOOTSTRAP) catch {};
}

fn shimDispatch(s: *send_mod.Send, args: []const Ref) Ref {
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    const table = send_mod.tablePtr() orelse return ref_mod.NIL;
    const alloc = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const c = s.closure orelse return ref_mod.NIL;
    if (!ref_mod.isEid(c.data)) return ref_mod.NIL;
    const wrap_eid = ref_mod.untagEid(c.data);
    const w = resolveWrap(store, wrap_eid) orelse return ref_mod.NIL;
    // Phase 9.5.5: thread the wrap's owning Module through so return
    // pointers can be tagged with `:ffi-owner` and arg pointers can be
    // checked for `:ffi-stale?`. Owner = 0 keeps Phase 9 behavior.
    var owner_mod: u64 = 0;
    if (store.currentValue(wrap_eid, kernel_eids.ATTR_FFI_OWNER)) |owner_ref| {
        if (ref_mod.isEid(owner_ref)) owner_mod = ref_mod.untagEid(owner_ref);
    }
    return invokeWithOwner(alloc, store, table, w.addr, w.sig.*, args, owner_mod);
}

/// All FFI shims now share the variadic Imp signature; arity comes from
/// `n_args` and args are read from the buffer. The wrap's expected arity
/// is in its `:ffi-sig` slot, so distinct per-arity shims aren't needed.
fn shimAny(s: *send_mod.Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    return shimDispatch(s, args[0..n_args]);
}

pub const RegisterError = error{
    OutOfMemory,
    ArityTooHigh,
    SigParseFailure,
};

/// Resolve a single declarative wrap: parse `sig_str`, assert the TRANSIENT
/// resolved facts (`:ffi-addr`, `:ffi-sig` pointer) onto the EXISTING
/// `wrap_eid`, and build the dispatch closure. The wrap entity's durable
/// facts (`:ffi-wrap-symbol` / `:ffi-sig-str` / `:wrap-selector` / `:ffi-owner`)
/// are asserted earlier by `wrap:as:signature:`; this only adds the
/// process-local resolution. Returns the closure EID for the caller to install
/// via `[vt sel mkEid(closure_eid)]`.
///
/// Phase 9 arity limit: 0..3 (matches send0..send3). Higher arity needs
/// either send-N expansion (future) or arg-vector dispatch.
pub fn buildWrapClosure(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
    wrap_eid: u64,
    addr: usize,
    sig_str: []const u8,
) anyerror!u64 {
    var sig = parseSig(allocator, sig_str) catch return error.SigParseFailure;
    if (sig.args.len > 3) {
        sig.deinit(allocator);
        return error.ArityTooHigh;
    }
    // Phase 15.E: re-resolving an already-wrapped eid (image reload, relink)
    // supersedes its `:ffi-sig`; free the prior holder first so it doesn't leak.
    freeSig(store, allocator, wrap_eid);

    // Stash the parsed Sig on the heap so the shim can reference it via
    // pointer Ref. Transient (process-local); not persisted.
    const sig_holder = try allocator.create(Sig);
    sig_holder.* = sig;

    const addr_ref = try boxed.mkFfiPtr(allocator, addr);
    _ = try store.assert(wrap_eid, kernel_eids.ATTR_FFI_ADDR, addr_ref, kernel_eids.M_BOOTSTRAP);
    _ = try store.assert(wrap_eid, kernel_eids.ATTR_FFI_SIG, ref_mod.mkPointer(@ptrCast(sig_holder)), kernel_eids.M_BOOTSTRAP);

    // All arities go through the variadic shim — Phase 10.2.5 unified Imp.
    const method_ptr: *const anyopaque = @ptrCast(&shimAny);

    // Allocate a Closure with .data = mkEid(wrap_eid). Match the kernel's
    // closure-as-libid-object layout: [vt_ref][method][data].
    const total = 3 * @sizeOf(Ref);
    const buf = try allocator.alignedAlloc(u8, .of(Ref), total);
    @memset(buf, 0);
    const vt_slot: *Ref = @ptrCast(@alignCast(buf.ptr));
    vt_slot.* = ref_mod.mkEid(kernel_eids.CLOSURE_VT);
    const c: *closure_mod.Closure = @ptrCast(@alignCast(buf.ptr + @sizeOf(Ref)));
    c.method = method_ptr;
    c.data = ref_mod.mkEid(wrap_eid);
    const closure_eid = table.alloc();
    try table.put(closure_eid, ref_mod.mkPointer(@ptrCast(c)));
    return closure_eid;
}

/// Legacy one-shot: mint a fresh wrap entity then resolve it. Retained for
/// direct callers/tests that bypass the declarative `wrap:as:signature:` path.
pub fn registerWrap(
    allocator: std.mem.Allocator,
    store: *fact_store_mod.FactStore,
    table: *object_table_mod.ObjectTable,
    addr: usize,
    sig_str: []const u8,
) anyerror!u64 {
    const wrap_eid = table.alloc();
    return buildWrapClosure(allocator, store, table, wrap_eid, addr, sig_str);
}
