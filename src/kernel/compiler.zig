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
        else => return error.UnknownAstKind,
    }
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
