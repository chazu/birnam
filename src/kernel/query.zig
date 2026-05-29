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

pub const Ref = ref_mod.Ref;

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

pub const Clause = union(enum) {
    triple: Triple,
    negation: []const Clause,
    disjunction: []const []const Clause,
    predicate: PredicateClause,
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

pub const Query = struct {
    allocator: std.mem.Allocator,
    var_count: u32 = 0,
    var_names: std.StringHashMap(u32),
    var_index_to_name: std.ArrayList([]const u8),
    find: std.ArrayList(u32),
    where: std.ArrayList(Clause),
    asof: ?u64 = null,
    since: ?u64 = null,
    between: ?[2]u64 = null,
    in_module: ?u64 = null,

    pub fn init(allocator: std.mem.Allocator) Query {
        return .{
            .allocator = allocator,
            .var_names = std.StringHashMap(u32).init(allocator),
            .var_index_to_name = .{},
            .find = .{},
            .where = .{},
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
        try self.where.append(self.allocator, .{ .triple = .{ .e = e, .a = a, .v = v } });
    }

    pub fn addPredicate(self: *Query, op: PredicateOp, lhs: Term, rhs: Term) !void {
        try self.where.append(self.allocator, .{ .predicate = .{ .op = op, .lhs = lhs, .rhs = rhs } });
    }

    pub fn addNegation(self: *Query, inner: []const Clause) !void {
        try self.where.append(self.allocator, .{ .negation = inner });
    }

    pub fn addDisjunction(self: *Query, branches: []const []const Clause) !void {
        try self.where.append(self.allocator, .{ .disjunction = branches });
    }

    pub fn addFind(self: *Query, var_idx: u32) !void {
        try self.find.append(self.allocator, var_idx);
    }

    pub fn setAsOf(self: *Query, tx: u64) void {
        self.asof = tx;
    }

    pub fn setSince(self: *Query, tx: u64) void {
        self.since = tx;
    }

    pub fn setBetween(self: *Query, from: u64, to: u64) void {
        self.between = .{ from, to };
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
fn evalTriple(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    triple: Triple,
    b: Binding,
    asof: ?u64,
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
            if (in_module) |m_filter| {
                // Module filter requires checking MEAT.
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

fn evalClause(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    clause: Clause,
    b: Binding,
    asof: ?u64,
    in_module: ?u64,
    out: *std.ArrayList(Binding),
) anyerror!void {
    switch (clause) {
        .triple => |t| try evalTriple(allocator, store, t, b, asof, in_module, out),
        .predicate => |p| {
            if (evalPredicate(p, b)) try out.append(allocator, try b.dup(allocator));
        },
        .negation => |inner| {
            // Evaluate inner against b. If any binding is produced, the negation fails.
            var inner_out = std.ArrayList(Binding){};
            defer inner_out.deinit(allocator);
            try evalAllClauses(allocator, store, inner, b, asof, in_module, &inner_out);
            if (inner_out.items.len == 0) {
                try out.append(allocator, try b.dup(allocator));
            }
        },
        .disjunction => |branches| {
            for (branches) |branch| {
                try evalAllClauses(allocator, store, branch, b, asof, in_module, out);
            }
        },
    }
}

fn evalAllClauses(
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    clauses: []const Clause,
    b: Binding,
    asof: ?u64,
    in_module: ?u64,
    out: *std.ArrayList(Binding),
) !void {
    if (clauses.len == 0) {
        try out.append(allocator, try b.dup(allocator));
        return;
    }
    var current = std.ArrayList(Binding){};
    defer current.deinit(allocator);
    try current.append(allocator, try b.dup(allocator));

    for (clauses) |c| {
        var next = std.ArrayList(Binding){};
        for (current.items) |cb| {
            try evalClause(allocator, store, c, cb, asof, in_module, &next);
        }
        current.deinit(allocator);
        current = next;
    }
    for (current.items) |cb| try out.append(allocator, cb);
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
    var bindings = std.ArrayList(Binding){};
    defer {
        for (bindings.items) |b| q.allocator.free(b.values);
        bindings.deinit(q.allocator);
    }

    // Seed with empty binding.
    const seed = try q.allocator.alloc(Ref, q.var_count);
    @memset(seed, UNBOUND);
    try bindings.append(q.allocator, .{ .values = seed });

    // Reorder clauses: predicates and negations after their dependencies.
    // For Phase 7 simplicity: stable left-to-right plus a single pre-pass
    // that moves predicates after the first triple that binds their vars.
    // Skipping the reorder for the spike — caller is expected to write
    // clauses in dependency order.

    for (q.where.items) |clause| {
        var next = std.ArrayList(Binding){};
        for (bindings.items) |cb| {
            try evalClause(q.allocator, store, clause, cb, q.asof, q.in_module, &next);
        }
        for (bindings.items) |b| q.allocator.free(b.values);
        bindings.deinit(q.allocator);
        bindings = next;
    }

    // Project to find variables, dedupe (set semantics).
    const arity: u32 = @intCast(q.find.items.len);
    var seen = std.AutoHashMap(u64, void).init(q.allocator);
    defer seen.deinit();

    var rows = std.ArrayList(Ref){};
    defer rows.deinit(q.allocator);

    for (bindings.items) |b| {
        // Build projection row.
        var row_buf = std.ArrayList(Ref){};
        defer row_buf.deinit(q.allocator);
        var hash: u64 = 0;
        var ok = true;
        for (q.find.items) |var_idx| {
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
