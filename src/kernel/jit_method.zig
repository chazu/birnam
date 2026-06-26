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
const closure_mod = @import("closure.zig");
const kernel_eids = @import("kernel_eids.zig");

const Ref = ref_mod.Ref;

/// Signature of a JIT'd method body (the output of `codegen.emitMethod`):
/// `Ref body(Ref recv, Ref *args, unsigned long n_args)`.
pub const MethodFn = *const fn (recv: Ref, args: [*]const Ref, n_args: usize) callconv(.c) Ref;

/// Dispatch trampoline. `send.sendN` calls this directly (its `.method` is not
/// `bytecodeDispatcherImpl`, so it takes the honest fn-ptr path), and it forwards
/// the receiver + args to the JIT'd native body whose address is parked in the
/// closure's `data` slot. The address is a code-segment pointer, not a heap Ref:
/// the GC's arena sweep never follows it and it is never serialized (the closure
/// is an object-table cache rebuilt/re-JIT'd on load), so storing it raw is safe.
pub fn trampoline(s: *send_mod.Send, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    const c = s.closure orelse return ref_mod.NIL;
    const f: MethodFn = @ptrFromInt(@as(usize, @intCast(c.data)));
    return f(s.receiver, args, n_args);
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
    c.data = @as(Ref, @intCast(@intFromPtr(jit_fn)));
    try table.put(method_eid, ref_mod.mkPointer(@ptrCast(c)));
}
