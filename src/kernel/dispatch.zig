//! Phase 11.3 — bytecode VM dispatch.
//!
//! Spike implementation: simple `while + switch` loop. Tail-call dispatch
//! per ADR-11 #4 is the eventual shape — the 11.9 probe already verifies
//! Zig lowers `@call(.always_tail, ...)` to `jmp` on host arch. Switching
//! over once handler coverage is complete is a localized change.
//!
//! A6 scope: literal pushes, locals, POP, control flow, RETURN. No SENDs,
//! no blocks. A7 adds SEND_* via the mcache + raw-EAVT bind path.

const std = @import("std");
const fact_store_mod = @import("fact_store.zig");
const object_table_mod = @import("object_table.zig");
const ref_mod = @import("ref.zig");
const kernel_eids = @import("kernel_eids.zig");
const bytecode = @import("bytecode.zig");
const strings = @import("strings.zig");
const boxed = @import("boxed.zig");
const send_mod = @import("send.zig");
const vm_mod = @import("vm.zig");
const closure_mod = @import("closure.zig");
const arenas = @import("arenas.zig");

pub const Ref = ref_mod.Ref;
pub const Opcode = bytecode.Opcode;
pub const FactStore = fact_store_mod.FactStore;
pub const ObjectTable = object_table_mod.ObjectTable;

pub const RunError = error{
    UnknownMethod,
    MissingBytecode,
    BadFrame,
    StackUnderflow,
    OpcodeNotImplemented,
    OutOfMemory,
};

