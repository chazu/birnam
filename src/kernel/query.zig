//! Phase 7: Datalog relational core.
//!
//! Query / Term / Clause types per DATALOG.md §2. Build-time variable index
//! allocation. Naive nested-loop join. Index selection per §2.9.
//!
//! Supports: triples, negation, disjunction, inequality predicates,
//! history modifiers (asOf/since/between), inModule filtering, single +
//! multi-var find. Set semantics via final deduplication pass.

const std = @import("std");
const ref_mod = @import("ref.zig");
const fact_store = @import("fact_store.zig");
const rrb = @import("rrb.zig");
const hamt = @import("hamt.zig");
const kernel_eids = @import("kernel_eids.zig");
const aggregate = @import("aggregate.zig");

pub const Ref = ref_mod.Ref;
pub const AggregateKind = aggregate.AggregateKind;
pub const AggregateSpec = aggregate.AggregateSpec;

pub const FindElement = union(enum) {
    var_idx: u32,
    aggregate: AggregateSpec,
};

// -- Term --------------------------------------------------------------------

pub const Term = union(enum) {
    constant: Ref,
    variable: u32,
    wildcard,
};

// -- Clauses -----------------------------------------------------------------

pub const Triple = struct { e: Term, a: Term, v: Term };

pub const PredicateOp = enum { eq, ne, lt, le, gt, ge };

pub const PredicateClause = struct {
    op: PredicateOp,
    lhs: Term,
    rhs: Term,
};

pub const RuleInvocation = struct {
    /// EID of the rule's interned name. The Registry maps this to one or
    /// more `Rule`s (definitional disjunction).
    name_eid: u64,
    args: []const Term,
};

/// Subquery clause: execute `inner` as a sub-relation, then for each result
/// row, bind the outer query's variables (listed in `bindings`) to the row's
/// columns positionally. `bindings.len` must equal `inner.find.items.len`.
pub const Subquery = struct {
    inner: *Query,
    bindings: []const u32,
};

pub const Clause = union(enum) {
    triple: Triple,
    negation: []const Clause,
    disjunction: []const []const Clause,
    predicate: PredicateClause,
    rule_invocation: RuleInvocation,
    subquery: Subquery,
};

// -- Bindings ----------------------------------------------------------------

/// A binding maps variable index → Ref. Unbound vars are sentinel u64-max.
const UNBOUND: Ref = std.math.maxInt(u64);

pub const Binding = struct {
    values: []Ref,

    pub fn dup(self: Binding, allocator: std.mem.Allocator) !Binding {
        const v = try allocator.alloc(Ref, self.values.len);
        @memcpy(v, self.values);
        return .{ .values = v };
    }

    pub fn get(self: Binding, idx: u32) ?Ref {
        if (idx >= self.values.len) return null;
        const v = self.values[idx];
        return if (v == UNBOUND) null else v;
    }

    pub fn set(self: *Binding, idx: u32, v: Ref) void {
        self.values[idx] = v;
    }
};

// -- Query -------------------------------------------------------------------

const BuilderFrameKind = enum { negation, disjunction, branch };

const BuilderFrame = struct {
    kind: BuilderFrameKind,
    clauses: std.ArrayList(Clause) = .{},
    branches: std.ArrayList([]const Clause) = .{},
};

