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
const arenas = @import("arenas.zig");

pub const Ref = ref_mod.Ref;

// -- Phase 11.3 — frame + value stack ---------------------------------------

/// Per-activation frame. Phase 11.3 (ADR-11 #2). One per active method or
/// block invocation. Frames live in the Vm's bump arena; slice into the
/// shared value stack via `bp`.
pub const Frame = extern struct {
    /// Pointer to current method's `:bytecode-stream` materialized bytes.
    /// Null only for the synthetic sentinel frame at stack base.
    bytecode: ?[*]const u8,
    /// Program counter into `bytecode`.
    pc: usize,
    /// EID of the method (or block-method) currently executing.
    method_eid: u64,
    /// Receiver of this activation.
    self_ref: Ref,
    /// Base pointer into Vm.value_stack: locals[0..local_count] follow.
    bp: usize,
    /// Number of locals + args reserved on the value stack.
    local_count: u8,
    /// Padding to keep struct size a multiple of 8.
    _pad: [7]u8 = [_]u8{0} ** 7,
    /// For block-method frames: pointer to enclosing method frame for
    /// RETURN_FROM_METHOD unwind (ADR-11 addendum #1). Null for method
    /// frames (they ARE the unwind target).
    parent_method_frame: ?*Frame,
    /// Monotonic frame id (Phase 11.12) for non-local-return targeting.
    frame_id: u64 = 0,
};

/// Initial value-stack capacity. Grows via reallocation when exceeded.
/// Bytecode bound at compile time via `:method-local-count` — never
/// recurses unboundedly.
pub const DEFAULT_VALUE_STACK_CAP: usize = 4096;

/// Maximum frame depth before forced ERROR_VT emission. Mirrors the
/// tree-walker's 8192 limit (ADR-10-E5) until the bytecode VM proves
/// stable enough to remove it.
pub const FRAME_STACK_LIMIT: usize = 8192;

// -- Phase 11.4 — per-method method cache (mcache) --------------------------

/// 16-byte aligned cache entry. ADR-11 #3: writer does a single 16-byte
/// atomic store (`movdqa` on x86_64 / `stp` paired on arm64). Reader uses
/// the 3-load re-check-vt pattern from ADR-11 addendum #5; the writer
/// atomicity is defensive — torn writes on weaker arches still get
/// caught by `vt1 != vt2` in the reader.
pub const MCEntry = extern struct {
    /// Receiver vtable EID. Zero = empty slot.
    vt: u64 align(16),
    /// Resolved method Imp as opaque pointer. Caller casts to the right
    /// Imp signature at the call site (same convention as Closure.method).
    method_impl: ?*const anyopaque,
};

comptime {
    if (@sizeOf(MCEntry) != 16) @compileError("MCEntry must be 16 bytes for atomic 128-bit store");
    if (@alignOf(MCEntry) < 16) @compileError("MCEntry must be 16-byte aligned");
}

