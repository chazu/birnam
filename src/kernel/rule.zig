//! Phase 7.1: Datalog rules + recursion.
//!
//! Rules are parameterized clause bodies. Multiple rules may share a name
//! (definitional disjunction). Phase 7.1 stores rules in a kernel-side
//! registry keyed by rule-name EID. Rule bodies are nested Zig `Clause`
//! structures. Phase 7.3.7 walks this back to pure-fact storage once
//! clauses become first-class fact entities.

const std = @import("std");
const hamt = @import("hamt.zig");
const query_mod = @import("query.zig");
const fact_store = @import("fact_store.zig");
const object_table = @import("object_table.zig");
const ref_mod = @import("ref.zig");
const kernel_eids = @import("kernel_eids.zig");

pub const Clause = query_mod.Clause;
pub const Term = query_mod.Term;

pub const Rule = struct {
    /// Number of positional parameters. Caller's invocation arg list must
    /// have exactly this many terms.
    n_params: u32,
    /// Body clauses. May reference variable indices 0..n_params-1 (the
    /// parameters) plus higher indices (internal locals). All internal
    /// vars get renamed to caller-scope fresh indices at evaluation time.
    body: []const Clause,
    /// Highest variable index used in body — needed for per-invocation
    /// rename pass.
    max_var: u32,
};

/// Phase 7.3.7: fact-backed rule storage. Rules live as entities in the
/// fact store; the Registry is a thin lookup helper. Module unload retracts
/// the rule facts via the regular Phase 5 mechanism.
///
/// Trade-off accepted for the spike: rule bodies are still stored as a
/// pointer-Ref to a heap `[]const Clause` slice (the body's clause-tree is
/// not yet fact-shaped). Phase 9.5+ may walk clause storage back to facts
/// fully; for now, registry definition is fact-shaped, body remains heap.
pub const Registry = struct {
    allocator: std.mem.Allocator,
    store: *fact_store.FactStore,
    table: *object_table.ObjectTable,

    pub fn init(
        allocator: std.mem.Allocator,
        store: *fact_store.FactStore,
        table: *object_table.ObjectTable,
    ) Registry {
        return .{ .allocator = allocator, .store = store, .table = table };
    }

    pub fn deinit(self: *Registry) void {
        _ = self;
        // No persistent state to free; rule body slices are owned by the
        // caller's arena (or kernel allocator) and live alongside the
        // facts referencing them.
    }

    pub fn define(self: *Registry, name_eid: u64, rule: Rule) !void {
        const rule_eid = self.table.alloc();
        // Heap-allocate a slice header so the pointer remains stable.
        const body_holder = try self.allocator.create([]const Clause);
        body_holder.* = rule.body;

        _ = try self.store.assert(rule_eid, kernel_eids.ATTR_RULE_NAME, ref_mod.mkEid(name_eid), kernel_eids.M_BOOTSTRAP);
        _ = try self.store.assert(rule_eid, kernel_eids.ATTR_RULE_PARAMS, ref_mod.mkSmallInt(@intCast(rule.n_params)).?, kernel_eids.M_BOOTSTRAP);
        _ = try self.store.assert(rule_eid, kernel_eids.ATTR_RULE_MAX_VAR, ref_mod.mkSmallInt(@intCast(rule.max_var)).?, kernel_eids.M_BOOTSTRAP);
        _ = try self.store.assert(rule_eid, kernel_eids.ATTR_RULE_BODY, ref_mod.mkPointer(@ptrCast(body_holder)), kernel_eids.M_BOOTSTRAP);
    }

    /// Look up all rules currently registered under `name_eid`. Returns a
    /// fresh owned slice (caller frees via `allocator.free`). Empty slice
    /// when no rules match.
    pub fn lookup(
        self: *const Registry,
        allocator: std.mem.Allocator,
        name_eid: u64,
    ) ![]Rule {
        var out = std.ArrayList(Rule){};
        var it = self.store.entitiesMatching(kernel_eids.ATTR_RULE_NAME, ref_mod.mkEid(name_eid));
        while (it.next()) |r_eid| {
            const params = self.store.currentValue(r_eid, kernel_eids.ATTR_RULE_PARAMS) orelse continue;
            const max_var = self.store.currentValue(r_eid, kernel_eids.ATTR_RULE_MAX_VAR) orelse continue;
            const body = self.store.currentValue(r_eid, kernel_eids.ATTR_RULE_BODY) orelse continue;
            if (!ref_mod.isSmallInt(params)) continue;
            if (!ref_mod.isSmallInt(max_var)) continue;
            if (!ref_mod.isPointer(body)) continue;
            const body_holder: *const []const Clause = @ptrCast(@alignCast(ref_mod.untagPointer(body)));
            try out.append(allocator, .{
                .n_params = @intCast(ref_mod.untagSmallInt(params)),
                .max_var = @intCast(ref_mod.untagSmallInt(max_var)),
                .body = body_holder.*,
            });
        }
        return out.toOwnedSlice(allocator);
    }
};

// -- Variable renaming -------------------------------------------------------