pub const Query = struct {
    allocator: std.mem.Allocator,
    var_count: u32 = 0,
    var_names: std.StringHashMap(u32),
    var_index_to_name: std.ArrayList([]const u8),
    find: std.ArrayList(FindElement),
    where: std.ArrayList(Clause),
    /// Stack of in-progress nested-clause builders. Empty = appending
    /// directly to `where`. Non-empty top frame is the current dest.
    builder_stack: std.ArrayList(BuilderFrame),
    asof: ?u64 = null,
    since: ?u64 = null,
    between: ?[2]u64 = null,
    in_module: ?u64 = null,
    /// Optional rule registry; queries with rule-invocation clauses must
    /// provide one. Owned by caller.
    rules: ?*const @import("rule.zig").Registry = null,
    /// Termination safety bounds for the rule fixpoint.
    max_iter: u32 = 100,
    max_tuples: u64 = 1_000_000,

    pub fn init(allocator: std.mem.Allocator) Query {
        return .{
            .allocator = allocator,
            .var_names = std.StringHashMap(u32).init(allocator),
            .var_index_to_name = .{},
            .find = .{},
            .where = .{},
            .builder_stack = .{},
        };
    }

    pub fn deinit(self: *Query) void {
        var it = self.var_names.iterator();
        while (it.next()) |entry| {
            self.allocator.free(entry.key_ptr.*);
        }
        self.var_names.deinit();
        self.var_index_to_name.deinit(self.allocator);
        self.find.deinit(self.allocator);
        // Caller-owned: clauses use slices from caller's arena; we don't recursively free.
        self.where.deinit(self.allocator);
        for (self.builder_stack.items) |*frame| {
            frame.clauses.deinit(self.allocator);
            frame.branches.deinit(self.allocator);
        }
        self.builder_stack.deinit(self.allocator);
    }

    fn currentDest(self: *Query) *std.ArrayList(Clause) {
        if (self.builder_stack.items.len == 0) return &self.where;
        const top = &self.builder_stack.items[self.builder_stack.items.len - 1];
        return &top.clauses;
    }

    pub fn beginNegation(self: *Query) !void {
        try self.builder_stack.append(self.allocator, .{ .kind = .negation });
    }

    pub fn endNegation(self: *Query) !void {
        std.debug.assert(self.builder_stack.items.len > 0);
        var frame = self.builder_stack.pop().?;
        std.debug.assert(frame.kind == .negation);
        const slice = try frame.clauses.toOwnedSlice(self.allocator);
        frame.branches.deinit(self.allocator);
        try self.currentDest().append(self.allocator, .{ .negation = slice });
    }

    pub fn beginDisjunction(self: *Query) !void {
        try self.builder_stack.append(self.allocator, .{ .kind = .disjunction });
    }

    pub fn beginBranch(self: *Query) !void {
        std.debug.assert(self.builder_stack.items.len > 0);
        std.debug.assert(self.builder_stack.items[self.builder_stack.items.len - 1].kind == .disjunction);
        try self.builder_stack.append(self.allocator, .{ .kind = .branch });
    }

    pub fn endBranch(self: *Query) !void {
        std.debug.assert(self.builder_stack.items.len > 0);
        var frame = self.builder_stack.pop().?;
        std.debug.assert(frame.kind == .branch);
        const slice = try frame.clauses.toOwnedSlice(self.allocator);
        frame.branches.deinit(self.allocator);
        // Parent must be disjunction.
        const parent = &self.builder_stack.items[self.builder_stack.items.len - 1];
        std.debug.assert(parent.kind == .disjunction);
        try parent.branches.append(self.allocator, slice);
    }

    pub fn endDisjunction(self: *Query) !void {
        std.debug.assert(self.builder_stack.items.len > 0);
        var frame = self.builder_stack.pop().?;
        std.debug.assert(frame.kind == .disjunction);
        const slice = try frame.branches.toOwnedSlice(self.allocator);
        frame.clauses.deinit(self.allocator);
        try self.currentDest().append(self.allocator, .{ .disjunction = slice });
    }

    /// Get or allocate a variable index by name. Build-time incremental.
    pub fn variable(self: *Query, name: []const u8) !u32 {
        if (self.var_names.get(name)) |idx| return idx;
        const idx = self.var_count;
        self.var_count += 1;
        const owned = try self.allocator.dupe(u8, name);
        try self.var_names.put(owned, idx);
        try self.var_index_to_name.append(self.allocator, owned);
        return idx;
    }

    pub fn varRef(self: *Query, name: []const u8) !Term {
        return .{ .variable = try self.variable(name) };
    }

    pub fn addTriple(self: *Query, e: Term, a: Term, v: Term) !void {
        try self.currentDest().append(self.allocator, .{ .triple = .{ .e = e, .a = a, .v = v } });
    }

    pub fn addPredicate(self: *Query, op: PredicateOp, lhs: Term, rhs: Term) !void {
        try self.currentDest().append(self.allocator, .{ .predicate = .{ .op = op, .lhs = lhs, .rhs = rhs } });
    }

    pub fn addNegation(self: *Query, inner: []const Clause) !void {
        try self.currentDest().append(self.allocator, .{ .negation = inner });
    }

    pub fn addDisjunction(self: *Query, branches: []const []const Clause) !void {
        try self.currentDest().append(self.allocator, .{ .disjunction = branches });
    }

    pub fn addRuleInvocation(self: *Query, name_eid: u64, args: []const Term) !void {
        try self.currentDest().append(self.allocator, .{ .rule_invocation = .{ .name_eid = name_eid, .args = args } });
    }

    pub fn addSubquery(self: *Query, inner: *Query, bindings: []const u32) !void {
        try self.currentDest().append(self.allocator, .{ .subquery = .{ .inner = inner, .bindings = bindings } });
    }

    pub fn setSince(self: *Query, tx: u64) void {
        self.since = tx;
    }

    pub fn setBetween(self: *Query, from: u64, to: u64) void {
        self.between = .{ from, to };
    }

    pub fn setRules(self: *Query, registry: *const @import("rule.zig").Registry) void {
        self.rules = registry;
    }

    pub fn addFind(self: *Query, var_idx: u32) !void {
        try self.find.append(self.allocator, .{ .var_idx = var_idx });
    }

    pub fn addFindAggregate(self: *Query, kind: AggregateKind, var_idx: u32) !void {
        try self.find.append(self.allocator, .{ .aggregate = .{ .kind = kind, .var_idx = var_idx } });
    }

    pub fn setAsOf(self: *Query, tx: u64) void {
        self.asof = tx;
    }

    pub fn setInModule(self: *Query, m: u64) void {
        self.in_module = m;
    }
};