/// Per-method method cache. Sized at first execution from the method's
/// `:method-send-site-count` annotation. Lazy-allocated in caller's
/// allocator (per-thread bump arena under the Datomic shape).
pub const MethodMCache = struct {
    entries: []align(16) MCEntry,

    pub fn alloc(allocator: std.mem.Allocator, site_count: usize) !MethodMCache {
        const entries = try allocator.alignedAlloc(MCEntry, .@"16", site_count);
        @memset(entries, MCEntry{ .vt = 0, .method_impl = null });
        return .{ .entries = entries };
    }

    pub fn deinit(self: *MethodMCache, allocator: std.mem.Allocator) void {
        allocator.free(self.entries);
        self.entries = &[_]MCEntry{};
    }

    /// 3-load re-check-vt reader (ADR-11 addendum #5). Returns method Imp
    /// on hit (vt1 == receiver_vt and vt1 == vt2). Mismatch — either
    /// genuine vt mismatch or a torn write — returns null; caller falls
    /// through to miss handler (bind).
    pub inline fn lookup(self: *const MethodMCache, site_idx: usize, receiver_vt: u64) ?*const anyopaque {
        if (site_idx >= self.entries.len) return null;
        const e = &self.entries[site_idx];
        const vt1 = @atomicLoad(u64, &e.vt, .monotonic);
        if (vt1 != receiver_vt) return null;
        const impl_ptr: ?*const anyopaque = @atomicLoad(?*const anyopaque, &e.method_impl, .monotonic);
        const vt2 = @atomicLoad(u64, &e.vt, .monotonic);
        if (vt1 != vt2) return null;
        return impl_ptr;
    }

    /// Defensive 16-byte atomic store. Pairs with the reader's re-check
    /// pattern: if this store tears (arm64 pre-8.1 `stp`), the reader
    /// catches vt1 != vt2 and falls through.
    pub inline fn store(self: *MethodMCache, site_idx: usize, vt: u64, impl: *const anyopaque) void {
        if (site_idx >= self.entries.len) return;
        const e = &self.entries[site_idx];
        const packed_value = packEntry(vt, impl);
        const raw: *align(16) u128 = @ptrCast(@alignCast(e));
        @atomicStore(u128, raw, packed_value, .release);
    }

    /// Walk-on-install invalidation (ADR-11 addendum #5). Called by the
    /// method-install hook for every live MethodMCache when a method's
    /// Imp is replaced. Entries pointing at the old Imp are atomic-zeroed.
    /// Methods currently executing on the stack keep their old Imp until
    /// RETURN (Smalltalk-80 semantics).
    pub fn invalidateImpl(self: *MethodMCache, old_impl: *const anyopaque) void {
        for (self.entries, 0..) |*e, i| {
            if (e.method_impl == old_impl) {
                const raw: *align(16) u128 = @ptrCast(@alignCast(e));
                @atomicStore(u128, raw, 0, .release);
                _ = i;
            }
        }
    }
};

fn packEntry(vt: u64, impl: *const anyopaque) u128 {
    const impl_bits: u64 = @intFromPtr(impl);
    return @as(u128, vt) | (@as(u128, impl_bits) << 64);
}

// -- Phase 11.11 — closure scope capture ---------------------------------

/// Mapping from a parent-frame local slot to this method's own local
/// slot. Recorded by the compiler each time an inner block references
/// a name declared in an enclosing block.
pub const CaptureSpec = extern struct {
    parent_slot: u8,
    local_slot: u8,
    _pad: u16 = 0,
};

/// Runtime layout for the heap-allocated capture-spec array carried by
/// `:method-captures-ptr`. Header is followed inline by `len` CaptureSpec
/// records.
pub const CaptureSpecArrayHeader = extern struct {
    len: u32,
    _pad: u32 = 0,
};

/// Heap-allocated payload referenced by a bytecode-block closure's
/// `closure.data`. Carries everything `bytecodeDispatcherImpl` needs to
/// invoke the block: which method to run, what `self` should be inside
/// the block (the enclosing method's receiver — Smalltalk semantics),
/// and the captured outer-scope values copied at MAKE_BLOCK time.
///
/// Phase 11.12 — parent_self_ref + parent_frame_id make `[block value]`
/// from arbitrary tree-walker or bytecode contexts dispatch with the
/// correct self and a frame target for non-local return.
pub const BytecodeClosure = extern struct {
    method_eid: u64,
    /// `self` inside the block body — the enclosing method's receiver.
    parent_self_ref: Ref,
    /// Unique monotonic identifier of the enclosing frame at MAKE_BLOCK
    /// time. Used by RETURN_FROM_METHOD to identify the target frame
    /// even after intermediate frames pop and push. 0 = no target
    /// (block doesn't escape).
    parent_frame_id: u64,
    captures_len: u32,
    _pad: u32 = 0,
    // [captures_len]Ref follows inline
};

/// Back-compat alias — keeps existing callers of vm.BytecodeBlock
/// (current dispatch tests) building during the Phase 11.12 transition.
pub const BytecodeBlock = BytecodeClosure;