/// Walk a clause tree, returning a copy where every variable index `i`
/// becomes `i + offset`. Caller owns the returned memory.
pub fn renameClauses(
    allocator: std.mem.Allocator,
    clauses: []const Clause,
    offset: u32,
) ![]Clause {
    const out = try allocator.alloc(Clause, clauses.len);
    for (clauses, 0..) |c, i| {
        out[i] = try renameClause(allocator, c, offset);
    }
    return out;
}

fn renameClause(allocator: std.mem.Allocator, c: Clause, offset: u32) anyerror!Clause {
    return switch (c) {
        .triple => |t| .{ .triple = .{
            .e = renameTerm(t.e, offset),
            .a = renameTerm(t.a, offset),
            .v = renameTerm(t.v, offset),
        } },
        .predicate => |p| .{ .predicate = .{
            .op = p.op,
            .lhs = renameTerm(p.lhs, offset),
            .rhs = renameTerm(p.rhs, offset),
        } },
        .negation => |inner| .{ .negation = try renameClauses(allocator, inner, offset) },
        .disjunction => |branches| blk: {
            const new_branches = try allocator.alloc([]const Clause, branches.len);
            for (branches, 0..) |b, i| new_branches[i] = try renameClauses(allocator, b, offset);
            break :blk .{ .disjunction = new_branches };
        },
        .pred_call => |pc| blk: {
            const new_args = try allocator.alloc(query_mod.Term, pc.args.len);
            for (pc.args, 0..) |t, i| new_args[i] = renameTerm(t, offset);
            break :blk .{ .pred_call = .{ .name_eid = pc.name_eid, .args = new_args } };
        },
        .fn_call => |fc| blk: {
            const new_args = try allocator.alloc(query_mod.Term, fc.args.len);
            for (fc.args, 0..) |t, i| new_args[i] = renameTerm(t, offset);
            break :blk .{ .fn_call = .{ .name_eid = fc.name_eid, .args = new_args, .target = fc.target + offset } };
        },
        .rule_invocation, .subquery => c, // pass through; full handling deferred
    };
}

fn renameTerm(t: Term, offset: u32) Term {
    return switch (t) {
        .variable => |idx| .{ .variable = idx + offset },
        else => t,
    };
}

// -- Stratification check ---------------------------------------------------

pub const StratificationError = error{NegativeCycle};

const Edge = struct { from: u64, to: u64, neg: bool };

/// Build the rule-dependency graph and check for negative cycles.
/// `seen_rules` is the set of rule names invoked by the top-level query.
/// We walk transitively. A negative edge is a rule invocation inside a
/// negation clause.
pub fn checkStratified(
    allocator: std.mem.Allocator,
    registry: *const Registry,
    seed_names: []const u64,
) !void {
    // Build edges: (from_name, to_name, negated).
    var edges = std.ArrayList(Edge){};
    defer edges.deinit(allocator);

    var visited = std.AutoHashMap(u64, void).init(allocator);
    defer visited.deinit();

    var stack = std.ArrayList(u64){};
    defer stack.deinit(allocator);

    for (seed_names) |n| try stack.append(allocator, n);

    while (stack.pop()) |name| {
        if (visited.contains(name)) continue;
        try visited.put(name, {});
        const rules = try registry.lookup(allocator, name);
        defer allocator.free(rules);
        for (rules) |r| {
            try collectEdges(allocator, r.body, name, false, &edges, &stack);
        }
    }

    // Detect negative cycles via SCC + check if any SCC has internal neg edge.
    // For the spike, use a simpler approximation: for each negative edge
    // (u, v), check if v can reach u via any path. If yes, that's a
    // negative cycle.
    for (edges.items) |e| {
        if (!e.neg) continue;
        if (try canReach(allocator, edges.items, e.to, e.from)) {
            return error.NegativeCycle;
        }
    }
}

fn collectEdges(
    allocator: std.mem.Allocator,
    clauses: []const Clause,
    from: u64,
    in_negation: bool,
    edges: *std.ArrayList(Edge),
    stack: *std.ArrayList(u64),
) anyerror!void {
    for (clauses) |c| {
        switch (c) {
            .triple, .predicate, .subquery, .pred_call, .fn_call => {},
            .negation => |inner| try collectEdges(allocator, inner, from, true, edges, stack),
            .disjunction => |branches| {
                for (branches) |b| try collectEdges(allocator, b, from, in_negation, edges, stack);
            },
            .rule_invocation => |ri| {
                try edges.append(allocator, .{ .from = from, .to = ri.name_eid, .neg = in_negation });
                try stack.append(allocator, ri.name_eid);
            },
        }
    }
}

fn canReach(
    allocator: std.mem.Allocator,
    edges: []const Edge,
    start: u64,
    target: u64,
) !bool {
    var visited = std.AutoHashMap(u64, void).init(allocator);
    defer visited.deinit();
    var stack = std.ArrayList(u64){};
    defer stack.deinit(allocator);
    try stack.append(allocator, start);
    while (stack.pop()) |n| {
        if (n == target) return true;
        if (visited.contains(n)) continue;
        try visited.put(n, {});
        for (edges) |e| {
            if (e.from == n) try stack.append(allocator, e.to);
        }
    }
    return false;
}
