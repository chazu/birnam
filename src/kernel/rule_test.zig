const std = @import("std");
const testing = std.testing;

const ref_mod = @import("ref.zig");
const fact_store = @import("fact_store.zig");
const query = @import("query.zig");
const rule_mod = @import("rule.zig");

const Ref = ref_mod.Ref;
const Term = query.Term;
const Clause = query.Clause;

const M: u64 = 1;

// Rule names — use a private EID range for tests.
const NAME_ANCESTOR: u64 = 10_000;
const NAME_EVEN: u64 = 10_001;
const NAME_ODD: u64 = 10_002;
const NAME_DESCENDANT: u64 = 10_003;
const NAME_LOOPS: u64 = 10_004;

// Attribute EID for :parent
const A_PARENT: u64 = 200;
const A_FLAG: u64 = 201;

// People
const ALICE: u64 = 100;
const BOB: u64 = 101;
const CAROL: u64 = 102;
const DAVE: u64 = 103;

fn makeAncestorRule_Base(allocator: std.mem.Allocator) ![]Clause {
    const cs = try allocator.alloc(Clause, 1);
    cs[0] = .{ .triple = .{
        .e = .{ .variable = 0 }, // ?x
        .a = .{ .constant = ref_mod.mkEid(A_PARENT) },
        .v = .{ .variable = 1 }, // ?z (direct parent IS ancestor)
    } };
    return cs;
}

fn makeAncestorRule_Recursive(allocator: std.mem.Allocator) ![]Clause {
    // Body: parent(?x, ?y), ancestor(?y, ?z)
    const cs = try allocator.alloc(Clause, 2);
    cs[0] = .{ .triple = .{
        .e = .{ .variable = 0 }, // ?x
        .a = .{ .constant = ref_mod.mkEid(A_PARENT) },
        .v = .{ .variable = 2 }, // ?y
    } };
    const args = try allocator.alloc(Term, 2);
    args[0] = .{ .variable = 2 }; // ?y
    args[1] = .{ .variable = 1 }; // ?z
    cs[1] = .{ .rule_invocation = .{ .name_eid = NAME_ANCESTOR, .args = args } };
    return cs;
}

// -- Ancestor: transitive closure -------------------------------------------

test "ancestor: 4-deep parent chain produces 3 ancestors of alice" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    _ = try store.assert(ALICE, A_PARENT, ref_mod.mkEid(BOB), M);
    _ = try store.assert(BOB, A_PARENT, ref_mod.mkEid(CAROL), M);
    _ = try store.assert(CAROL, A_PARENT, ref_mod.mkEid(DAVE), M);

    var registry = rule_mod.Registry.init(a);
    defer registry.deinit();

    try registry.define(NAME_ANCESTOR, .{
        .n_params = 2,
        .body = try makeAncestorRule_Base(a),
        .max_var = 1,
    });
    try registry.define(NAME_ANCESTOR, .{
        .n_params = 2,
        .body = try makeAncestorRule_Recursive(a),
        .max_var = 2,
    });

    var q = query.Query.init(a);
    defer q.deinit();
    q.setRules(&registry);
    const vd = try q.variable("d"); // descendant — bound to alice
    const vz = try q.variable("z"); // wanted: ancestors
    try q.addFind(vz);

    const args = try a.alloc(Term, 2);
    args[0] = .{ .constant = ref_mod.mkEid(ALICE) };
    args[1] = .{ .variable = vz };
    try q.addRuleInvocation(NAME_ANCESTOR, args);
    _ = vd;

    var r = try query.execute(&q, &store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 3), r.count());

    var seen_bob = false;
    var seen_carol = false;
    var seen_dave = false;
    var i: usize = 0;
    while (i < r.count()) : (i += 1) {
        const v = r.at(i, 0);
        if (v == ref_mod.mkEid(BOB)) seen_bob = true;
        if (v == ref_mod.mkEid(CAROL)) seen_carol = true;
        if (v == ref_mod.mkEid(DAVE)) seen_dave = true;
    }
    try testing.expect(seen_bob and seen_carol and seen_dave);
}

test "ancestor: with bound descendant, finds direct chain only" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    _ = try store.assert(ALICE, A_PARENT, ref_mod.mkEid(BOB), M);
    _ = try store.assert(BOB, A_PARENT, ref_mod.mkEid(CAROL), M);

    var registry = rule_mod.Registry.init(a);
    defer registry.deinit();
    try registry.define(NAME_ANCESTOR, .{ .n_params = 2, .body = try makeAncestorRule_Base(a), .max_var = 1 });
    try registry.define(NAME_ANCESTOR, .{ .n_params = 2, .body = try makeAncestorRule_Recursive(a), .max_var = 2 });

    var q = query.Query.init(a);
    defer q.deinit();
    q.setRules(&registry);
    const vx = try q.variable("x"); // ancestor — wanted
    try q.addFind(vx);

    const args = try a.alloc(Term, 2);
    args[0] = .{ .variable = vx };
    args[1] = .{ .constant = ref_mod.mkEid(CAROL) }; // descendant = carol
    try q.addRuleInvocation(NAME_ANCESTOR, args);

    var r = try query.execute(&q, &store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 2), r.count()); // alice, bob
}

