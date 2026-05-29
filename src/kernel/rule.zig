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

/// Per-name list of rules (definitional disjunction).
const RuleList = std.ArrayList(Rule);

/// HAMT keyed by rule-name EID → list of rules.
pub const Registry = struct {
    allocator: std.mem.Allocator,
    map: std.AutoHashMap(u64, *RuleList),

    pub fn init(allocator: std.mem.Allocator) Registry {
        return .{
            .allocator = allocator,
            .map = std.AutoHashMap(u64, *RuleList).init(allocator),
        };
    }

    pub fn deinit(self: *Registry) void {
        var it = self.map.iterator();
        while (it.next()) |entry| {
            entry.value_ptr.*.deinit(self.allocator);
            self.allocator.destroy(entry.value_ptr.*);
        }
        self.map.deinit();
    }

    pub fn define(self: *Registry, name_eid: u64, rule: Rule) !void {
        const gop = try self.map.getOrPut(name_eid);
        if (!gop.found_existing) {
            const list = try self.allocator.create(RuleList);
            list.* = .{};
            gop.value_ptr.* = list;
        }
        try gop.value_ptr.*.append(self.allocator, rule);
    }

    pub fn lookup(self: *const Registry, name_eid: u64) ?[]const Rule {
        const list = self.map.get(name_eid) orelse return null;
        return list.items;
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
    var edges = std.ArrayList(struct { from: u64, to: u64, neg: bool }){};
    defer edges.deinit(allocator);

    var visited = std.AutoHashMap(u64, void).init(allocator);
    defer visited.deinit();

    var stack = std.ArrayList(u64){};
    defer stack.deinit(allocator);

    for (seed_names) |n| try stack.append(allocator, n);

    while (stack.pop()) |name| {
        if (visited.contains(name)) continue;
        try visited.put(name, {});
        const rules = registry.lookup(name) orelse continue;
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
    edges: *std.ArrayList(struct { from: u64, to: u64, neg: bool }),
    stack: *std.ArrayList(u64),
) anyerror!void {
    _ = from;
    _ = in_negation;
    _ = edges;
    _ = stack;
    _ = clauses;
    _ = allocator;
    // Phase 7.1: rule-invocation clauses don't exist yet in query.zig's
    // Clause union — we add them as part of integration. The full graph
    // walk lands when the Clause type gets `.rule_invocation`.
}

fn canReach(
    allocator: std.mem.Allocator,
    edges: []const struct { from: u64, to: u64, neg: bool },
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
