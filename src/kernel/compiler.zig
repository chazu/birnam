//! Phase 11.2 — AST → bytecode compiler.
//!
//! Spike implementation in Zig. Self-hosting in birnam itself is the
//! VISION §296 trajectory (ROADMAP X6) but not committed to a phase.
//! Until then, Zig-side compiler living in the kernel layer is the
//! pragmatic path.
//!
//! Input: AST root entity from the lang module's parser.
//! Output: CompiledMethod carrying the bytecode bytestream, constant
//! pool of Refs, and the structural annotations the VM dispatch loop
//! plus mark-sweep walker rely on.
//!
//! Phase A1 scope: literals (smallint + boxed literal entities) + RETURN.
//! Subsequent compiler phases extend: stmt-seq, var-refs, sends, blocks,
//! control flow, non-local return.

const std = @import("std");
const fact_store_mod = @import("fact_store.zig");
const object_table_mod = @import("object_table.zig");
const ref_mod = @import("ref.zig");
const kernel_eids = @import("kernel_eids.zig");
const bytecode = @import("bytecode.zig");
const strings = @import("strings.zig");
const boxed = @import("boxed.zig");

pub const Ref = ref_mod.Ref;
pub const FactStore = fact_store_mod.FactStore;
pub const ObjectTable = object_table_mod.ObjectTable;
pub const Opcode = bytecode.Opcode;

pub const CompileError = error{
    UnknownAstKind,
    AstMissingValue,
    OutOfMemory,
};

/// Output of a single-method compile. Caller owns the byte slice +
/// constant pool storage and is responsible for materializing them as
/// fact-store-attached Refs before discarding the struct.
pub const CompiledMethod = struct {
    bytecode: []u8,
    constant_pool: []Ref,
    method_body_ast: u64,
    arg_count: u8,
    local_count: u8,
    send_site_count: u16,

    pub fn deinit(self: *CompiledMethod, allocator: std.mem.Allocator) void {
        allocator.free(self.bytecode);
        allocator.free(self.constant_pool);
        self.bytecode = &[_]u8{};
        self.constant_pool = &[_]Ref{};
    }
};

/// Per-compile mutable state. One CompilerState per method (or block-
/// method) being compiled.
const State = struct {
    allocator: std.mem.Allocator,
    store: *const FactStore,
    encoder: bytecode.Encoder,
    /// Growing pool of Refs referenced by PUSH_CONST.
    constant_pool: std.ArrayList(Ref),
    send_site_count: u16 = 0,
    /// Highest local slot index seen. Final local_count = highest + 1.
    /// -1 sentinel = no locals seen yet.
    max_local_slot: i32 = -1,

    fn deinit(self: *State) void {
        self.encoder.deinit(self.allocator);
        self.constant_pool.deinit(self.allocator);
    }

    /// Add a Ref to the constant pool and return its index. Deduplicates
    /// by linear scan — pool sizes are small (typical method has < 16
    /// non-smallint literals).
    fn poolIndexFor(self: *State, r: Ref) !u16 {
        for (self.constant_pool.items, 0..) |existing, i| {
            if (existing == r) return @intCast(i);
        }
        const idx: usize = self.constant_pool.items.len;
        if (idx > std.math.maxInt(u16)) return error.OutOfMemory;
        try self.constant_pool.append(self.allocator, r);
        return @intCast(idx);
    }
};

/// Compile a single AST node as a top-level method body. Wraps the
/// emitted expression bytes in an implicit RETURN.
pub fn compileMethod(
    allocator: std.mem.Allocator,
    store: *const FactStore,
    ast_root_eid: u64,
) CompileError!CompiledMethod {
    var state = State{
        .allocator = allocator,
        .store = store,
        .encoder = bytecode.Encoder.init(allocator),
        .constant_pool = std.ArrayList(Ref){},
    };
    errdefer state.deinit();

    try compileExpr(&state, ast_root_eid);
    state.encoder.ret(allocator) catch return error.OutOfMemory;

    const bytes = state.encoder.toOwnedSlice(allocator) catch return error.OutOfMemory;
    const pool = state.constant_pool.toOwnedSlice(allocator) catch return error.OutOfMemory;

    const local_count: u8 = if (state.max_local_slot < 0) 0 else @intCast(state.max_local_slot + 1);

    return .{
        .bytecode = bytes,
        .constant_pool = pool,
        .method_body_ast = ast_root_eid,
        .arg_count = 0,
        .local_count = local_count,
        .send_site_count = state.send_site_count,
    };
}

