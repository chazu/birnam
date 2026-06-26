//! Install JIT'd native code as a method — "native code is a cache of bytecode"
//! (docs/10a). Closes the mode-1 loop: a compiled method's object-table closure
//! is swapped from the bytecode interpreter to a trampoline that runs the libtcc
//! JIT'd body, so ordinary `send` dispatch hits native code. The `:bytecode-stream`
//! fact stays (canonical data); native is the derived, droppable, re-JIT-able
//! cache — the same demotion W1 made for the object table over facts.

const std = @import("std");
const ref_mod = @import("ref.zig");
const send_mod = @import("send.zig");
const object_table = @import("object_table.zig");
const fact_store = @import("fact_store.zig");
const closure_mod = @import("closure.zig");
const kernel_eids = @import("kernel_eids.zig");
const strings = @import("strings.zig");
const boxed = @import("boxed.zig");
const codegen = @import("codegen.zig");
const dispatch = @import("dispatch.zig");
const jit = @import("jit.zig");
const mcache = @import("mcache.zig");

const Ref = ref_mod.Ref;

/// libtcc's runtime archive dir (`tcc_add_library_path`). null: our generated C
/// only does integer ops + calls the runtime by symbol, so it needs NO libtcc1
/// helpers — verified — which is why birnam can statically link the vendored
/// `libtcc.a` with no runtime files at all (self-contained).
pub const RUNTIME_LIB_DIR: ?[*:0]const u8 = null;

/// Bind every runtime entry point a generated body may reference to its host
/// fn-ptr, so the JIT'd C resolves. One place; codegen's `RUNTIME_PRELUDE` and
/// this list must stay in sync.
fn bindRuntime(j: *jit.Jit) jit.Error!void {
    try j.addSymbol("birnam_send0", @ptrCast(&send_mod.birnamSend0));
    try j.addSymbol("birnam_send1", @ptrCast(&send_mod.birnamSend1));
    try j.addSymbol("birnam_send2", @ptrCast(&send_mod.birnamSend2));
    try j.addSymbol("birnam_sendN", @ptrCast(&send_mod.birnamSendN));
    try j.addSymbol("birnam_is_falsey", @ptrCast(&dispatch.birnamIsFalsey));
    try j.addSymbol("birnam_is_error", @ptrCast(&dispatch.birnamIsError));
    try j.addSymbol("birnam_make_block", @ptrCast(&dispatch.birnamMakeBlock));
    try j.addSymbol("birnam_invoke_block", @ptrCast(&dispatch.birnamInvokeBlock));
}

/// A method's bytecode lowered + JIT'd, but NOT yet installed: the owning `Jit`
/// (its memory backs `fn_ptr`) and the native body. The caller installs it
/// (`installTrampoline`) and keeps the `Jit` alive; `deinit` drops the native code.
pub const Compiled = struct { jit: jit.Jit, fn_ptr: MethodFn };

/// Read a method's `:bytecode-stream` + `:constant-pool`, lower to C
/// (`codegen.emitMethod`), and JIT it (libtcc) — without installing. Returns
/// `error.Declined` if the body uses an opcode the codegen doesn't lower yet
/// (a block's `^`), so the caller keeps the interpreter. The lifetime split
/// (compile here, install separately) lets `JitManager` own + re-JIT + drop Jits.
pub fn compileMethod(
    allocator: std.mem.Allocator,
    store: *const fact_store.FactStore,
    method_eid: u64,
) !Compiled {
    const bc_ref = store.currentValue(method_eid, kernel_eids.ATTR_BYTECODE_STREAM) orelse return error.NoBytecode;
    if (!strings.isString(bc_ref)) return error.NoBytecode;

    var pool = std.ArrayList(Ref){};
    defer pool.deinit(allocator);
    if (store.currentValue(method_eid, kernel_eids.ATTR_CONSTANT_POOL)) |pr| {
        if (boxed.isRrbRef(pr)) {
            const rrb = boxed.untagRrbRef(pr);
            for (0..rrb.count()) |i| try pool.append(allocator, rrb.nth(i));
        }
    }

    const c_src = codegen.emitMethod(allocator, "m", strings.bytes(bc_ref), pool.items, store) catch |e| switch (e) {
        error.UnsupportedOpcode => return error.Declined,
        else => return e,
    };
    defer allocator.free(c_src);

    var j = try jit.Jit.init(RUNTIME_LIB_DIR);
    errdefer j.deinit();
    try bindRuntime(&j);
    try j.compile(c_src);
    try j.relocate();
    return .{ .jit = j, .fn_ptr = try j.getFn(MethodFn, "m") };
}

