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

/// Where libtcc's runtime archive lives on the dev install (`tcc_add_library_path`).
/// A shipped birnam bundles it and overrides this. TODO: make configurable.
pub const RUNTIME_LIB_DIR: [*:0]const u8 = "/usr/local/lib/tcc";

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
}

/// The whole mode-1 pipeline for one method: read its `:bytecode-stream` +
/// `:constant-pool`, lower to C (`codegen.emitMethod`), JIT (libtcc), and install
/// the native body AS the method (`install`). Returns the owning `Jit` — the
/// caller keeps it alive (its memory backs the code) and `deinit`s it to drop the
/// native cache (reverting to the interpreter on next `rematerializeClosures` /
/// re-install). Returns `error.Declined` if the body uses an opcode the codegen
/// doesn't lower yet (e.g. blocks) — caller keeps the interpreter.
pub fn jitAndInstall(
    allocator: std.mem.Allocator,
    store: *const fact_store.FactStore,
    table: *object_table.ObjectTable,
    method_eid: u64,
) !jit.Jit {
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

    const c_src = codegen.emitMethod(allocator, "m", strings.bytes(bc_ref), pool.items) catch |e| switch (e) {
        error.UnsupportedOpcode => return error.Declined,
        else => return e,
    };
    defer allocator.free(c_src);

    var j = try jit.Jit.init(RUNTIME_LIB_DIR);
    errdefer j.deinit();
    try bindRuntime(&j);
    try j.compile(c_src);
    try j.relocate();
    const f = try j.getFn(MethodFn, "m");
    try install(allocator, table, method_eid, f);
    return j;
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

fn registerFn(f: MethodFn) ?u64 {
    if (g_registry_len >= g_registry.len) return null;
    const idx = g_registry_len;
    g_registry[idx] = f;
    g_registry_len += 1;
    return idx;
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

/// Swap `method_eid`'s object-table closure to `{trampoline, <jit_fn addr>}` so
/// dispatch to it runs `jit_fn` (native) instead of the bytecode interpreter. The
/// caller owns + keeps alive the `Jit` whose memory backs `jit_fn`. Reverting to
/// the interpreter is just re-installing the bytecode closure (or dropping this
/// slot and letting `rematerializeClosures` rebuild it from the persisted facts).
pub fn install(
    allocator: std.mem.Allocator,
    table: *object_table.ObjectTable,
    method_eid: u64,
    jit_fn: MethodFn,
) !void {
    const total = 3 * @sizeOf(Ref);
    const buf = try allocator.alignedAlloc(u8, .of(Ref), total);
    @memset(buf, 0);
    const vt_slot: *Ref = @ptrCast(@alignCast(buf.ptr));
    vt_slot.* = ref_mod.mkEid(kernel_eids.BYTECODE_METHOD_VT);
    const c: *closure_mod.Closure = @ptrCast(@alignCast(buf.ptr + @sizeOf(Ref)));
    c.method = @ptrCast(&trampoline);
    const idx = registerFn(jit_fn) orelse return error.JitRegistryFull;
    c.data = ref_mod.mkSmallInt(@intCast(idx)) orelse return error.JitRegistryFull;
    try table.put(method_eid, ref_mod.mkPointer(@ptrCast(c)));
    // The method cache keys (vt, sel) → a *closure pointer*; we just swapped the
    // closure at `method_eid`, so any cached entry now points at the OLD closure
    // (a redefine would dispatch freed JIT code). Flush — caches repopulate on the
    // next send. A JIT install is infrequent, so clearing all is fine.
    mcache.clearAll();
}