/// Execute a bytecode method. Returns the value on top of the value
/// stack at RETURN. `receiver` becomes the frame's self_ref. `args` are
/// loaded as locals[0..args.len]; `captures` are copied per the method's
/// `:method-captures-ptr` spec into their assigned local slots.
pub fn runMethod(
    allocator: std.mem.Allocator,
    store: *const FactStore,
    receiver: Ref,
    method_eid: u64,
    args: []const Ref,
    captures: []const Ref,
) RunError!Ref {
    noteCall(method_eid); // hotness — auto-JIT trigger (no-op unless enabled)
    const bc_ref = store.currentValue(method_eid, kernel_eids.ATTR_BYTECODE_STREAM) orelse
        return error.MissingBytecode;
    if (!strings.isString(bc_ref)) return error.MissingBytecode;
    const code = strings.bytes(bc_ref);

    var pool_items: []const Ref = &[_]Ref{};
    if (store.currentValue(method_eid, kernel_eids.ATTR_CONSTANT_POOL)) |pool_ref| {
        if (boxed.isRrbRef(pool_ref)) {
            // RRB iteration; small pools so just collect once.
            const rrb = boxed.untagRrbRef(pool_ref);
            const n = rrb.count();
            const buf = try allocator.alloc(Ref, n);
            for (0..n) |i| buf[i] = rrb.nth(i);
            pool_items = buf;
        }
    }
    defer if (pool_items.len > 0) allocator.free(pool_items);

    var locals = std.ArrayList(Ref){};
    defer locals.deinit(allocator);
    for (args) |a| try locals.append(allocator, a);

    // Phase 11.11 — copy captures into the slots the compiler assigned.
    // Capture spec is a heap-allocated CaptureSpecArray pointer-Ref.
    if (store.currentValue(method_eid, kernel_eids.ATTR_METHOD_CAPTURES_PTR)) |spec_ref| {
        const specs = vm_mod.captureSpecsFromRef(spec_ref);
        for (specs, 0..) |sp, i| {
            if (i >= captures.len) break;
            while (locals.items.len <= sp.local_slot) try locals.append(allocator, ref_mod.NIL);
            locals.items[sp.local_slot] = captures[i];
        }
    }

    var value_stack = std.ArrayList(Ref){};
    defer value_stack.deinit(allocator);

    // Phase 11.6 — track this activation on Vm.frames for reify-stack.
    // Capacity reserved upfront so pointers stay stable for pc write-
    // through during the loop.
    const vm = vm_mod.current();
    try vm_mod.ensureFrameCapacity(allocator, vm.frames.items.len + 16);
    const this_frame_id = vm.next_frame_id;
    vm.next_frame_id += 1;
    try vm.frames.append(allocator, vm_mod.Frame{
        .bytecode = code.ptr,
        .pc = 0,
        .method_eid = method_eid,
        .self_ref = receiver,
        .bp = 0,
        .local_count = 0,
        .parent_method_frame = null,
        .frame_id = this_frame_id,
    });
    const frame_idx = vm.frames.items.len - 1;
    defer {
        _ = vm.frames.pop();
        // When the stack drains, release the backing buffer so a
        // subsequent runMethod call from a different allocator scope
        // doesn't reallocate against a dead allocator. Reify-stack only
        // sees active frames, so freeing on drain is consistent with the
        // semantics.
        if (vm.frames.items.len == 0) vm_mod.freeFrames(allocator);
    }

    var pc: usize = 0;
    while (pc < code.len) {
        vm.frames.items[frame_idx].pc = pc;
        const op = bytecode.readOpcode(code, pc);
        switch (op) {
            .PUSH_SMALLINT_I32 => {
                const n = bytecode.readI32(code, pc + 1);
                const r = ref_mod.mkSmallInt(@as(i64, n)) orelse return error.OutOfMemory;
                try value_stack.append(allocator, r);
                pc += 5;
            },
            .PUSH_CONST => {
                const idx = bytecode.readU16(code, pc + 1);
                if (idx >= pool_items.len) return error.BadFrame;
                try value_stack.append(allocator, pool_items[idx]);
                pc += 3;
            },
            .LOAD_CONST_TRUE => {
                try value_stack.append(allocator, ref_mod.mkEid(kernel_eids.TRUE_EID));
                pc += 1;
            },
            .LOAD_CONST_FALSE => {
                try value_stack.append(allocator, ref_mod.mkEid(kernel_eids.FALSE_EID));
                pc += 1;
            },
            .LOAD_CONST_NIL => {
                try value_stack.append(allocator, ref_mod.NIL);
                pc += 1;
            },
            .LOAD_SELF => {
                try value_stack.append(allocator, receiver);
                pc += 1;
            },
            .PUSH_LOCAL => {
                const slot = bytecode.readU8(code, pc + 1);
                if (slot >= locals.items.len) return error.BadFrame;
                try value_stack.append(allocator, locals.items[slot]);
                pc += 2;
            },
            .STORE_LOCAL => {
                const slot = bytecode.readU8(code, pc + 1);
                if (value_stack.items.len == 0) return error.StackUnderflow;
                const v = value_stack.pop().?;
                // Grow locals to fit slot index.
                while (locals.items.len <= slot) try locals.append(allocator, ref_mod.NIL);
                locals.items[slot] = v;
                pc += 2;
            },
            .POP => {
                if (value_stack.items.len == 0) return error.StackUnderflow;
                _ = value_stack.pop();
                pc += 1;
            },
            .JUMP => {
                const off = bytecode.readI16(code, pc + 1);
                pc = applyJump(pc + 3, off);
            },
            .BRANCH_IF_FALSE => {
                if (value_stack.items.len == 0) return error.StackUnderflow;
                const v = value_stack.pop().?;
                const off = bytecode.readI16(code, pc + 1);
                const after = pc + 3;
                pc = if (isFalsey(v)) applyJump(after, off) else after;
            },
            .BRANCH_IF_ERROR => {
                // Peek TOS: if it's an error value, leave it on the stack and
                // jump (the statement sequence's result IS the error — halt);
                // otherwise discard the non-final statement's value and fall
                // through. Phase 17.7 finish — VM stmt-seq mid-halt on error.
                if (value_stack.items.len == 0) return error.StackUnderflow;
                const v = value_stack.items[value_stack.items.len - 1];
                const off = bytecode.readI16(code, pc + 1);
                const after = pc + 3;
                if (isErrorValue(v)) {
                    pc = applyJump(after, off);
                } else {
                    _ = value_stack.pop();
                    pc = after;
                }
            },
            .SEND_0 => {
                const sel_eid: u64 = bytecode.readU32(code, pc + 1);
                _ = bytecode.readU16(code, pc + 5);
                if (value_stack.items.len == 0) return error.StackUnderflow;
                const recv = value_stack.pop().?;
                const result = send_mod.send0(recv, sel_eid);
                if (try handleSendUnwind(vm, result)) |caught| return caught;
                try value_stack.append(allocator, result);
                pc += 7;
            },
            .SEND_1 => {
                const sel_eid: u64 = bytecode.readU32(code, pc + 1);
                _ = bytecode.readU16(code, pc + 5);
                if (value_stack.items.len < 2) return error.StackUnderflow;
                const arg = value_stack.pop().?;
                const recv = value_stack.pop().?;
                const result = send_mod.send1(recv, sel_eid, arg);
                if (try handleSendUnwind(vm, result)) |caught| return caught;
                try value_stack.append(allocator, result);
                pc += 7;
            },
            .SEND_N => {
                const sel_eid: u64 = bytecode.readU32(code, pc + 1);
                const n_args: u8 = bytecode.readU8(code, pc + 5);
                _ = bytecode.readU16(code, pc + 6);
                if (value_stack.items.len < @as(usize, n_args) + 1) return error.StackUnderflow;
                const top = value_stack.items.len;
                const args_start = top - @as(usize, n_args);
                const args_ptr: [*]const Ref = value_stack.items.ptr + args_start;
                const recv = value_stack.items[args_start - 1];
                const result = send_mod.sendN(recv, sel_eid, args_ptr, n_args);
                value_stack.shrinkRetainingCapacity(args_start - 1);
                if (try handleSendUnwind(vm, result)) |caught| return caught;
                try value_stack.append(allocator, result);
                pc += 8;
            },
            .MAKE_BLOCK => {
                const idx = bytecode.readU16(code, pc + 1);
                if (idx >= pool_items.len) return error.BadFrame;
                const method_ref = pool_items[idx];
                if (!ref_mod.isEid(method_ref)) return error.BadFrame;
                const block_method_eid = ref_mod.untagEid(method_ref);

                // Phase 11.12 — every MAKE_BLOCK allocates a real closure
                // entity wrapping a BytecodeClosure heap struct that
                // carries (method_eid, parent_self_ref, parent_frame_id,
                // captures). Closure entity has :vtable CLOSURE_VT so
                // `[block value]` dispatches via send.bind +
                // closureValueImpl + bytecodeDispatcherImpl uniformly,
                // whether the value-send originates from tree-walker
                // or bytecode context.
                var n_captures: u32 = 0;
                var specs: []const vm_mod.CaptureSpec = &[_]vm_mod.CaptureSpec{};
                if (store.currentValue(block_method_eid, kernel_eids.ATTR_METHOD_CAPTURES_PTR)) |spec_ref| {
                    specs = vm_mod.captureSpecsFromRef(spec_ref);
                    n_captures = @intCast(specs.len);
                }

                const parent_self = receiver;
                const parent_frame_id = vm.frames.items[frame_idx].frame_id;
                const blk = try vm_mod.allocBytecodeBlock(allocator, block_method_eid, parent_self, parent_frame_id, n_captures);
                for (specs, 0..) |sp, i| {
                    if (sp.parent_slot >= locals.items.len) {
                        blk.captures_tail[i] = ref_mod.NIL;
                    } else {
                        blk.captures_tail[i] = locals.items[sp.parent_slot];
                    }
                }

                // Wrap the BytecodeClosure ptr in a libid-object closure
                // ([vt_ref][method=bytecodeDispatcherImpl][data=blk_ref])
                // and register a fresh closure-eid. Push mkEid(closure_eid).
                const closure_eid = try allocClosureEntity(allocator, blk.ref);
                try value_stack.append(allocator, ref_mod.mkEid(closure_eid));
                pc += 3;
            },
            .INVOKE_BLOCK => {
                const n_args: u8 = bytecode.readU8(code, pc + 1);
                _ = bytecode.readU16(code, pc + 2);
                if (value_stack.items.len < @as(usize, n_args) + 1) return error.StackUnderflow;
                const top = value_stack.items.len;
                const args_start = top - @as(usize, n_args);
                const block_ref = value_stack.items[args_start - 1];
                if (!ref_mod.isEid(block_ref)) return error.BadFrame;
                // Look up the closure entity to recover method_eid, self,
                // captures from its BytecodeClosure data.
                const closure_eid = ref_mod.untagEid(block_ref);
                const tbl = send_mod.tablePtr() orelse return error.BadFrame;
                const closure_ptr_ref = tbl.get(closure_eid) orelse return error.BadFrame;
                if (!ref_mod.isPointer(closure_ptr_ref)) return error.BadFrame;
                const closure_base: [*]const Ref = @ptrCast(@alignCast(ref_mod.untagPointer(closure_ptr_ref)));
                const data = closure_base[1];

                var block_eid: u64 = 0;
                var block_captures: []const Ref = &[_]Ref{};
                var block_self: Ref = receiver;
                if (ref_mod.isEid(data)) {
                    block_eid = ref_mod.untagEid(data);
                } else if (ref_mod.isPointer(data)) {
                    block_eid = vm_mod.blockMethodEidFromRef(data);
                    block_captures = vm_mod.blockCapturesFromRef(data);
                    block_self = vm_mod.blockParentSelfFromRef(data);
                } else {
                    return error.BadFrame;
                }

                const args_snapshot = try allocator.dupe(Ref, value_stack.items[args_start..top]);
                defer allocator.free(args_snapshot);
                value_stack.shrinkRetainingCapacity(args_start - 1);
                const result = try runMethod(allocator, store, block_self, block_eid, args_snapshot, block_captures);
                // Non-local return propagation: if the block triggered
                // RETURN_FROM_METHOD, the inner runMethod returned its
                // unwind_value. Decide here whether we are the target
                // method frame.
                if (vm.unwinding) {
                    if (vm.unwind_target_remaining == 0) {
                        // We ARE the target method. Catch, return the
                        // unwind value as our normal result.
                        vm.unwinding = false;
                        return vm.unwind_value;
                    } else {
                        // Pass through — caller is a still-inner frame.
                        vm.unwind_target_remaining -= 1;
                        return result;
                    }
                }
                try value_stack.append(allocator, result);
                pc += 4;
            },
            .RETURN_FROM_METHOD => {
                const depth: u8 = bytecode.readU8(code, pc + 1);
                if (value_stack.items.len == 0) return error.StackUnderflow;
                const v = value_stack.pop().?;
                vm.unwinding = true;
                vm.unwind_value = v;
                vm.unwind_target_remaining = depth;
                return v;
            },
            .RETURN => {
                if (value_stack.items.len == 0) return ref_mod.NIL;
                return value_stack.items[value_stack.items.len - 1];
            },
        }
    }
    return error.BadFrame;
}