// -- Definitional disjunction --------------------------------------------

test "rules with same name union their derived tuples" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    _ = try store.assert(ALICE, A_FLAG, ref_mod.mkSmallInt(1).?, M);
    _ = try store.assert(BOB, A_FLAG, ref_mod.mkSmallInt(2).?, M);

    var registry = rule_mod.Registry.init(a);
    defer registry.deinit();

    // flagged(?e) :- flag(?e, 1).
    const r1 = try a.alloc(Clause, 1);
    r1[0] = .{ .triple = .{
        .e = .{ .variable = 0 },
        .a = .{ .constant = ref_mod.mkEid(A_FLAG) },
        .v = .{ .constant = ref_mod.mkSmallInt(1).? },
    } };
    try registry.define(NAME_LOOPS, .{ .n_params = 1, .body = r1, .max_var = 0 });

    // flagged(?e) :- flag(?e, 2).
    const r2 = try a.alloc(Clause, 1);
    r2[0] = .{ .triple = .{
        .e = .{ .variable = 0 },
        .a = .{ .constant = ref_mod.mkEid(A_FLAG) },
        .v = .{ .constant = ref_mod.mkSmallInt(2).? },
    } };
    try registry.define(NAME_LOOPS, .{ .n_params = 1, .body = r2, .max_var = 0 });

    var q = query.Query.init(a);
    defer q.deinit();
    q.setRules(&registry);
    const ve = try q.variable("e");
    try q.addFind(ve);

    const args = try a.alloc(Term, 1);
    args[0] = .{ .variable = ve };
    try q.addRuleInvocation(NAME_LOOPS, args);

    var r = try query.execute(&q, &store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 2), r.count()); // alice + bob, via the two rules
}

// -- Termination safety bound -----------------------------------------------

test "max_iter bound trips on a contrived rule" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    var i: u64 = 0;
    while (i < 200) : (i += 1) {
        _ = try store.assert(1000 + i, A_FLAG, ref_mod.mkSmallInt(@intCast(i)).?, M);
    }

    var registry = rule_mod.Registry.init(a);
    defer registry.deinit();

    // flagged(?e) :- flag(?e, _).
    const body = try a.alloc(Clause, 1);
    body[0] = .{ .triple = .{
        .e = .{ .variable = 0 },
        .a = .{ .constant = ref_mod.mkEid(A_FLAG) },
        .v = .{ .wildcard = {} },
    } };
    try registry.define(NAME_LOOPS, .{ .n_params = 1, .body = body, .max_var = 0 });

    var q = query.Query.init(a);
    defer q.deinit();
    q.setRules(&registry);
    q.max_iter = 1; // pathologically tight
    q.max_tuples = 50; // force trip before fixpoint
    const ve = try q.variable("e");
    try q.addFind(ve);
    const args = try a.alloc(Term, 1);
    args[0] = .{ .variable = ve };
    try q.addRuleInvocation(NAME_LOOPS, args);

    const result = query.execute(&q, &store);
    try testing.expectError(error.MaxTuples, result);
}

// -- No rules → empty results --------------------------------------------

// -- Stratification ---------------------------------------------------------

test "stratification rejects rule with negation cycle" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    var registry = rule_mod.Registry.init(a);
    defer registry.deinit();

    // bad(?x) :- person(?x), not bad(?x).
    // Negation through self-recursion → must be rejected.
    const body = try a.alloc(Clause, 2);
    body[0] = .{ .triple = .{
        .e = .{ .variable = 0 },
        .a = .{ .constant = ref_mod.mkEid(A_FLAG) },
        .v = .{ .wildcard = {} },
    } };
    const neg_inner = try a.alloc(Clause, 1);
    const neg_args = try a.alloc(query.Term, 1);
    neg_args[0] = .{ .variable = 0 };
    neg_inner[0] = .{ .rule_invocation = .{ .name_eid = NAME_LOOPS, .args = neg_args } };
    body[1] = .{ .negation = neg_inner };
    try registry.define(NAME_LOOPS, .{ .n_params = 1, .body = body, .max_var = 0 });

    var q = query.Query.init(a);
    defer q.deinit();
    q.setRules(&registry);
    const vx = try q.variable("x");
    try q.addFind(vx);
    const args = try a.alloc(query.Term, 1);
    args[0] = .{ .variable = vx };
    try q.addRuleInvocation(NAME_LOOPS, args);

    const result = query.execute(&q, &store);
    try testing.expectError(error.NegativeCycle, result);
}

test "rule_invocation with no registry yields no bindings" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var store = fact_store.FactStore.init(a);
    var q = query.Query.init(a);
    defer q.deinit();
    // No q.setRules — registry is null.
    const ve = try q.variable("e");
    try q.addFind(ve);
    const args = try a.alloc(Term, 1);
    args[0] = .{ .variable = ve };
    try q.addRuleInvocation(NAME_ANCESTOR, args);

    var r = try query.execute(&q, &store);
    defer r.deinit();
    try testing.expectEqual(@as(usize, 0), r.count());
}
