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
/// pushed as locals[0..args.len].
pub fn runMethod(
    allocator: std.mem.Allocator,
    store: *const FactStore,
    receiver: Ref,
    method_eid: u64,
    args: []const Ref,
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

    var value_stack = std.ArrayList(Ref){};
    defer value_stack.deinit(allocator);

    var pc: usize = 0;
    while (pc < code.len) {
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
                // A8b spike: block representation = the block-method-eid
                // Ref straight from the pool. INVOKE_BLOCK runs it via
                // runMethod. Full Block-obj allocation with scope capture
                // lands once closures-over-outer-locals are wired.
                try value_stack.append(allocator, pool_items[idx]);
                pc += 3;
            },
            .INVOKE_BLOCK => {
                const n_args: u8 = bytecode.readU8(code, pc + 1);
                _ = bytecode.readU16(code, pc + 2); // site_idx — unused in spike
                if (value_stack.items.len < @as(usize, n_args) + 1) return error.StackUnderflow;
                const top = value_stack.items.len;
                const args_start = top - @as(usize, n_args);
                const block_ref = value_stack.items[args_start - 1];
                if (!ref_mod.isEid(block_ref)) return error.BadFrame;
                const block_eid = ref_mod.untagEid(block_ref);
                // Snapshot args before shrinking the stack.
                const args_snapshot = try allocator.dupe(Ref, value_stack.items[args_start..top]);
                defer allocator.free(args_snapshot);
                value_stack.shrinkRetainingCapacity(args_start - 1);
                // Block sees the enclosing frame's self.
                const result = try runMethod(allocator, store, receiver, block_eid, args_snapshot);
                // Non-local return propagation: if the block triggered
                // RETURN_FROM_METHOD, the inner runMethod returned its
                // unwind_value. Decide here whether we are the target
                // method frame.
                const vm = vm_mod.current();
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
                const vm = vm_mod.current();
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
