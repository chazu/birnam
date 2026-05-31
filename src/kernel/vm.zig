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