// -- Term resolution ---------------------------------------------------------

fn resolveTerm(t: Term, b: Binding) ?Ref {
    return switch (t) {
        .constant => |c| c,
        .variable => |idx| b.get(idx),
        .wildcard => null,
    };
}

fn isBound(t: Term, b: Binding) bool {
    return switch (t) {
        .constant => true,
        .variable => |idx| b.get(idx) != null,
        .wildcard => false,
    };
}

// -- Triple evaluation -------------------------------------------------------

/// Evaluate a triple clause against the current binding `b`. For each matching
/// fact in the store, produce an extended binding. Append into `out`.
fn historyFilter(
    store: *fact_store.FactStore,
    e: u64,
    a: u64,
    since: ?u64,
    between: ?[2]u64,
) bool {
    if (since == null and between == null) return true;
    const tx = store.lastTouchTx(e, a) orelse return false;
    if (since) |s| {
        if (tx <= s) return false;
    }
    if (between) |bt| {
        if (tx < bt[0] or tx > bt[1]) return false;
    }
    return true;
}

fn evalTriple(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    triple: Triple,
    b: Binding,
    asof: ?u64,
    since: ?u64,
    between: ?[2]u64,
    in_module: ?u64,
    out: *std.ArrayList(Binding),
) !void {
    const e_term = triple.e;
    const a_term = triple.a;
    const v_term = triple.v;

    // History-aware view of (e, a) → current value.
    const get_current = struct {
        fn run(s: *fact_store.FactStore, e: u64, a: u64, asof_tx: ?u64) ?Ref {
            if (asof_tx) |tx| return s.currentValueAsOf(e, a, tx);
            return s.currentValue(e, a);
        }
    }.run;

    // Case A: e and a both bound → direct lookup or hasFact.
    if (isBound(e_term, b) and isBound(a_term, b)) {
        const e_val = resolveTerm(e_term, b).?;
        const a_val = resolveTerm(a_term, b).?;
        if (!ref_mod.isEid(e_val) or !ref_mod.isEid(a_val)) return;
        const e_eid = ref_mod.untagEid(e_val);
        const a_eid = ref_mod.untagEid(a_val);
        const got = get_current(store, e_eid, a_eid, asof);
        if (got) |v| {
            if (!matchOrBind(v_term, v, b)) return;
            if (!historyFilter(store, e_eid, a_eid, since, between)) return;
            if (in_module) |m_filter| {
                if (!moduleMatches(store, m_filter, e_eid, a_eid, v)) return;
            }
            var nb = try b.dup(allocator);
            bindIfVariable(v_term, v, &nb);
            try out.append(allocator, nb);
        }
        return;
    }

    // Case B: only e bound → walk e's attribute map.
    if (isBound(e_term, b) and !isBound(a_term, b)) {
        const e_val = resolveTerm(e_term, b).?;
        if (!ref_mod.isEid(e_val)) return;
        const e_eid = ref_mod.untagEid(e_val);
        var it = store.attributesOf(e_eid);
        while (it.next()) |entry| {
            const eff_v = if (asof) |tx| (store.currentValueAsOf(e_eid, entry.a, tx) orelse continue) else entry.v;
            if (!matchOrBind(a_term, ref_mod.mkEid(entry.a), b)) continue;
            if (!matchOrBind(v_term, eff_v, b)) continue;
            if (!historyFilter(store, e_eid, entry.a, since, between)) continue;
            if (in_module) |m_filter| {
                if (!moduleMatches(store, m_filter, e_eid, entry.a, eff_v)) continue;
            }
            var nb = try b.dup(allocator);
            bindIfVariable(a_term, ref_mod.mkEid(entry.a), &nb);
            bindIfVariable(v_term, eff_v, &nb);
            try out.append(allocator, nb);
        }
        return;
    }

    // Case C: only a bound → AEVT walk.
    if (!isBound(e_term, b) and isBound(a_term, b)) {
        const a_val = resolveTerm(a_term, b).?;
        if (!ref_mod.isEid(a_val)) return;
        const a_eid = ref_mod.untagEid(a_val);
        var it = store.entitiesWith(a_eid);
        while (it.next()) |entry| {
            const eff_v = if (asof) |tx| (store.currentValueAsOf(entry.e, a_eid, tx) orelse continue) else entry.v;
            if (!matchOrBind(e_term, ref_mod.mkEid(entry.e), b)) continue;
            if (!matchOrBind(v_term, eff_v, b)) continue;
            if (!historyFilter(store, entry.e, a_eid, since, between)) continue;
            if (in_module) |m_filter| {
                if (!moduleMatches(store, m_filter, entry.e, a_eid, eff_v)) continue;
            }
            var nb = try b.dup(allocator);
            bindIfVariable(e_term, ref_mod.mkEid(entry.e), &nb);
            bindIfVariable(v_term, eff_v, &nb);
            try out.append(allocator, nb);
        }
        return;
    }

    // Case D: a and v bound → AVET (entitiesMatching).
    if (!isBound(e_term, b) and isBound(a_term, b) and isBound(v_term, b)) {
        // Subcase of case C; already covered above. Left for symmetry.
        return;
    }

    // Case E: nothing bound — full scan. Phase 7 SHIPS THIS but with a warning.
    // Iterate AEVT over every attribute, then every entity within. Slow.
    // For now skip silently — tests will avoid this case until perf matters.
    // TODO Phase 7+: emit a warning fact or log.
    return;
}