/// Compile an expression. Leaves one value on the value-stack top.
fn compileExpr(state: *State, ast_eid: u64) CompileError!void {
    const vt_ref = state.store.currentValue(ast_eid, kernel_eids.ATTR_VTABLE) orelse
        return error.UnknownAstKind;
    if (!ref_mod.isEid(vt_ref)) return error.UnknownAstKind;
    const vt = ref_mod.untagEid(vt_ref);

    switch (vt) {
        kernel_eids.AST_LITERAL_VT => try compileLiteral(state, ast_eid),
        kernel_eids.AST_STMT_SEQ_VT => try compileStmtSeq(state, ast_eid),
        kernel_eids.AST_RETURN_VT => try compileReturn(state, ast_eid),
        kernel_eids.AST_SELF_VT => state.encoder.loadSelf(state.allocator) catch return error.OutOfMemory,
        kernel_eids.AST_UNARY_SEND_VT => try compileSend(state, ast_eid, 0),
        kernel_eids.AST_BINARY_SEND_VT => try compileSend(state, ast_eid, 1),
        kernel_eids.AST_KEYWORD_SEND_VT => try compileKeywordSend(state, ast_eid),
        else => return error.UnknownAstKind,
    }
}

/// Compile a unary or binary send. n_args is 0 or 1.
fn compileSend(state: *State, ast_eid: u64, n_args: u8) CompileError!void {
    try compileReceiver(state, ast_eid);

    if (n_args > 0) {
        const first_arg_ref = state.store.currentValue(ast_eid, kernel_eids.ATTR_AST_FIRST_ARG) orelse
            return error.UnknownAstKind;
        if (!ref_mod.isEid(first_arg_ref)) return error.UnknownAstKind;
        try compileExpr(state, ref_mod.untagEid(first_arg_ref));
    }

    const sel_eid = try readSelectorEid(state, ast_eid);
    const site_idx = try bumpSendSite(state);

    if (n_args == 0) {
        state.encoder.send0(state.allocator, @intCast(sel_eid), site_idx) catch return error.OutOfMemory;
    } else {
        state.encoder.send1(state.allocator, @intCast(sel_eid), site_idx) catch return error.OutOfMemory;
    }
}

/// Compile a keyword send. Arg count = length of :ast-first-arg /
/// :ast-next-sibling chain. SEND_N opcode carries arg-count operand.
fn compileKeywordSend(state: *State, ast_eid: u64) CompileError!void {
    try compileReceiver(state, ast_eid);

    const first_arg_ref = state.store.currentValue(ast_eid, kernel_eids.ATTR_AST_FIRST_ARG) orelse
        return error.UnknownAstKind;
    if (!ref_mod.isEid(first_arg_ref)) return error.UnknownAstKind;

    var n_args: u8 = 0;
    var cur: u64 = first_arg_ref;
    while (true) {
        if (!ref_mod.isEid(cur)) return error.UnknownAstKind;
        const cur_eid = ref_mod.untagEid(cur);
        try compileExpr(state, cur_eid);
        if (n_args == std.math.maxInt(u8)) return error.OutOfMemory;
        n_args += 1;
        const next_ref = state.store.currentValue(cur_eid, kernel_eids.ATTR_AST_NEXT_SIBLING);
        if (next_ref == null) break;
        cur = next_ref.?;
    }

    const sel_eid = try readSelectorEid(state, ast_eid);
    const site_idx = try bumpSendSite(state);
    state.encoder.sendN(state.allocator, @intCast(sel_eid), n_args, site_idx) catch return error.OutOfMemory;
}