// -- MAKE_BLOCK helper ------------------------------------------------------

/// Mint a closure entity wrapping a BytecodeClosure pointer. Returns the
/// fresh closure EID. The entity's object-table slot points at a
/// libid-object-style `[vt_ref][method][data]` 3-Ref buffer with vt =
/// CLOSURE_VT, method = bytecodeDispatcherImpl, data = closure_data_ref.
fn allocClosureEntity(allocator: std.mem.Allocator, closure_data_ref: Ref) RunError!u64 {
    const table = send_mod.tablePtr() orelse return error.BadFrame;
    const total = 3 * @sizeOf(Ref);
    // Bytecode block wrappers churn per `value`/`whileTrue:`; route to the swept
    // closures_vt arena when live so :gc reclaims dead ones (Phase 15.A). Its
    // `data` is the captures-arena BytecodeClosure, pinned via consider(data).
    const a = if (arenas.ready()) arenas.closuresVt() else allocator;
    const buf = a.alignedAlloc(u8, .of(Ref), total) catch return error.OutOfMemory;
    @memset(buf, 0);
    const vt_slot: *Ref = @ptrCast(@alignCast(buf.ptr));
    vt_slot.* = ref_mod.mkEid(kernel_eids.CLOSURE_VT);
    const c_ptr: *closure_mod.Closure = @ptrCast(@alignCast(buf.ptr + @sizeOf(Ref)));
    c_ptr.* = .{ .method = @ptrCast(&bytecodeDispatcherImpl), .data = closure_data_ref };
    const eid = table.alloc();
    table.put(eid, ref_mod.mkPointer(@ptrCast(c_ptr))) catch return error.OutOfMemory;
    return eid;
}