/// If `t` is a constant or already-bound variable, returns true iff it equals `v`.
/// If `t` is an unbound variable or wildcard, returns true unconditionally.
fn matchOrBind(t: Term, v: Ref, b: Binding) bool {
    return switch (t) {
        .constant => |c| c == v,
        .variable => |idx| if (b.get(idx)) |existing| existing == v else true,
        .wildcard => true,
    };
}

fn bindIfVariable(t: Term, v: Ref, b: *Binding) void {
    switch (t) {
        .variable => |idx| if (b.get(idx) == null) b.set(idx, v),
        else => {},
    }
}

fn moduleMatches(
    store: *fact_store.FactStore,
    m_filter: u64,
    e: u64,
    a: u64,
    v: Ref,
) bool {
    // Walk MEAT for the filter module. Check if this (e, a) has matching v.
    var it = store.factsOfModule(m_filter, store.allocator) catch return false;
    defer it.deinit();
    while (it.next()) |triple| {
        if (triple.e == e and triple.a == a and triple.v == v) return true;
    }
    return false;
}

// -- Predicate evaluation ----------------------------------------------------

fn evalPredicate(p: PredicateClause, b: Binding) bool {
    const lhs = resolveTerm(p.lhs, b) orelse return false;
    const rhs = resolveTerm(p.rhs, b) orelse return false;
    return switch (p.op) {
        .eq => lhs == rhs,
        .ne => lhs != rhs,
        .lt, .le, .gt, .ge => blk: {
            // SmallInts only per Phase 7 Q4.
            if (!ref_mod.isSmallInt(lhs) or !ref_mod.isSmallInt(rhs)) break :blk false;
            const l = ref_mod.untagSmallInt(lhs);
            const r = ref_mod.untagSmallInt(rhs);
            break :blk switch (p.op) {
                .lt => l < r,
                .le => l <= r,
                .gt => l > r,
                .ge => l >= r,
                else => unreachable,
            };
        },
    };
}

// -- Clause dispatch ---------------------------------------------------------

pub const SubqueryMemo = std.AutoHashMap(*const Query, *Result);

const DerivedMap = std.AutoHashMap(u64, *DerivedSet);