fn compileReceiver(state: *State, send_ast_eid: u64) CompileError!void {
    const recv_ref = state.store.currentValue(send_ast_eid, kernel_eids.ATTR_AST_RECEIVER) orelse
        return error.UnknownAstKind;
    if (!ref_mod.isEid(recv_ref)) return error.UnknownAstKind;
    try compileExpr(state, ref_mod.untagEid(recv_ref));
}

fn readSelectorEid(state: *State, send_ast_eid: u64) CompileError!u32 {
    const sel_ref = state.store.currentValue(send_ast_eid, kernel_eids.ATTR_AST_SELECTOR) orelse
        return error.UnknownAstKind;
    if (!ref_mod.isEid(sel_ref)) return error.UnknownAstKind;
    const sel_eid = ref_mod.untagEid(sel_ref);
    if (sel_eid > std.math.maxInt(u32)) return error.OutOfMemory;
    return @intCast(sel_eid);
}

fn bumpSendSite(state: *State) CompileError!u16 {
    const idx = state.send_site_count;
    if (idx == std.math.maxInt(u16)) return error.OutOfMemory;
    state.send_site_count += 1;
    return idx;
}

// -- Materialization (Phase 11.8 — annotation emit) ------------------------

pub const MaterializeError = error{
    OutOfMemory,
    StoreError,
};

/// Persist a CompiledMethod into the fact store as a method entity.
/// Returns the minted method EID.
///
/// Asserted facts (all under `module_eid` for provenance):
///   [m :vtable CLOSURE_VT]
///   [m :bytecode-stream <string-Ref>]
///   [m :constant-pool <rrb-Ref>]                  (only if pool non-empty)
///   [m :method-body-ast <ast-eid-Ref>]
///   [m :method-arg-count <SmallInt>]
///   [m :method-local-count <SmallInt>]
///   [m :method-send-site-count <SmallInt>]
///
/// The CompiledMethod's owned byte slice and pool slice are duplicated
/// into `allocator` via `strings.mkString` / `boxed.mkRrb` (which build
/// long-lived heap copies). Caller can deinit the CompiledMethod
/// immediately after materialize returns.
pub fn materialize(
    allocator: std.mem.Allocator,
    store: *FactStore,
    table: *ObjectTable,
    m: *const CompiledMethod,
    module_eid: u64,
) MaterializeError!u64 {
    const method_eid = table.alloc();

    const bytes_ref = strings.mkString(allocator, m.bytecode) catch return error.OutOfMemory;
    const pool_ref: ?Ref = if (m.constant_pool.len == 0)
        null
    else
        boxed.mkRrb(allocator, m.constant_pool) catch return error.OutOfMemory;

    storeAssert(store, method_eid, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.CLOSURE_VT), module_eid) catch return error.StoreError;
    storeAssert(store, method_eid, kernel_eids.ATTR_BYTECODE_STREAM, bytes_ref, module_eid) catch return error.StoreError;
    if (pool_ref) |p| {
        storeAssert(store, method_eid, kernel_eids.ATTR_CONSTANT_POOL, p, module_eid) catch return error.StoreError;
    }
    storeAssert(store, method_eid, kernel_eids.ATTR_METHOD_BODY_AST, ref_mod.mkEid(m.method_body_ast), module_eid) catch return error.StoreError;
    storeAssertSmallInt(store, method_eid, kernel_eids.ATTR_METHOD_ARG_COUNT, m.arg_count, module_eid) catch return error.StoreError;
    storeAssertSmallInt(store, method_eid, kernel_eids.ATTR_METHOD_LOCAL_COUNT, m.local_count, module_eid) catch return error.StoreError;
    storeAssertSmallInt(store, method_eid, kernel_eids.ATTR_METHOD_SEND_SITE_COUNT, m.send_site_count, module_eid) catch return error.StoreError;

    return method_eid;
}