// -- Bytecode dispatcher Imp ------------------------------------------------

/// Closure body for compiled bytecode methods. When a bytecode-method
/// entity is installed via `[vt sel mkEid(method_eid)]` and the method's
/// object-table slot holds a pointer to `Closure { method =
/// &bytecodeDispatcherImpl, data = mkEid(method_eid) }`, sending `sel`
/// to a `vt`-tagged receiver dispatches here. The closure's `data`
/// field carries the method EID — used to look up bytecode + constant
/// pool from the fact store.
pub fn bytecodeDispatcherImpl(
    s: *send_mod.Send,
    args: [*]const Ref,
    n_args: usize,
) callconv(.c) Ref {
    const closure = s.closure orelse return ref_mod.NIL;
    const allocator = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const store = send_mod.storePtr() orelse return ref_mod.NIL;

    var method_eid: u64 = 0;
    var self_ref: Ref = s.receiver;
    var captures: []const Ref = &[_]Ref{};

    if (ref_mod.isEid(closure.data)) {
        // Direct method invocation (closure installed by compiler.materialize).
        // `self` = caller-supplied receiver. Phase 12.8: primary method dispatch
        // no longer reaches here — `send.sendN` enters `runMethod` directly at the
        // bind boundary. This branch now serves only `value`-on-a-method-eid
        // (closureValueImpl calls `.method` through the buffer) + any defensive
        // fn-ptr call site.
        method_eid = ref_mod.untagEid(closure.data);
    } else if (ref_mod.isPointer(closure.data)) {
        // Block closure (allocated at MAKE_BLOCK). `self` comes from the
        // enclosing method's frame at MAKE_BLOCK time.
        method_eid = vm_mod.blockMethodEidFromRef(closure.data);
        self_ref = vm_mod.blockParentSelfFromRef(closure.data);
        captures = vm_mod.blockCapturesFromRef(closure.data);
    } else {
        return ref_mod.NIL;
    }

    const result = runMethod(allocator, store, self_ref, method_eid, args[0..n_args], captures) catch
        return ref_mod.NIL;
    return result;
}