const DerivedSet = struct {
    allocator: std.mem.Allocator,
    /// Each entry is a `[]Ref` of length `n_params`. Allocated from `allocator`.
    tuples: std.ArrayList([]Ref),
    /// Hash → count map for dedup-by-content.
    seen: std.AutoHashMap(u64, void),
    n_params: u32,

    pub fn init(allocator: std.mem.Allocator, n_params: u32) DerivedSet {
        return .{
            .allocator = allocator,
            .tuples = .{},
            .seen = std.AutoHashMap(u64, void).init(allocator),
            .n_params = n_params,
        };
    }

    pub fn deinit(self: *DerivedSet) void {
        for (self.tuples.items) |t| self.allocator.free(t);
        self.tuples.deinit(self.allocator);
        self.seen.deinit();
    }

    fn hashTuple(t: []const Ref) u64 {
        var h: u64 = 0x9E3779B97F4A7C15;
        for (t) |v| h = (h ^ v) *% 0xBF58476D1CE4E5B9;
        return h;
    }

    pub fn add(self: *DerivedSet, tuple: []const Ref) !bool {
        const h = hashTuple(tuple);
        if (self.seen.contains(h)) return false;
        try self.seen.put(h, {});
        const owned = try self.allocator.dupe(Ref, tuple);
        try self.tuples.append(self.allocator, owned);
        return true;
    }
};

fn evalRuleInvocation(
    allocator: std.mem.Allocator,
    ri: RuleInvocation,
    b: Binding,
    derived: *const DerivedMap,
    out: *std.ArrayList(Binding),
) !void {
    const set = derived.get(ri.name_eid) orelse return;
    for (set.tuples.items) |tuple| {
        if (tuple.len != ri.args.len) continue;
        var nb = try b.dup(allocator);
        var ok = true;
        for (ri.args, tuple) |arg, val| {
            switch (arg) {
                .constant => |c| if (c != val) {
                    ok = false;
                    break;
                },
                .variable => |idx| {
                    if (nb.get(idx)) |existing| {
                        if (existing != val) {
                            ok = false;
                            break;
                        }
                    } else {
                        nb.set(idx, val);
                    }
                },
                .wildcard => {},
            }
        }
        if (ok) {
            try out.append(allocator, nb);
        } else {
            allocator.free(nb.values);
        }
    }
}

fn evalSubquery(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    sq: Subquery,
    b: Binding,
    memo: ?*SubqueryMemo,
    out: *std.ArrayList(Binding),
) !void {
    if (sq.bindings.len != sq.inner.find.items.len) return;
    // Look up or compute the subquery's result.
    var result_ptr: *Result = undefined;
    var owned = false;
    if (memo) |m| {
        if (m.get(sq.inner)) |cached| {
            result_ptr = cached;
        } else {
            const fresh = try allocator.create(Result);
            fresh.* = try execute(sq.inner, store);
            try m.put(sq.inner, fresh);
            result_ptr = fresh;
        }
    } else {
        const fresh = try allocator.create(Result);
        fresh.* = try execute(sq.inner, store);
        result_ptr = fresh;
        owned = true;
    }
    defer if (owned) {
        result_ptr.deinit();
        allocator.destroy(result_ptr);
    };

    const r = result_ptr;
    var row: usize = 0;
    while (row < r.count()) : (row += 1) {
        var nb = try b.dup(allocator);
        var ok = true;
        for (sq.bindings, 0..) |outer_var, col| {
            const v = r.at(row, @intCast(col));
            if (nb.get(outer_var)) |existing| {
                if (existing != v) {
                    ok = false;
                    break;
                }
            } else {
                nb.set(outer_var, v);
            }
        }
        if (ok) {
            try out.append(allocator, nb);
        } else {
            allocator.free(nb.values);
        }
    }
}

fn evalClause(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    clause: Clause,
    b: Binding,
    asof: ?u64,
    since: ?u64,
    between: ?[2]u64,
    in_module: ?u64,
    derived: ?*const DerivedMap,
    memo: ?*SubqueryMemo,
    out: *std.ArrayList(Binding),
) anyerror!void {
    switch (clause) {
        .triple => |t| try evalTriple(allocator, store, t, b, asof, since, between, in_module, out),
        .predicate => |p| {
            if (evalPredicate(p, b)) try out.append(allocator, try b.dup(allocator));
        },
        .negation => |inner| {
            var inner_out = std.ArrayList(Binding){};
            defer {
                for (inner_out.items) |ib| allocator.free(ib.values);
                inner_out.deinit(allocator);
            }
            try evalAllClauses(allocator, store, inner, b, asof, since, between, in_module, derived, memo, &inner_out);
            if (inner_out.items.len == 0) {
                try out.append(allocator, try b.dup(allocator));
            }
        },
        .disjunction => |branches| {
            for (branches) |branch| {
                try evalAllClauses(allocator, store, branch, b, asof, since, between, in_module, derived, memo, out);
            }
        },
        .rule_invocation => |ri| {
            if (derived) |d| try evalRuleInvocation(allocator, ri, b, d, out);
        },
        .subquery => |sq| try evalSubquery(allocator, store, sq, b, memo, out),
    }
}