/// Decode a capture-spec pointer Ref into the spec slice.
pub fn captureSpecsFromRef(ptr_ref: Ref) []const CaptureSpec {
    if (!ref_mod.isPointer(ptr_ref)) return &[_]CaptureSpec{};
    const base_ptr = ref_mod.untagPointer(ptr_ref);
    const header: *const CaptureSpecArrayHeader = @ptrCast(@alignCast(base_ptr));
    const tail_addr: usize = @intFromPtr(base_ptr) + @sizeOf(CaptureSpecArrayHeader);
    const tail: [*]const CaptureSpec = @ptrFromInt(tail_addr);
    return tail[0..header.len];
}

/// Capture values out of a BytecodeClosure pointer Ref.
pub fn blockCapturesFromRef(ptr_ref: Ref) []const Ref {
    if (!ref_mod.isPointer(ptr_ref)) return &[_]Ref{};
    const base_ptr = ref_mod.untagPointer(ptr_ref);
    const header: *const BytecodeClosure = @ptrCast(@alignCast(base_ptr));
    const tail_addr: usize = @intFromPtr(base_ptr) + @sizeOf(BytecodeClosure);
    const tail: [*]const Ref = @ptrFromInt(tail_addr);
    return tail[0..header.captures_len];
}

/// Decode a BytecodeClosure pointer Ref → its method EID.
pub fn blockMethodEidFromRef(ptr_ref: Ref) u64 {
    if (!ref_mod.isPointer(ptr_ref)) return 0;
    const header: *const BytecodeClosure = @ptrCast(@alignCast(ref_mod.untagPointer(ptr_ref)));
    return header.method_eid;
}

/// Decode a BytecodeClosure pointer Ref → its parent (enclosing-method)
/// self_ref. Used by `bytecodeDispatcherImpl` as the block frame's
/// `self_ref` so the Smalltalk-semantic `self` inside a block matches
/// the enclosing method's receiver.
pub fn blockParentSelfFromRef(ptr_ref: Ref) Ref {
    if (!ref_mod.isPointer(ptr_ref)) return ref_mod.NIL;
    const header: *const BytecodeClosure = @ptrCast(@alignCast(ref_mod.untagPointer(ptr_ref)));
    return header.parent_self_ref;
}

/// Decode a BytecodeClosure pointer Ref → parent_frame_id (the frame
/// to unwind to on RETURN_FROM_METHOD). 0 = no target.
pub fn blockParentFrameIdFromRef(ptr_ref: Ref) u64 {
    if (!ref_mod.isPointer(ptr_ref)) return 0;
    const header: *const BytecodeClosure = @ptrCast(@alignCast(ref_mod.untagPointer(ptr_ref)));
    return header.parent_frame_id;
}

/// Allocate a BytecodeClosure + inline captures. Returns the
/// pointer Ref plus a writable tail handle for the caller to copy
/// captured values into.
pub fn allocBytecodeBlock(
    allocator: std.mem.Allocator,
    method_eid: u64,
    parent_self_ref: Ref,
    parent_frame_id: u64,
    n_captures: u32,
) !struct { ref: Ref, captures_tail: [*]Ref } {
    const total = @sizeOf(BytecodeClosure) + @sizeOf(Ref) * n_captures;
    // Phase 15.1b/15.3: route to the captures arena (live) so the collector
    // can conservatively scan closure capture tails for boxed Refs. Fallback
    // to the passed allocator in bare unit tests (no registry).
    const ba = if (arenas.ready()) arenas.captures() else allocator;
    const buf = try ba.alignedAlloc(u8, .of(BytecodeClosure), total);
    const header: *BytecodeClosure = @ptrCast(@alignCast(buf.ptr));
    header.* = .{
        .method_eid = method_eid,
        .parent_self_ref = parent_self_ref,
        .parent_frame_id = parent_frame_id,
        .captures_len = n_captures,
    };
    const tail_addr: usize = @intFromPtr(buf.ptr) + @sizeOf(BytecodeClosure);
    const tail: [*]Ref = @ptrFromInt(tail_addr);
    return .{ .ref = ref_mod.mkPointer(@ptrCast(buf.ptr)), .captures_tail = tail };
}