// -- Phase 11.6 — reify-stack API ------------------------------------------
//
// Stack reification (materializing Vm frames as facts) moved to `reify.zig`
// (W6 cut #4): it *writes* facts, an L3+ concern, so it lives above the
// dispatch layer (L2). Core dispatch writes no facts. See `reify.reifyStack`.

/// Phase 11.12 — non-local return propagation after a SEND. If the
/// inner method invoked `^expr` and `unwind_target_remaining` is zero,
/// this frame is the target — catch by returning the unwind value.
/// Otherwise decrement the depth and propagate by exiting our own
/// dispatch loop with the inner result.
inline fn handleSendUnwind(vm: *vm_mod.Vm, inner_result: Ref) RunError!?Ref {
    if (!vm.unwinding) return null;
    if (vm.unwind_target_remaining == 0) {
        vm.unwinding = false;
        return vm.unwind_value;
    }
    vm.unwind_target_remaining -= 1;
    return inner_result;
}

inline fn applyJump(after: usize, offset: i16) usize {
    const signed: i64 = @as(i64, @intCast(after)) + offset;
    return @intCast(signed);
}

/// Boolean-falseness convention: NIL and FALSE_EID are falsey; everything
/// else (including 0, empty string, etc.) is truthy. Matches Smalltalk
/// semantics — message-receivers `True` / `False` carry the dispatch.
inline fn isFalsey(v: Ref) bool {
    if (ref_mod.isNil(v)) return true;
    if (ref_mod.isEid(v) and ref_mod.untagEid(v) == kernel_eids.FALSE_EID) return true;
    return false;
}

/// True iff `v` is an ERROR_VT entity (a propagating Result error). Backs
/// BRANCH_IF_ERROR so a compiled statement sequence halts mid-sequence on an
/// error, matching the tree-walker (Phase 17.7 finish).
inline fn isErrorValue(v: Ref) bool {
    if (!ref_mod.isEid(v)) return false;
    const vt = send_mod.vtableEidOf(v) orelse return false;
    return vt == kernel_eids.ERROR_VT;
}

// -- C-ABI predicates for the codegen (docs/10a / libtcc) -------------------
// Generated method bodies lowering BRANCH_IF_FALSE / BRANCH_IF_ERROR call these
// so the falsey/error semantics stay defined in one place (here), not duplicated
// in emitted C. Registered via `Jit.addSymbol`.
pub fn birnamIsFalsey(v: Ref) callconv(.c) c_int {
    return if (isFalsey(v)) 1 else 0;
}
pub fn birnamIsError(v: Ref) callconv(.c) c_int {
    return if (isErrorValue(v)) 1 else 0;
}

// -- Hotness / auto-JIT (docs/10a — interpret cold, compile hot) ------------
// A cheap call counter at `runMethod` entry. Once a bytecode method is called
// HOT_THRESHOLD times it is queued; the shell drains the queue at the quiescent
// REPL boundary and promotes each to native (so the JIT runs OUTSIDE dispatch,
// and the kernel never statically depends on libtcc). Direct-mapped counters
// (eid % SLOTS) — collisions only perturb the heuristic, never correctness; a
// method that goes native stops calling `runMethod`, so its count freezes.
const HOT_THRESHOLD: u32 = 64;
const HOT_SLOTS: usize = 8192;
var g_hot_enabled: bool = false;
var g_hot_counters = [_]u32{0} ** HOT_SLOTS;
var g_hot_pending: [512]u64 = undefined;
var g_hot_pending_len: usize = 0;

