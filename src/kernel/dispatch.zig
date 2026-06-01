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
    try vm.frames.append(allocator, vm_mod.Frame{
        .bytecode = code.ptr,
        .pc = 0,
        .method_eid = method_eid,
        .self_ref = receiver,
        .bp = 0,
        .local_count = 0,
        .parent_method_frame = null,
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
            .SEND_0 => {
                const sel_eid: u64 = bytecode.readU32(code, pc + 1);
                _ = bytecode.readU16(code, pc + 5); // site_idx — mcache integration in a later phase
                if (value_stack.items.len == 0) return error.StackUnderflow;
                const recv = value_stack.pop().?;
                const result = send_mod.send0(recv, sel_eid);
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
                try value_stack.append(allocator, result);
                pc += 7;
            },
            .SEND_N => {
                const sel_eid: u64 = bytecode.readU32(code, pc + 1);
                const n_args: u8 = bytecode.readU8(code, pc + 5);
                _ = bytecode.readU16(code, pc + 6);
                if (value_stack.items.len < @as(usize, n_args) + 1) return error.StackUnderflow;
                // Args were pushed left-to-right; they sit at top with the
                // last one on top. Pass a contiguous slice to sendN.
                const top = value_stack.items.len;
                const args_start = top - @as(usize, n_args);
                const args_ptr: [*]const Ref = value_stack.items.ptr + args_start;
                const recv = value_stack.items[args_start - 1];
                const result = send_mod.sendN(recv, sel_eid, args_ptr, n_args);
                // Pop args + recv, push result.
                value_stack.shrinkRetainingCapacity(args_start - 1);
                try value_stack.append(allocator, result);
                pc += 8;
            },
            .MAKE_BLOCK => {
                const idx = bytecode.readU16(code, pc + 1);
                if (idx >= pool_items.len) return error.BadFrame;
                const method_ref = pool_items[idx];
                if (!ref_mod.isEid(method_ref)) return error.BadFrame;
                const block_method_eid = ref_mod.untagEid(method_ref);
                // Phase 11.11 — read capture spec; if any, allocate a
                // BytecodeBlock carrying the captured values from this
                // frame. Otherwise push the bare method-eid Ref (fast
                // path for non-capturing blocks).
                const spec_opt = store.currentValue(block_method_eid, kernel_eids.ATTR_METHOD_CAPTURES_PTR);
                if (spec_opt) |spec_ref| {
                    const specs = vm_mod.captureSpecsFromRef(spec_ref);
                    if (specs.len > 0) {
                        const blk = try vm_mod.allocBytecodeBlock(allocator, block_method_eid, @intCast(specs.len));
                        for (specs, 0..) |sp, i| {
                            if (sp.parent_slot >= locals.items.len) {
                                blk.captures_tail[i] = ref_mod.NIL;
                            } else {
                                blk.captures_tail[i] = locals.items[sp.parent_slot];
                            }
                        }
                        try value_stack.append(allocator, blk.ref);
                        pc += 3;
                        continue;
                    }
                }
                try value_stack.append(allocator, method_ref);
                pc += 3;
            },
            .INVOKE_BLOCK => {
                const n_args: u8 = bytecode.readU8(code, pc + 1);
                _ = bytecode.readU16(code, pc + 2); // site_idx — unused in spike
                if (value_stack.items.len < @as(usize, n_args) + 1) return error.StackUnderflow;
                const top = value_stack.items.len;
                const args_start = top - @as(usize, n_args);
                const block_ref = value_stack.items[args_start - 1];
                // Block ref is either mkEid(method_eid) (no captures) or
                // a pointer Ref to BytecodeBlock (with captures).
                var block_eid: u64 = 0;
                var block_captures: []const Ref = &[_]Ref{};
                if (ref_mod.isEid(block_ref)) {
                    block_eid = ref_mod.untagEid(block_ref);
                } else if (ref_mod.isPointer(block_ref)) {
                    block_eid = vm_mod.blockMethodEidFromRef(block_ref);
                    block_captures = vm_mod.blockCapturesFromRef(block_ref);
                } else {
                    return error.BadFrame;
                }
                // Snapshot args before shrinking the stack.
                const args_snapshot = try allocator.dupe(Ref, value_stack.items[args_start..top]);
                defer allocator.free(args_snapshot);
                value_stack.shrinkRetainingCapacity(args_start - 1);
                // Block sees the enclosing frame's self.
                const result = try runMethod(allocator, store, receiver, block_eid, args_snapshot, block_captures);
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
    if (!ref_mod.isEid(closure.data)) return ref_mod.NIL;
    const method_eid = ref_mod.untagEid(closure.data);
    const allocator = send_mod.kernelAllocator() orelse return ref_mod.NIL;
    const store = send_mod.storePtr() orelse return ref_mod.NIL;
    const result = runMethod(allocator, store, s.receiver, method_eid, args[0..n_args], &[_]Ref{}) catch
        return ref_mod.NIL;
    return result;
}

// -- Phase 11.6 — reify-stack API ------------------------------------------

/// Materialize the current Vm frame stack as transient fact-entities.
/// Returns the EIDs of the asserted frame entities, deepest-frame-first
/// (caller frame at index 0, current/innermost at end). Caller owns the
/// returned slice.
///
/// Asserted facts per frame (under `module_eid` provenance):
///   [f :vtable ?]                 (left null — frames have no vt yet)
///   [f :frame-pc <SmallInt>]
///   [f :frame-method <method-eid-Ref>]
///   [f :frame-self <Ref>]         (omitted if NIL — Date discipline)
///   [f :frame-parent <prev-frame-eid-Ref>]  (omitted for stack base)
///
/// Phase 11.6 ships single-thread semantics — the snapshot is taken
/// from the current Vm only. Multi-thread "snapshot under tx" lands
/// when the Datomic transactor introduces the channel boundary.
pub fn reifyStack(
    allocator: std.mem.Allocator,
    store: *FactStore,
    table: *ObjectTable,
    module_eid: u64,
) ![]u64 {
    const vm = vm_mod.current();
    const n = vm.frames.items.len;
    var eids = try allocator.alloc(u64, n);
    errdefer allocator.free(eids);

    for (vm.frames.items, 0..) |f, i| {
        const eid = table.alloc();
        eids[i] = eid;
        const pc_ref = ref_mod.mkSmallInt(@intCast(f.pc)) orelse return error.OutOfMemory;
        _ = try store.assert(eid, kernel_eids.ATTR_FRAME_PC, pc_ref, module_eid);
        _ = try store.assert(eid, kernel_eids.ATTR_FRAME_METHOD, ref_mod.mkEid(f.method_eid), module_eid);
        if (!ref_mod.isNil(f.self_ref)) {
            _ = try store.assert(eid, kernel_eids.ATTR_FRAME_SELF, f.self_ref, module_eid);
        }
        if (i > 0) {
            _ = try store.assert(eid, kernel_eids.ATTR_FRAME_PARENT, ref_mod.mkEid(eids[i - 1]), module_eid);
        }
    }
    return eids;
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