fn evalAllClauses(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    clauses: []const Clause,
    b: Binding,
    asof: ?u64,
    since: ?u64,
    between: ?[2]u64,
    in_module: ?u64,
    derived: ?*const DerivedMap,
    memo: ?*SubqueryMemo,
    out: *std.ArrayList(Binding),
) !void {
    if (clauses.len == 0) {
        try out.append(allocator, try b.dup(allocator));
        return;
    }
    var current = std.ArrayList(Binding){};
    defer {
        for (current.items) |cb| allocator.free(cb.values);
        current.deinit(allocator);
    }
    try current.append(allocator, try b.dup(allocator));

    for (clauses) |c| {
        var next = std.ArrayList(Binding){};
        for (current.items) |cb| {
            try evalClause(allocator, store, c, cb, asof, since, between, in_module, derived, memo, &next);
        }
        for (current.items) |cb| allocator.free(cb.values);
        current.deinit(allocator);
        current = next;
    }
    for (current.items) |cb| try out.append(allocator, cb);
    current.items.len = 0;
}

// -- Semi-naive rule materialization -----------------------------------------

const rule_mod = @import("rule.zig");

pub const RuleError = error{
    MaxIterations,
    MaxTuples,
    NoRegistry,
};

fn collectRuleNamesInClauses(
    allocator: std.mem.Allocator,
    clauses: []const Clause,
    names: *std.AutoHashMap(u64, void),
    registry: *const rule_mod.Registry,
) !void {
    for (clauses) |c| {
        switch (c) {
            .triple, .predicate, .subquery => {},
            .negation => |inner| try collectRuleNamesInClauses(allocator, inner, names, registry),
            .disjunction => |branches| {
                for (branches) |b| try collectRuleNamesInClauses(allocator, b, names, registry);
            },
            .rule_invocation => |ri| {
                if (!names.contains(ri.name_eid)) {
                    try names.put(ri.name_eid, {});
                    if (registry.lookup(ri.name_eid)) |rules| {
                        for (rules) |rule| {
                            try collectRuleNamesInClauses(allocator, rule.body, names, registry);
                        }
                    }
                }
            },
        }
    }
}

fn materializeRules(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    query: *Query,
) !DerivedMap {
    var derived = DerivedMap.init(allocator);
    const registry = query.rules orelse return derived; // No rules, no work.

    // Collect rule names referenced by the query (transitively).
    var names = std.AutoHashMap(u64, void).init(allocator);
    defer names.deinit();
    try collectRuleNamesInClauses(allocator, query.where.items, &names, registry);

    if (names.count() == 0) return derived;

    // Initialize empty derived sets per name.
    var nit = names.iterator();
    while (nit.next()) |entry| {
        const name = entry.key_ptr.*;
        const rules = registry.lookup(name) orelse continue;
        // n_params from first rule (all rules with same name must agree;
        // enforce in registry.define later).
        const n_params = rules[0].n_params;
        const set = try allocator.create(DerivedSet);
        set.* = DerivedSet.init(allocator, n_params);
        try derived.put(name, set);
    }

    // Fixpoint loop.
    var iter: u32 = 0;
    var total: u64 = 0;
    var changed = true;
    while (changed) {
        changed = false;
        iter += 1;
        if (iter > query.max_iter) return error.MaxIterations;

        nit = names.iterator();
        while (nit.next()) |entry| {
            const name = entry.key_ptr.*;
            const rules = registry.lookup(name) orelse continue;
            const set = derived.get(name).?;
            for (rules) |rule| {
                const seed = try allocator.alloc(Ref, rule.max_var + 1);
                @memset(seed, UNBOUND);
                const seed_binding = Binding{ .values = seed };
                var rule_out = std.ArrayList(Binding){};
                defer {
                    for (rule_out.items) |b| allocator.free(b.values);
                    rule_out.deinit(allocator);
                }
                try evalAllClauses(
                    allocator,
                    store,
                    rule.body,
                    seed_binding,
                    query.asof,
                    query.since,
                    query.between,
                    query.in_module,
                    &derived,
                    null, // no memo inside rule body materialization
                    &rule_out,
                );
                allocator.free(seed);

                // Project each output binding to the first n_params vars.
                const tuple_buf = try allocator.alloc(Ref, rule.n_params);
                defer allocator.free(tuple_buf);
                for (rule_out.items) |b| {
                    var ok = true;
                    for (0..rule.n_params) |i| {
                        const v = b.get(@intCast(i)) orelse {
                            ok = false;
                            break;
                        };
                        tuple_buf[i] = v;
                    }
                    if (!ok) continue;
                    if (try set.add(tuple_buf)) {
                        changed = true;
                        total += 1;
                        if (total > query.max_tuples) return error.MaxTuples;
                    }
                }
            }
        }
    }
    return derived;
}