inline fn noteCall(method_eid: u64) void {
    if (!g_hot_enabled) return;
    const slot = method_eid % HOT_SLOTS;
    g_hot_counters[slot] +%= 1;
    if (g_hot_counters[slot] >= HOT_THRESHOLD) {
        g_hot_counters[slot] = 0;
        if (g_hot_pending_len < g_hot_pending.len) {
            g_hot_pending[g_hot_pending_len] = method_eid;
            g_hot_pending_len += 1;
        }
    }
}

/// Enable/disable hotness-triggered auto-JIT (the shell wires this to `:autojit`).
pub fn setAutoJit(on: bool) void {
    g_hot_enabled = on;
}
pub fn autoJitEnabled() bool {
    return g_hot_enabled;
}

/// Move the queued hot method eids into `buf`, clearing the queue. The shell
/// calls this at the line boundary and promotes each via the JIT manager.
pub fn drainHotMethods(buf: []u64) usize {
    const n = @min(buf.len, g_hot_pending_len);
    @memcpy(buf[0..n], g_hot_pending[0..n]);
    g_hot_pending_len = 0;
    return n;
}

// -- C-ABI block ops for the codegen (MAKE_BLOCK / INVOKE_BLOCK) -------------
// A JIT'd method that contains blocks emits these. The block BODY is a separate
// method (interpreted via runMethod), so only block creation + invocation cross
// into the JIT'd C; the body runs unchanged. `parent_frame_id = 0`: a JIT'd
// method has no VM frame, so a block that does a non-local return (`^`) back into
// it is unsupported (returns its value locally) — value-returning blocks (the
// common case: ifTrue:/collect:/whileTrue: arms) work fully.

/// Mirror of MAKE_BLOCK: allocate a `BytecodeClosure { method, self, frame=0,
/// caps }` (caps already resolved by the caller from its locals, in spec order)
/// and wrap it in a closure entity. Returns `mkEid(closure)`.
pub fn birnamMakeBlock(method_eid: u64, parent_self: Ref, caps: [*]const Ref, n_caps: usize) callconv(.c) Ref {
    const allocator = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const blk = vm_mod.allocBytecodeBlock(allocator, method_eid, parent_self, 0, @intCast(n_caps)) catch return ref_mod.NIL;
    var i: usize = 0;
    while (i < n_caps) : (i += 1) blk.captures_tail[i] = caps[i];
    const closure_eid = allocClosureEntity(allocator, blk.ref) catch return ref_mod.NIL;
    return ref_mod.mkEid(closure_eid);
}

/// Mirror of INVOKE_BLOCK: invoke the block closure `block_ref` with `args`,
/// returning its result. Replicates the interpreter's non-local-return bookkeeping
/// (decrement `unwind_target_remaining`, catch at 0) so `^` propagation matches
/// for the cases it can reach.
pub fn birnamInvokeBlock(block_ref: Ref, args: [*]const Ref, n_args: usize) callconv(.c) Ref {
    if (!ref_mod.isEid(block_ref)) return ref_mod.NIL;
    const allocator = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    const tbl = send_mod.tablePtr() orelse return ref_mod.NIL;
    const closure_ptr_ref = tbl.get(ref_mod.untagEid(block_ref)) orelse return ref_mod.NIL;
    if (!ref_mod.isPointer(closure_ptr_ref)) return ref_mod.NIL;
    const closure_base: [*]const Ref = @ptrCast(@alignCast(ref_mod.untagPointer(closure_ptr_ref)));
    const data = closure_base[1];

    var block_eid: u64 = 0;
    var captures: []const Ref = &[_]Ref{};
    var block_self: Ref = ref_mod.NIL;
    if (ref_mod.isEid(data)) {
        block_eid = ref_mod.untagEid(data);
    } else if (ref_mod.isPointer(data)) {
        block_eid = vm_mod.blockMethodEidFromRef(data);
        captures = vm_mod.blockCapturesFromRef(data);
        block_self = vm_mod.blockParentSelfFromRef(data);
    } else return ref_mod.NIL;

    const vm = vm_mod.current();
    const result = runMethod(allocator, store, block_self, block_eid, args[0..n_args], captures) catch return ref_mod.NIL;
    if (vm.unwinding) {
        if (vm.unwind_target_remaining == 0) {
            vm.unwinding = false;
            return vm.unwind_value;
        }
        vm.unwind_target_remaining -= 1;
    }
    return result;
}