/// Per-thread VM state. Forward-compat field set per ADR-11 #2 + #7.
pub const Vm = struct {
    // -- Phase 11 bytecode VM ----------------------------------------------
    /// Active bytecode frame stack. Pushed/popped by dispatch.runMethod
    /// around every invocation (top-level and INVOKE_BLOCK). Capacity is
    /// reserved upfront via `ensureFrameCapacity` so pointers into the
    /// list stay stable for the current dispatch loop's pc write-through.
    frames: std.ArrayList(Frame) = .{},
    /// Program counter into current frame's bytecode.
    pc: usize = 0,
    /// Bytecode bytes for the current frame; null until 11.3 lands.
    bytecode: ?[*]const u8 = null,
    /// Receiver of the currently-executing method.
    current_self: Ref = ref_mod.NIL,
    /// Non-local return flag. Per-thread by construction
    /// (ADR-11 addendum #5 retires `lang.zig` g_unwinding via
    /// RETURN_FROM_METHOD opcode at 11.5). When set, the active
    /// dispatch loop exits early; the calling block-frame's
    /// dispatcher decrements `unwind_target_remaining` and either
    /// catches (depth = 0 → clear flag, value becomes result) or
    /// propagates (depth > 0 → re-exit).
    unwinding: bool = false,
    /// Value carried by an active non-local return.
    unwind_value: Ref = ref_mod.NIL,
    /// Number of additional frame pops remaining before the unwind
    /// target is reached. RETURN_FROM_METHOD opcode loads its
    /// method-frame-depth operand into this field. Each enclosing
    /// block frame decrements on catch.
    unwind_target_remaining: u8 = 0,
    /// Bump arena slot for transient per-thread allocations.
    bump_arena_slot: ?*anyopaque = null,
    /// Monotonic frame-id counter — incremented on every runMethod push.
    /// Stored on each Frame so RETURN_FROM_METHOD can identify the
    /// target frame via stable id even if the frame stack churns.
    next_frame_id: u64 = 1,

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
/// `promotion.init` so test boundaries see a clean slate. Leaks any
/// frames currently allocated — callers are expected to have unwound
/// the frame stack before reset (i.e., reset is a clean-slate signal,
/// not a panic-cleanup).
pub fn reset() void {
    g_current_vm.frames = .{};
    g_current_vm.recursion_guard = 0;
    g_current_vm.ffi_pause = 0;
    g_current_vm.unwinding = false;
    g_current_vm.unwind_value = ref_mod.NIL;
    g_current_vm.unwind_target_remaining = 0;
    g_current_vm.pc = 0;
    g_current_vm.bytecode = null;
    g_current_vm.current_self = ref_mod.NIL;
    g_current_vm.bump_arena_slot = null;
    g_current_vm.next_frame_id = 1;
}

/// Reserve frame-stack capacity so subsequent appends never realloc
/// during a dispatch loop. Idempotent — repeated calls only grow.
pub fn ensureFrameCapacity(allocator: std.mem.Allocator, n: usize) !void {
    try g_current_vm.frames.ensureTotalCapacity(allocator, n);
}

/// Free any frame storage. Tests call this in tear-down.
pub fn freeFrames(allocator: std.mem.Allocator) void {
    g_current_vm.frames.deinit(allocator);
    g_current_vm.frames = .{};
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
    /// `[op i16:offset]` — peek TOS; if it is an ERROR_VT value, jump (leaving
    /// the error on the stack), else pop and fall through. Emitted between the
    /// statements of a sequence so a compiled `a. b.` halts on `a`'s error
    /// instead of running `b` (Phase 17.7 finish).
    BRANCH_IF_ERROR = 18,
};

/// Decoded operand size per opcode (bytes following the opcode byte).
/// Source of truth for instruction stepping. Drives the build-time
/// tail-call probe + reify-stack pc walk.
pub fn operandSize(op: Opcode) usize {
    return switch (op) {
        .POP, .LOAD_SELF, .LOAD_CONST_TRUE, .LOAD_CONST_FALSE, .LOAD_CONST_NIL, .RETURN => 0,
        .PUSH_LOCAL, .STORE_LOCAL, .RETURN_FROM_METHOD => 1,
        .PUSH_CONST, .MAKE_BLOCK, .JUMP, .BRANCH_IF_FALSE, .BRANCH_IF_ERROR => 2,
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

