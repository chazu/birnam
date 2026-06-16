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
const closure_mod = @import("closure.zig");
const dispatch = @import("dispatch.zig");
const vm_mod = @import("vm.zig");
const arenas = @import("arenas.zig");

pub const Ref = ref_mod.Ref;
pub const FactStore = fact_store_mod.FactStore;
pub const ObjectTable = object_table_mod.ObjectTable;
pub const Opcode = bytecode.Opcode;

pub const CompileError = error{
    UnknownAstKind,
    AstMissingValue,
    VarNotInScope,
    TooManyLocals,
    OutOfMemory,
};

/// Top-level compilation environment (Cluster ①, 2026-06-15). When set,
/// a free variable (no frame slot / capture) and an outermost-level
/// `let`/`:=` lower to message sends against a persistent scope entity
/// instead of failing with `VarNotInScope` — so REPL/script top-level
/// statements compile to bytecode while keeping cross-line binding
/// visibility and global access. The scope semantics live in the lang
/// module's SCOPE_VT methods; the compiler only emits the sends, using
/// the selector eids the lang module supplies here.
pub const TopLevelEnv = struct {
    /// `mkEid(top_scope_eid)` — receiver of the scope sends.
    scope_const: Ref,
    /// `lookup:` selector eid (1 arg: name symbol) → bound value or
    /// an undefined-variable error value.
    lookup_sel: u32,
    /// `bind:value:` selector eid (2 args: name, value) → installs a
    /// fresh binding in the scope, returns the value. (`let`)
    bind_sel: u32,
    /// `set:value:` selector eid (2 args: name, value) → mutates the
    /// nearest existing binding, returns the value (or an error value
    /// if unbound). (`:=`)
    set_sel: u32,
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
    /// Child block-methods compiled inline as part of this method.
    /// Each carries the parent's constant-pool index where its
    /// materialized method-Ref must be written before the parent's
    /// pool is realized. Tree is depth-first.
    block_children: []BlockChild,
    /// Phase 11.11 — variables captured from the enclosing method's
    /// frame at MAKE_BLOCK time. Each spec maps a parent-frame slot
    /// (where the value lives in the enclosing activation) to this
    /// method's local slot (where the value gets copied to at INVOKE_BLOCK).
    captures: []CaptureSpec,

    pub fn deinit(self: *CompiledMethod, allocator: std.mem.Allocator) void {
        for (self.block_children) |*child| child.compiled.deinit(allocator);
        allocator.free(self.block_children);
        allocator.free(self.bytecode);
        allocator.free(self.constant_pool);
        allocator.free(self.captures);
        self.bytecode = &[_]u8{};
        self.constant_pool = &[_]Ref{};
        self.block_children = &[_]BlockChild{};
        self.captures = &[_]CaptureSpec{};
    }
};

pub const CaptureSpec = vm_mod.CaptureSpec;