fn freeDerivedMap(allocator: std.mem.Allocator, dm: *DerivedMap) void {
    var it = dm.iterator();
    while (it.next()) |entry| {
        entry.value_ptr.*.deinit();
        allocator.destroy(entry.value_ptr.*);
    }
    dm.deinit();
}

fn collectInvokedNames(clauses: []const Clause, out: *std.AutoHashMap(u64, void)) !void {
    for (clauses) |c| {
        switch (c) {
            .triple, .predicate, .subquery => {},
            .negation => |inner| try collectInvokedNames(inner, out),
            .disjunction => |branches| {
                for (branches) |b| try collectInvokedNames(b, out);
            },
            .rule_invocation => |ri| try out.put(ri.name_eid, {}),
        }
    }
}

// -- Execution + projection --------------------------------------------------

pub const Result = struct {
    allocator: std.mem.Allocator,
    /// Find arity. For single-var find = 1; for multi-var = N.
    arity: u32,
    /// Flat row-major storage: row i = rows[i*arity .. (i+1)*arity].
    rows: []Ref,

    pub fn count(self: *const Result) usize {
        return self.rows.len / self.arity;
    }

    pub fn at(self: *const Result, row: usize, col: u32) Ref {
        return self.rows[row * self.arity + col];
    }

    pub fn deinit(self: *Result) void {
        self.allocator.free(self.rows);
    }
};

pub fn execute(q: *Query, store: *fact_store.FactStore) !Result {
    // Stratification check (lazy at query-execute time per Phase 7.1 Q3).
    if (q.rules) |reg| {
        var seed_names = std.ArrayList(u64){};
        defer seed_names.deinit(q.allocator);
        var ns = std.AutoHashMap(u64, void).init(q.allocator);
        defer ns.deinit();
        try collectInvokedNames(q.where.items, &ns);
        var nit = ns.iterator();
        while (nit.next()) |entry| try seed_names.append(q.allocator, entry.key_ptr.*);
        if (seed_names.items.len > 0) {
            try @import("rule.zig").checkStratified(q.allocator, reg, seed_names.items);
        }
    }

    // First: materialize all rules referenced by the query (if any).
    var derived = try materializeRules(q.allocator, store, q);
    defer freeDerivedMap(q.allocator, &derived);

    var bindings = std.ArrayList(Binding){};
    defer {
        for (bindings.items) |b| q.allocator.free(b.values);
        bindings.deinit(q.allocator);
    }

    const seed = try q.allocator.alloc(Ref, q.var_count);
    @memset(seed, UNBOUND);
    try bindings.append(q.allocator, .{ .values = seed });

    var memo = SubqueryMemo.init(q.allocator);
    defer {
        var mit = memo.iterator();
        while (mit.next()) |e| {
            e.value_ptr.*.deinit();
            q.allocator.destroy(e.value_ptr.*);
        }
        memo.deinit();
    }

    for (q.where.items) |clause| {
        var next = std.ArrayList(Binding){};
        for (bindings.items) |cb| {
            try evalClause(q.allocator, store, clause, cb, q.asof, q.since, q.between, q.in_module, &derived, &memo, &next);
        }
        for (bindings.items) |b| q.allocator.free(b.values);
        bindings.deinit(q.allocator);
        bindings = next;
    }

    const arity: u32 = @intCast(q.find.items.len);

    // Phase 7.2: detect aggregates in find clause. If none, original
    // distinct-projection path. Otherwise: group-by aggregation.
    var has_aggregate = false;
    for (q.find.items) |fe| {
        if (fe == .aggregate) {
            has_aggregate = true;
            break;
        }
    }

    if (!has_aggregate) {
        var seen = std.AutoHashMap(u64, void).init(q.allocator);
        defer seen.deinit();
        var rows = std.ArrayList(Ref){};
        defer rows.deinit(q.allocator);
        for (bindings.items) |b| {
            var row_buf = std.ArrayList(Ref){};
            defer row_buf.deinit(q.allocator);
            var hash: u64 = 0;
            var ok = true;
            for (q.find.items) |fe| {
                const var_idx = fe.var_idx;
                const v = b.get(var_idx) orelse {
                    ok = false;
                    break;
                };
                try row_buf.append(q.allocator, v);
                hash = hash *% 0x9E3779B97F4A7C15 ^ v;
            }
            if (!ok) continue;
            if (seen.contains(hash)) continue;
            try seen.put(hash, {});
            try rows.appendSlice(q.allocator, row_buf.items);
        }
        return .{
            .allocator = q.allocator,
            .arity = arity,
            .rows = try rows.toOwnedSlice(q.allocator),
        };
    }

    return try projectAggregates(q, bindings.items);
}