/// Convenience (used by tests): compile + register + install in one call, return
/// the owning `Jit`. `JitManager` uses the finer-grained pieces instead.
pub fn jitAndInstall(
    allocator: std.mem.Allocator,
    store: *const fact_store.FactStore,
    table: *object_table.ObjectTable,
    method_eid: u64,
) !jit.Jit {
    var c = try compileMethod(allocator, store, method_eid);
    errdefer c.jit.deinit();
    const idx = registerFn(c.fn_ptr) orelse return error.JitRegistryFull;
    try installTrampoline(allocator, table, method_eid, idx);
    return c.jit;
}

/// Signature of a JIT'd method body (the output of `codegen.emitMethod`):
/// `Ref body(Ref recv, Ref *args, unsigned long n_args)`.
pub const MethodFn = *const fn (recv: Ref, args: [*]const Ref, n_args: usize) callconv(.c) Ref;

/// Side registry of JIT'd bodies. A trampoline closure's `data` is a **SmallInt
/// index** into this, NOT a raw fn-ptr: a code address is pointer-aligned, so
/// stored raw it would read as a pointer-tagged `Ref` and the GC's reachability
/// walk would dereference code memory and crash. A SmallInt is ignored by the GC.
/// Fixed-capacity + process-lifetime (no allocator); re-JIT appends a fresh slot
/// (old slots leak but are bounded). 4096 distinct JIT'd methods for now.
var g_registry: [4096]MethodFn = undefined;
var g_registry_len: usize = 0;

/// Append a JIT'd body, returning its index (the SmallInt parked in closure.data).
pub fn registerFn(f: MethodFn) ?u64 {
    if (g_registry_len >= g_registry.len) return null;
    const idx = g_registry_len;
    g_registry[idx] = f;
    g_registry_len += 1;
    return idx;
}

/// Overwrite an existing slot — a re-JIT reuses its method's index, so the
/// closure.data SmallInt stays valid and no slot leaks.
pub fn registrySet(idx: u64, f: MethodFn) void {
    if (idx < g_registry_len) g_registry[idx] = f;
}

/// Dispatch trampoline. `send.sendN` calls this directly (its `.method` is not
/// `bytecodeDispatcherImpl`, so it takes the honest fn-ptr path), and it forwards
/// the receiver + args to the JIT'd native body indexed by the closure's `data`
/// (a SmallInt — see `g_registry`).
pub fn trampoline(s: *send_mod.Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    const c = s.closure orelse return ref_mod.NIL;
    if (!ref_mod.isSmallInt(c.data)) return ref_mod.NIL;
    const idx: usize = @intCast(ref_mod.untagSmallInt(c.data));
    if (idx >= g_registry_len) return ref_mod.NIL;
    return g_registry[idx](s.receiver, args, n_args);
}

/// Build a `[vt][method][data]` closure buffer and put it at `method_eid`, then
/// flush the mcache. The mcache keys (vt, sel) → a *closure pointer*; swapping the
/// closure at `method_eid` would otherwise leave cached entries pointing at the
/// OLD closure (a redefine/demote could dispatch freed JIT code). A mode swap is
/// infrequent, so clearing all is fine.
fn putClosure(
    allocator: std.mem.Allocator,
    table: *object_table.ObjectTable,
    method_eid: u64,
    method: *const anyopaque,
    data: Ref,
) !void {
    const buf = try allocator.alignedAlloc(u8, .of(Ref), 3 * @sizeOf(Ref));
    @memset(buf, 0);
    const vt_slot: *Ref = @ptrCast(@alignCast(buf.ptr));
    vt_slot.* = ref_mod.mkEid(kernel_eids.BYTECODE_METHOD_VT);
    const c: *closure_mod.Closure = @ptrCast(@alignCast(buf.ptr + @sizeOf(Ref)));
    c.method = method;
    c.data = data;
    try table.put(method_eid, ref_mod.mkPointer(@ptrCast(c)));
    mcache.clearAll();
}

/// NATIVE: swap `method_eid`'s closure to `{trampoline, SmallInt(idx)}` so
/// dispatch runs the JIT'd body at registry `idx`.
pub fn installTrampoline(
    allocator: std.mem.Allocator,
    table: *object_table.ObjectTable,
    method_eid: u64,
    idx: u64,
) !void {
    const data = ref_mod.mkSmallInt(@intCast(idx)) orelse return error.JitRegistryFull;
    try putClosure(allocator, table, method_eid, @ptrCast(&trampoline), data);
}

/// BYTECODE (demote): swap `method_eid`'s closure back to the interpreter
/// `{bytecodeDispatcherImpl, mkEid(method_eid)}` — the shape `rematerializeClosures`
/// builds. Dispatch reverts to `runMethod` over the (canonical) `:bytecode-stream`.
pub fn installBytecode(
    allocator: std.mem.Allocator,
    table: *object_table.ObjectTable,
    method_eid: u64,
) !void {
    try putClosure(allocator, table, method_eid, @ptrCast(&dispatch.bytecodeDispatcherImpl), ref_mod.mkEid(method_eid));
}