pub const BlockChild = struct {
    pool_idx: u16,
    compiled: CompiledMethod,
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
    /// Block-method children awaiting materialization. Each child's
    /// `compiled` field is fully populated; `pool_idx` is the parent's
    /// constant-pool slot reserved for the materialized block method's
    /// EID-Ref.
    block_children: std.ArrayList(BlockChild),
    /// Compile-time scope map: name-EID → frame-local slot. Filled by
    /// AST_LET_VT, block params, and capture-emit (Phase 11.11).
    scope: std.AutoHashMap(u64, u8),
    /// Enclosing compile state (block-method-nesting chain). Null for
    /// a top-level method. Phase 11.11 — compileVarRef walks this chain
    /// when the name misses in current scope, recording a capture-spec
    /// at each intermediate level so the chain stays flat at runtime
    /// (each block frame only ever reads from its immediate parent).
    parent_state: ?*State = null,
    /// Capture specs for this method (parent-slot → my-local-slot).
    /// Ordered: spec index = position in the runtime captures array.
    captures: std.ArrayList(CaptureSpec),
    /// Cluster ① — set (and threaded into child block states) when this
    /// is a top-level statement compile. Null for ordinary method/block
    /// compiles (a free var then stays `VarNotInScope`).
    top_level: ?*const TopLevelEnv = null,

    fn deinit(self: *State) void {
        self.encoder.deinit(self.allocator);
        self.constant_pool.deinit(self.allocator);
        for (self.block_children.items) |*c| c.compiled.deinit(self.allocator);
        self.block_children.deinit(self.allocator);
        self.scope.deinit();
        self.captures.deinit(self.allocator);
    }

    /// Allocate the next frame-local slot. u8 ceiling matches the
    /// PUSH_LOCAL / STORE_LOCAL operand width.
    fn allocSlot(self: *State) !u8 {
        const next: i32 = self.max_local_slot + 1;
        if (next > std.math.maxInt(u8)) return error.TooManyLocals;
        self.max_local_slot = next;
        return @intCast(next);
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

    /// Reserve a pool slot for a block-method ref. Materialization fills
    /// the slot once the child method's EID is known.
    fn reservePoolSlot(self: *State) !u16 {
        const idx: usize = self.constant_pool.items.len;
        if (idx > std.math.maxInt(u16)) return error.OutOfMemory;
        try self.constant_pool.append(self.allocator, ref_mod.NIL);
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
    return compileMethodInner(allocator, store, ast_root_eid, null, null, null);
}

/// Compile an arbitrary statement AST as a top-level method body in
/// "top-level mode" (Cluster ①). Free variables and outermost `let`/`:=`
/// lower to scope sends per `env`. Run the result with receiver = nil.
pub fn compileTopLevel(
    allocator: std.mem.Allocator,
    store: *const FactStore,
    stmt_ast_eid: u64,
    env: *const TopLevelEnv,
) CompileError!CompiledMethod {
    return compileMethodInner(allocator, store, stmt_ast_eid, null, null, env);
}

/// Compile an AST_BLOCK_VT entity as a top-level method: block params
/// become locals[0..n] and block body becomes the method body. Used by
/// the IdApi entry that lang dylib calls to lower
/// `[vt method:do: block]` installs into bytecode-method installs.
pub fn compileBlockAsMethod(
    allocator: std.mem.Allocator,
    store: *const FactStore,
    block_ast_eid: u64,
) CompileError!CompiledMethod {
    const body_ref = store.currentValue(block_ast_eid, kernel_eids.ATTR_AST_BLOCK_BODY) orelse
        return error.AstMissingValue;
    if (!ref_mod.isEid(body_ref)) return error.UnknownAstKind;
    const body_eid = ref_mod.untagEid(body_ref);

    var first_param: ?u64 = null;
    if (store.currentValue(block_ast_eid, kernel_eids.ATTR_AST_BLOCK_PARAMS)) |p_ref| {
        if (ref_mod.isEid(p_ref)) first_param = ref_mod.untagEid(p_ref);
    }

    return compileMethodInner(allocator, store, body_eid, first_param, null, null);
}

/// Internal variant — supports a pre-populated parameter list for
/// block-method compilation (block params occupy locals[0..n]) and a
/// pointer to the enclosing method's compile state for closure scope
/// capture (Phase 11.11).
fn compileMethodInner(
    allocator: std.mem.Allocator,
    store: *const FactStore,
    ast_root_eid: u64,
    first_param_eid: ?u64,
    parent_state: ?*State,
    top_level: ?*const TopLevelEnv,
) CompileError!CompiledMethod {
    var state = State{
        .allocator = allocator,
        .store = store,
        .encoder = bytecode.Encoder.init(allocator),
        .constant_pool = std.ArrayList(Ref){},
        .block_children = std.ArrayList(BlockChild){},
        .scope = std.AutoHashMap(u64, u8).init(allocator),
        .parent_state = parent_state,
        .captures = std.ArrayList(CaptureSpec){},
        .top_level = top_level,
    };
    // defer (not errdefer) so the scope map is always freed; the
    // owned lists are emptied by toOwnedSlice and become no-op cleanups.
    defer state.deinit();

    // Block params occupy locals[0..n_params] before the body compiles.
    var arg_count: u8 = 0;
    if (first_param_eid) |first| {
        var cur: u64 = first;
        while (true) {
            const name_ref = state.store.currentValue(cur, kernel_eids.ATTR_AST_NAME) orelse
                return error.AstMissingValue;
            if (!ref_mod.isEid(name_ref)) return error.UnknownAstKind;
            const name_eid = ref_mod.untagEid(name_ref);

            const slot = state.allocSlot() catch return error.TooManyLocals;
            state.scope.put(name_eid, slot) catch return error.OutOfMemory;
            arg_count += 1;

            const next_ref = state.store.currentValue(cur, kernel_eids.ATTR_AST_NEXT_SIBLING);
            if (next_ref == null) break;
            if (!ref_mod.isEid(next_ref.?)) return error.UnknownAstKind;
            cur = ref_mod.untagEid(next_ref.?);
        }
    }

    try compileExpr(&state, ast_root_eid);
    state.encoder.ret(allocator) catch return error.OutOfMemory;

    const bytes = state.encoder.toOwnedSlice(allocator) catch return error.OutOfMemory;
    const pool = state.constant_pool.toOwnedSlice(allocator) catch return error.OutOfMemory;
    const children = state.block_children.toOwnedSlice(allocator) catch return error.OutOfMemory;
    const captures = state.captures.toOwnedSlice(allocator) catch return error.OutOfMemory;

    const local_count: u8 = if (state.max_local_slot < 0) 0 else @intCast(state.max_local_slot + 1);

    return .{
        .bytecode = bytes,
        .constant_pool = pool,
        .method_body_ast = ast_root_eid,
        .arg_count = arg_count,
        .local_count = local_count,
        .send_site_count = state.send_site_count,
        .block_children = children,
        .captures = captures,
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
        kernel_eids.AST_BLOCK_VT => try compileBlock(state, ast_eid),
        kernel_eids.AST_LET_VT => try compileLet(state, ast_eid),
        kernel_eids.AST_VAR_REF_VT => try compileVarRef(state, ast_eid),
        kernel_eids.AST_ASSIGN_VT => try compileAssign(state, ast_eid),
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

/// Compile `let x = expr`. Allocates a fresh frame-local slot, stores
/// the init value, and reloads it as the let-expression's result.
fn compileLet(state: *State, ast_eid: u64) CompileError!void {
    const name_eid = try readNameEid(state, ast_eid);
    const init_ref = state.store.currentValue(ast_eid, kernel_eids.ATTR_AST_LET_INIT) orelse
        return error.AstMissingValue;
    if (!ref_mod.isEid(init_ref)) return error.UnknownAstKind;

    // Outermost-level `let` in top-level mode persists to the scope so
    // the binding is visible to later REPL lines / script forms. A `let`
    // inside a top-level block (parent_state != null) stays frame-local.
    if (state.parent_state == null) {
        if (state.top_level) |env| {
            try emitScopeStore(state, env.scope_const, env.bind_sel, name_eid, ref_mod.untagEid(init_ref));
            return;
        }
    }

    try compileExpr(state, ref_mod.untagEid(init_ref));

    // Allocate slot AFTER compiling init — the init expression cannot
    // legally reference the binding being introduced.
    const slot = state.allocSlot() catch return error.TooManyLocals;
    state.scope.put(name_eid, slot) catch return error.OutOfMemory;

    state.encoder.storeLocal(state.allocator, slot) catch return error.OutOfMemory;
    state.encoder.pushLocal(state.allocator, slot) catch return error.OutOfMemory;
}

/// Compile a variable reference. Looks up the name in the current
/// method's scope first, then walks the enclosing-method chain
/// (Phase 11.11). On a parent-chain hit, each intermediate level
/// captures the name from its own parent so the runtime chain stays
/// flat — each block frame only ever reads from its immediate parent.
fn compileVarRef(state: *State, ast_eid: u64) CompileError!void {
    const name_eid = try readNameEid(state, ast_eid);
    if (try resolveVarSlot(state, name_eid)) |slot| {
        state.encoder.pushLocal(state.allocator, slot) catch return error.OutOfMemory;
        return;
    }
    // Free variable: in top-level mode, resolve against the persistent
    // scope at runtime (`[scope lookup: #name]`); otherwise it's an error.
    const env = state.top_level orelse return error.VarNotInScope;
    try emitScopeSend1(state, env.scope_const, env.lookup_sel, name_eid);
}

/// Find or capture-emit a slot for `name_eid`. Returns the slot index
/// in `state`'s frame, or null if the name is not a local/param/capture
/// anywhere up the enclosing-method chain (a free/global variable).
/// Recurses up the parent chain; each level records a CaptureSpec
/// mapping parent's slot to its own. On a total miss no capture slots
/// are allocated (the parent recursion returns null before any append).
fn resolveVarSlot(state: *State, name_eid: u64) CompileError!?u8 {
    if (state.scope.get(name_eid)) |slot| return slot;

    const parent = state.parent_state orelse return null;
    const parent_slot = (try resolveVarSlot(parent, name_eid)) orelse return null;

    // Allocate a local slot in our frame to hold the captured value.
    const local_slot = state.allocSlot() catch return error.TooManyLocals;
    state.scope.put(name_eid, local_slot) catch return error.OutOfMemory;
    state.captures.append(state.allocator, .{ .parent_slot = parent_slot, .local_slot = local_slot }) catch return error.OutOfMemory;
    return local_slot;
}

/// Emit `[scope <sel> #name]` — push scope const, push the name as a
/// symbol const, SEND_1. Used for top-level `lookup:`.
fn emitScopeSend1(state: *State, scope_const: Ref, sel: u32, name_eid: u64) CompileError!void {
    const scope_idx = try state.poolIndexFor(scope_const);
    state.encoder.pushConst(state.allocator, scope_idx) catch return error.OutOfMemory;
    const name_idx = try state.poolIndexFor(ref_mod.mkEid(name_eid));
    state.encoder.pushConst(state.allocator, name_idx) catch return error.OutOfMemory;
    const site = try bumpSendSite(state);
    state.encoder.send1(state.allocator, sel, site) catch return error.OutOfMemory;
}

/// Emit `[scope <sel> #name value]` where `value` is produced by
/// compiling `value_ast_eid` — push scope const, push name symbol const,
/// compile the value expression, SEND_N(2). Used for top-level
/// `bind:value:` (`let`) and `set:value:` (`:=`). The send's return is
/// the bound value, matching the frame-local let/assign result.
fn emitScopeStore(state: *State, scope_const: Ref, sel: u32, name_eid: u64, value_ast_eid: u64) CompileError!void {
    const scope_idx = try state.poolIndexFor(scope_const);
    state.encoder.pushConst(state.allocator, scope_idx) catch return error.OutOfMemory;
    const name_idx = try state.poolIndexFor(ref_mod.mkEid(name_eid));
    state.encoder.pushConst(state.allocator, name_idx) catch return error.OutOfMemory;
    try compileExpr(state, value_ast_eid);
    const site = try bumpSendSite(state);
    state.encoder.sendN(state.allocator, sel, 2, site) catch return error.OutOfMemory;
}

/// Compile `name := expr`. Value of the assign expression is the
/// stored value. Same capture-walk semantics as compileVarRef — but
/// note that captured-then-written values are by-value under Phase
/// 11.11 semantics (mutations don't propagate to outer scope).
/// Reference semantics is a Phase 11.12 follow-up.
fn compileAssign(state: *State, ast_eid: u64) CompileError!void {
    const name_eid = try readNameEid(state, ast_eid);
    const value_ref = state.store.currentValue(ast_eid, kernel_eids.ATTR_AST_ASSIGN_VALUE) orelse
        return error.AstMissingValue;
    if (!ref_mod.isEid(value_ref)) return error.UnknownAstKind;

    if (try resolveVarSlot(state, name_eid)) |slot| {
        try compileExpr(state, ref_mod.untagEid(value_ref));
        state.encoder.storeLocal(state.allocator, slot) catch return error.OutOfMemory;
        state.encoder.pushLocal(state.allocator, slot) catch return error.OutOfMemory;
        return;
    }
    // Free variable assignment: in top-level mode, mutate the nearest
    // existing scope binding at runtime (`[scope set: #name value]`).
    const env = state.top_level orelse return error.VarNotInScope;
    try emitScopeStore(state, env.scope_const, env.set_sel, name_eid, ref_mod.untagEid(value_ref));
}

fn readNameEid(state: *State, ast_eid: u64) CompileError!u64 {
    const name_ref = state.store.currentValue(ast_eid, kernel_eids.ATTR_AST_NAME) orelse
        return error.AstMissingValue;
    if (!ref_mod.isEid(name_ref)) return error.UnknownAstKind;
    return ref_mod.untagEid(name_ref);
}

/// Compile a `[: params | body]` block literal. The block body is
/// compiled as its own CompiledMethod and stored in `state.block_children`
/// to be materialized later (parent's materialize patches the pool slot
/// with the block's method-eid). MAKE_BLOCK opcode pushes the
/// reified block value at runtime.
fn compileBlock(state: *State, ast_eid: u64) CompileError!void {
    const body_ref = state.store.currentValue(ast_eid, kernel_eids.ATTR_AST_BLOCK_BODY) orelse
        return error.UnknownAstKind;
    if (!ref_mod.isEid(body_ref)) return error.UnknownAstKind;
    const body_eid = ref_mod.untagEid(body_ref);

    // Block params (zero or more) populate the block-method's locals[0..n].
    var first_param: ?u64 = null;
    if (state.store.currentValue(ast_eid, kernel_eids.ATTR_AST_BLOCK_PARAMS)) |p_ref| {
        if (ref_mod.isEid(p_ref)) first_param = ref_mod.untagEid(p_ref);
    }

    // Recursively compile the block body as its own method, threading
    // current state as parent so VAR_REF in the body can resolve outer
    // locals via the capture chain.
    // Thread top_level into the child so a free var / `:=` inside a
    // top-level block still resolves to the persistent scope.
    var child = try compileMethodInner(state.allocator, state.store, body_eid, first_param, state, state.top_level);
    errdefer child.deinit(state.allocator);

    // Reserve pool slot + record child for materialize-time patching.
    const slot = state.reservePoolSlot() catch return error.OutOfMemory;
    state.block_children.append(state.allocator, .{ .pool_idx = slot, .compiled = child }) catch return error.OutOfMemory;

    state.encoder.makeBlock(state.allocator, slot) catch return error.OutOfMemory;
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
    return materializeInner(allocator, store, table, m, module_eid, null);
}

fn materializeInner(
    allocator: std.mem.Allocator,
    store: *FactStore,
    table: *ObjectTable,
    m: *const CompiledMethod,
    module_eid: u64,
    parent_method_eid: ?u64,
) MaterializeError!u64 {
    const method_eid = table.alloc();

    // Walk block children first: each produces a method-eid that gets
    // patched into our constant pool. Children carry :block-parent-method
    // back-pointer to us for non-local-return resolution.
    var patched_pool: []Ref = blk: {
        if (m.constant_pool.len == 0) break :blk &[_]Ref{};
        const copy = allocator.dupe(Ref, m.constant_pool) catch return error.OutOfMemory;
        break :blk copy;
    };
    errdefer if (patched_pool.len > 0) allocator.free(patched_pool);

    for (m.block_children) |*child| {
        const child_eid = try materializeInner(allocator, store, table, &child.compiled, module_eid, method_eid);
        patched_pool[child.pool_idx] = ref_mod.mkEid(child_eid);
    }

    // Route the bytecode-stream string box to the swept strings arena (Phase
    // 15.D-followon): it's a `[method :bytecode-stream str]` fact value (R1),
    // so :gc reclaims it when the method is pruned. Safe — a frame's raw
    // `bytecode` pointer into the payload exists only while that method runs,
    // never at the quiescent gc point. The HAMT key dup stays on `allocator`.
    const sa = if (arenas.ready()) arenas.strings() else allocator;
    const bytes_ref = strings.mkString(sa, m.bytecode) catch return error.OutOfMemory;
    const pool_ref: ?Ref = if (patched_pool.len == 0)
        null
    else
        boxed.mkRrb(allocator, patched_pool) catch return error.OutOfMemory;
    if (patched_pool.len > 0) allocator.free(patched_pool);

    storeAssert(store, method_eid, kernel_eids.ATTR_VTABLE, ref_mod.mkEid(kernel_eids.CLOSURE_VT), module_eid) catch return error.StoreError;
    storeAssert(store, method_eid, kernel_eids.ATTR_BYTECODE_STREAM, bytes_ref, module_eid) catch return error.StoreError;
    if (pool_ref) |p| {
        storeAssert(store, method_eid, kernel_eids.ATTR_CONSTANT_POOL, p, module_eid) catch return error.StoreError;
    }
    storeAssert(store, method_eid, kernel_eids.ATTR_METHOD_BODY_AST, ref_mod.mkEid(m.method_body_ast), module_eid) catch return error.StoreError;
    storeAssertSmallInt(store, method_eid, kernel_eids.ATTR_METHOD_ARG_COUNT, m.arg_count, module_eid) catch return error.StoreError;
    storeAssertSmallInt(store, method_eid, kernel_eids.ATTR_METHOD_LOCAL_COUNT, m.local_count, module_eid) catch return error.StoreError;
    storeAssertSmallInt(store, method_eid, kernel_eids.ATTR_METHOD_SEND_SITE_COUNT, m.send_site_count, module_eid) catch return error.StoreError;
    if (parent_method_eid) |pme| {
        storeAssert(store, method_eid, kernel_eids.ATTR_BLOCK_PARENT_METHOD, ref_mod.mkEid(pme), module_eid) catch return error.StoreError;
    }

    // Phase 11.11 — capture-spec array. Heap-allocate a CaptureSpecArray
    // (extern struct with inline tail) and store its pointer Ref under
    // :method-captures-ptr. Date 1NF preserved by omitting the attr
    // entirely when nothing is captured.
    if (m.captures.len > 0) {
        const ptr_ref = try allocCaptureSpecArrayRef(allocator, m.captures);
        storeAssert(store, method_eid, kernel_eids.ATTR_METHOD_CAPTURES_PTR, ptr_ref, module_eid) catch return error.StoreError;
    }

    // Install the Closure LAST: every fact assertion above triggers
    // promotion.onEntityMutate which demotes any materialized entity
    // (clearing its object-table entry). Putting the closure after all
    // facts means demote-on-mutate can't undo our work — the entity is
    // not yet in the table during the asserts.
    //
    // Layout matches the kernel's libid-object convention: `[vt_ref]
    // [method][data]` with `vt_ref` at offset -1 so `vtableEidOf` finds
    // CLOSURE_VT for any send to a method-eid Ref (the value/value:/
    // whileTrue: methods on CLOSURE_VT work as if the bytecode method
    // were a block closure).
    const total = 3 * @sizeOf(Ref);
    // Route the method-closure wrapper to the swept closures_vt arena (Phase
    // 15.5): when a method is redefined and the superseded entity is later
    // pruned (slot cleared) and uncaptured, :gc reclaims this wrapper. Eager
    // free at the redefine tx is unsafe — a captured inner block resolves its
    // method-eid through this slot, and only a full mark knows if it survives.
    const ca = if (arenas.ready()) arenas.closuresVt() else allocator;
    const buf = ca.alignedAlloc(u8, .of(Ref), total) catch return error.OutOfMemory;
    @memset(buf, 0);
    const vt_slot: *Ref = @ptrCast(@alignCast(buf.ptr));
    vt_slot.* = ref_mod.mkEid(kernel_eids.CLOSURE_VT);
    const c_ptr: *closure_mod.Closure = @ptrCast(@alignCast(buf.ptr + @sizeOf(Ref)));
    c_ptr.* = .{ .method = @ptrCast(&dispatch.bytecodeDispatcherImpl), .data = ref_mod.mkEid(method_eid) };
    table.put(method_eid, ref_mod.mkPointer(@ptrCast(c_ptr))) catch return error.StoreError;

    return method_eid;
}

fn allocCaptureSpecArrayRef(allocator: std.mem.Allocator, specs: []const CaptureSpec) MaterializeError!Ref {
    const header_sz = @sizeOf(vm_mod.CaptureSpecArrayHeader);
    const total = header_sz + @sizeOf(CaptureSpec) * specs.len;
    const buf = allocator.alignedAlloc(u8, .of(vm_mod.CaptureSpecArrayHeader), total) catch return error.OutOfMemory;
    const header: *vm_mod.CaptureSpecArrayHeader = @ptrCast(@alignCast(buf.ptr));
    header.* = .{ .len = @intCast(specs.len) };
    const tail_addr: usize = @intFromPtr(buf.ptr) + header_sz;
    const tail: [*]CaptureSpec = @ptrFromInt(tail_addr);
    for (specs, 0..) |sp, i| tail[i] = sp;
    return ref_mod.mkPointer(@ptrCast(buf.ptr));
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