inline fn storeAssert(store: *FactStore, e: u64, a: u64, v: Ref, mod_eid: u64) !void {
    _ = try store.assert(e, a, v, mod_eid);
}

inline fn storeAssertSmallInt(store: *FactStore, e: u64, a: u64, n: u16, mod_eid: u64) !void {
    const r = ref_mod.mkSmallInt(@as(i64, n)) orelse return error.OutOfMemory;
    _ = try store.assert(e, a, r, mod_eid);
}

/// Compile a statement sequence. Each non-last statement's value is
/// popped; the last statement's value remains on stack as the seq's
/// result. Empty seq pushes nil.
fn compileStmtSeq(state: *State, ast_eid: u64) CompileError!void {
    const first = state.store.currentValue(ast_eid, kernel_eids.ATTR_AST_STMT_FIRST);
    if (first == null) {
        state.encoder.loadNil(state.allocator) catch return error.OutOfMemory;
        return;
    }

    var cur = first.?;
    while (true) {
        if (!ref_mod.isEid(cur)) return error.UnknownAstKind;
        const cur_eid = ref_mod.untagEid(cur);

        const next_ref = state.store.currentValue(cur_eid, kernel_eids.ATTR_AST_NEXT_SIBLING);
        try compileExpr(state, cur_eid);

        if (next_ref) |nx| {
            // Not the last stmt — drop value before next iter.
            state.encoder.pop(state.allocator) catch return error.OutOfMemory;
            cur = nx;
        } else break;
    }
}

/// Compile `^expr`. Phase A2 lowers to plain RETURN inside a method
/// body. Non-local return from inside a block is wired in Phase A6
/// (RETURN_FROM_METHOD opcode).
fn compileReturn(state: *State, ast_eid: u64) CompileError!void {
    const value_ref = state.store.currentValue(ast_eid, kernel_eids.ATTR_AST_RETURN_VALUE) orelse
        return error.AstMissingValue;
    if (!ref_mod.isEid(value_ref)) return error.UnknownAstKind;
    try compileExpr(state, ref_mod.untagEid(value_ref));
    state.encoder.ret(state.allocator) catch return error.OutOfMemory;
}

/// Emit code to push the literal entity's value. SmallInt literals lower
/// to PUSH_SMALLINT_I32 if they fit i32; otherwise (including boxed
/// floats / strings / nil / true / false) go through the constant pool.
fn compileLiteral(state: *State, ast_eid: u64) CompileError!void {
    // Absence of :ast-value means nil literal. Date-discipline forbids
    // NIL as a fact value, so the parser cannot encode nil as a stored
    // value — it omits the attribute instead.
    const value = state.store.currentValue(ast_eid, kernel_eids.ATTR_AST_VALUE) orelse {
        state.encoder.loadNil(state.allocator) catch return error.OutOfMemory;
        return;
    };

    // Dedicated true / false opcodes save a pool entry. NIL handled by
    // the absence-of-:ast-value path above.
    if (ref_mod.isEid(value)) {
        const v_eid = ref_mod.untagEid(value);
        if (v_eid == kernel_eids.TRUE_EID) {
            state.encoder.loadTrue(state.allocator) catch return error.OutOfMemory;
            return;
        }
        if (v_eid == kernel_eids.FALSE_EID) {
            state.encoder.loadFalse(state.allocator) catch return error.OutOfMemory;
            return;
        }
    }

    if (ref_mod.isSmallInt(value)) {
        const n = ref_mod.untagSmallInt(value);
        if (n >= std.math.minInt(i32) and n <= std.math.maxInt(i32)) {
            state.encoder.pushSmallInt(state.allocator, @intCast(n)) catch return error.OutOfMemory;
            return;
        }
    }

    const idx = state.poolIndexFor(value) catch return error.OutOfMemory;
    state.encoder.pushConst(state.allocator, idx) catch return error.OutOfMemory;
}
