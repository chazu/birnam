//! Phase 11.7.i — per-thread VM state.
//!
//! Holds execution-time state that was previously kernel globals
//! (recursion guard, FFI pause counter). Forward-compat for Phase 11
//! bytecode VM (frame stack, pc, current_self, unwinding) and for the
//! Datomic multi-thread shape locked in ADR-11 #7: each thread carries
//! its own `Vm`; mcache + persistent indexes are shared.
//!
//! Phase 11 ships single-threaded with one global `Vm` instance behind
//! `current()`. When the transactor lands, `current()` becomes
//! thread-local and the rest of the codebase needs no changes.

const std = @import("std");
const ref_mod = @import("ref.zig");

pub const Ref = ref_mod.Ref;

/// Per-thread VM state. Forward-compat field set per ADR-11 #2 + #7.
pub const Vm = struct {
    // -- Phase 11 bytecode VM (stub fields, wired in 11.3) ------------------
    /// Active bytecode frame stack base; null until 11.3 lands.
    frames: ?*anyopaque = null,
    /// Program counter into current frame's bytecode.
    pc: usize = 0,
    /// Bytecode bytes for the current frame; null until 11.3 lands.
    bytecode: ?[*]const u8 = null,
    /// Receiver of the currently-executing method.
    current_self: Ref = ref_mod.NIL,
    /// Non-local return flag. Per-thread by construction
    /// (ADR-11 addendum #5 retires `lang.zig` g_unwinding via
    /// RETURN_FROM_METHOD opcode at 11.5).
    unwinding: bool = false,
    /// Bump arena slot for transient per-thread allocations.
    bump_arena_slot: ?*anyopaque = null,

    // -- Live state migrated from kernel globals ---------------------------
    /// Recursion guard for `:bind` override (Phase 7.5). Was `g_recursion`
    /// in send.zig.
    recursion_guard: u32 = 0,
    /// FFI pause counter; while > 0 promotion is blocked so FFI callers
    /// observe stable materialized struct addresses. Was `g_ffi_depth`
    /// in promotion.zig.
    ffi_pause: u32 = 0,
};

/// Single global Vm — phase 11 ships single-threaded. Replace with
/// thread-local storage when the Datomic transactor lands.
var g_current_vm: Vm = .{};

/// Accessor for the current thread's VM state.
pub inline fn current() *Vm {
    return &g_current_vm;
}

/// Reset all per-Vm state to defaults. Called from `initDispatch` and
/// `promotion.init` so test boundaries see a clean slate.
pub fn reset() void {
    g_current_vm = .{};
}

// -- Phase 11 bytecode opcodes -----------------------------------------------

/// Opcode set locked in ADR-11 addendum. Each value = single byte in the
/// `:bytecode-stream`. 16 ops in use; 240 codes reserved for future
/// growth (bytecode-format-version bumps in the image manifest gate
/// breaking opcode-set changes per Phase 12.1).
pub const Opcode = enum(u8) {
    /// `[op u16:pool-idx]` — push pool[idx] onto value stack. Pool holds
    /// number/string literal entities, block-method Refs, symbol entities.
    PUSH_CONST = 0,
    /// `[op i32]` — push tagged SmallInt; skips pool for inline ints.
    PUSH_SMALLINT_I32 = 1,
    /// `[op u8:slot]` — push frame-local at slot.
    PUSH_LOCAL = 2,
    /// `[op u8:slot]` — pop value-stack top into frame-local at slot.
    STORE_LOCAL = 3,
    /// `[op]` — discard top of value stack.
    POP = 4,
    /// `[op]` — push current frame's self_ref.
    LOAD_SELF = 5,
    /// `[op]` — push TRUE_EID.
    LOAD_CONST_TRUE = 6,
    /// `[op]` — push FALSE_EID.
    LOAD_CONST_FALSE = 7,
    /// `[op]` — push NIL.
    LOAD_CONST_NIL = 8,
    /// `[op u32:sel-eid u16:site-idx]` — unary send (no args).
    SEND_0 = 9,
    /// `[op u32:sel-eid u16:site-idx]` — binary send (1 arg).
    SEND_1 = 10,
    /// `[op u32:sel-eid u8:arg-count u16:site-idx]` — keyword/n-ary send.
    SEND_N = 11,
    /// `[op]` — return value-stack top from current frame.
    RETURN = 12,
    /// `[op u8:method-frame-depth]` — non-local return; unwinds VM stack
    /// to the named method frame (block frame chain → enclosing method).
    /// Escaped block (no matching frame on stack) emits ERROR_VT entity.
    RETURN_FROM_METHOD = 13,
    /// `[op i16:offset]` — unconditional pc-relative jump.
    JUMP = 14,
    /// `[op i16:offset]` — pop, jump if value is FALSE_EID or NIL.
    BRANCH_IF_FALSE = 15,
    /// `[op u16:pool-idx]` — pool slot holds block-method-ref; capture
    /// current scope-ref, allocate Block obj, push Ref onto value stack.
    MAKE_BLOCK = 16,
    /// `[op u8:arg-count u16:site-idx]` — invoke top-of-stack Block obj.
    INVOKE_BLOCK = 17,
};

/// Decoded operand size per opcode (bytes following the opcode byte).
/// Source of truth for instruction stepping. Drives the build-time
/// tail-call probe + reify-stack pc walk.
pub fn operandSize(op: Opcode) usize {
    return switch (op) {
        .POP, .LOAD_SELF, .LOAD_CONST_TRUE, .LOAD_CONST_FALSE, .LOAD_CONST_NIL, .RETURN => 0,
        .PUSH_LOCAL, .STORE_LOCAL, .RETURN_FROM_METHOD => 1,
        .PUSH_CONST, .MAKE_BLOCK, .JUMP, .BRANCH_IF_FALSE => 2,
        .INVOKE_BLOCK => 3,
        .PUSH_SMALLINT_I32 => 4,
        .SEND_0, .SEND_1 => 6,
        .SEND_N => 7,
    };
}

/// Total instruction size including opcode byte.
pub fn instructionSize(op: Opcode) usize {
    return 1 + operandSize(op);
}