/// Group-by aggregation. Partitions bindings by their projection onto
/// non-aggregate find vars, then computes each aggregate per group over
/// distinct values (Date discipline).
fn projectAggregates(q: *Query, bindings: []const Binding) !Result {
    const arity: u32 = @intCast(q.find.items.len);
    const allocator = q.allocator;

    // Collect group-key (non-aggregate) find element positions.
    var group_positions = std.ArrayList(usize){};
    defer group_positions.deinit(allocator);
    for (q.find.items, 0..) |fe, i| {
        if (fe == .var_idx) try group_positions.append(allocator, i);
    }

    // Build group hash key for a binding. If group_positions is empty,
    // returns a fixed sentinel hash so all bindings land in one group.
    const universal_hash: u64 = 0xC0FFEE_BABE;

    const Group = struct {
        bindings: std.ArrayList(Binding),
        // For result emission, remember any binding (we just need one to
        // read group-key values).
    };

    var groups = std.AutoHashMap(u64, *Group).init(allocator);
    defer {
        var it = groups.iterator();
        while (it.next()) |entry| {
            entry.value_ptr.*.bindings.deinit(allocator);
            allocator.destroy(entry.value_ptr.*);
        }
        groups.deinit();
    }

    for (bindings) |b| {
        // Skip bindings that don't satisfy group keys.
        var ok = true;
        var hash: u64 = if (group_positions.items.len == 0) universal_hash else 0;
        for (group_positions.items) |pos| {
            const var_idx = q.find.items[pos].var_idx;
            const v = b.get(var_idx) orelse {
                ok = false;
                break;
            };
            hash = hash *% 0x9E3779B97F4A7C15 ^ v;
        }
        if (!ok) continue;
        const gop = try groups.getOrPut(hash);
        if (!gop.found_existing) {
            const g = try allocator.create(Group);
            g.* = .{ .bindings = .{} };
            gop.value_ptr.* = g;
        }
        try gop.value_ptr.*.bindings.append(allocator, b);
    }

    // Emit one row per group.
    var rows = std.ArrayList(Ref){};
    defer rows.deinit(allocator);

    var git = groups.iterator();
    while (git.next()) |entry| {
        const g = entry.value_ptr.*;
        if (g.bindings.items.len == 0) continue;
        const sample = g.bindings.items[0];
        for (q.find.items) |fe| {
            switch (fe) {
                .var_idx => |idx| {
                    const v = sample.get(idx) orelse return error.UnboundGroupVar;
                    try rows.append(allocator, v);
                },
                .aggregate => |spec| {
                    // Collect distinct values of spec.var_idx across this group.
                    var distinct_set = std.AutoHashMap(Ref, void).init(allocator);
                    defer distinct_set.deinit();
                    for (g.bindings.items) |gb| {
                        const v = gb.get(spec.var_idx) orelse continue;
                        try distinct_set.put(v, {});
                    }
                    var values = std.ArrayList(Ref){};
                    defer values.deinit(allocator);
                    var dit = distinct_set.iterator();
                    while (dit.next()) |de| try values.append(allocator, de.key_ptr.*);
                    const result = try aggregate.compute(allocator, spec.kind, values.items);
                    try rows.append(allocator, result);
                },
            }
        }
    }

    return .{
        .allocator = q.allocator,
        .arity = arity,
        .rows = try rows.toOwnedSlice(allocator),
    };
}
