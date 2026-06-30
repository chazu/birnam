//! src/id/relational.zig — the relational reading of the store: a datalog join.
//!
//! The rung-2 store (`store.zig`) holds facts and answers POINT reads
//! (`currentValue:attr:`). This module adds the RELATIONAL reading — pattern
//! queries over the same facts — realizing the North Star's "object model and
//! relational model are one store read two ways" (docs/30, D5.2). It is a sibling
//! module over the IdApi: it imports the running `Store` and INSTALLS its query
//! protocol onto the store's own vtable, so the same well-known object answers
//! both `assert:attr:value:` and `findAll:where:`. That object is also exported as
//! `db` — the name the example programs use.
//!
//! The core surface (the rest of the old kernel's datalog — predicates, negation,
//! disjunction, asOf/since, aggregation, rules, nested-ref pull — grows on top):
//!
//!   [db entity]                       → a fresh blank entity (a unique identity)
//!   [db assert: e attr: a value: v]   → (inherited from the store)
//!   [db findAll: ?v where: clauses]   → a Vector of every binding of ?v
//!   [db count: ?v where: clauses]     → how many distinct bindings of ?v
//!   [db sum:/min:/max: ?v where: …]   → fold over the distinct bindings of ?v
//!   [db pull: e spec: #[ #attr … ]]   → a {attr: currentValue} record Map
//!
//! A clause is `#[ e a v ]` (a Vector); each position is either a CONSTANT (matched
//! by identity) or a LOGIC VARIABLE (`?p` in source). A clause whose MIDDLE position
//! is a comparison-op symbol (`#lt #le #gt #ge #eq #ne`, or the glyphs `#< #<= #>
//! #>= #= #~=`) is instead a PREDICATE clause `#[ lhs op rhs ]` — it filters the
//! current solutions (keep the row iff `lhs op rhs`) and binds nothing, so it must
//! come AFTER the clauses that bind its variables (no clause-reorder planner yet).
//! A clause led by the symbol `#not` — `#[ #not innerClause… ]` — is NEGATION as
//! failure: keep a solution iff the inner conjunction (threaded from that
//! solution's bindings) has no match; it too binds nothing, so the negated
//! variables should already be bound. A clause led by `#or` — `#[ #or branch… ]` —
//! is DISJUNCTION: the union of the solutions each branch produces. A clause led by
//! `#rule` — `#[ #rule #name arg… ]` — invokes a (possibly recursive) RULE defined
//! by `[db defineRule: #name params: #[?a …] body: #[ clause… ]]`; a name may have
//! several definitions and its relation is their union. Logic variables are minted
//! by the grammar/codegen as `[LogicVar named: #p]` — canonical per name, so the
//! `?p` in the result position and the `?p` inside a clause are the SAME object and
//! join by identity. The engine scans the store's LIVE triples (via the
//! `liveTriplesDo:` seam) and joins clauses left-to-right, threading bindings.
//!
//! Depends on the IdApi + the Store; uses the collection factories (`Vector`/`Map`)
//! lazily at query time, so it loads after them without a bringup-order knot.

const std = @import("std");
const idapi = @import("idapi.zig");

const Oop = idapi.Oop;
const ca = std.heap.c_allocator;

var g_api: *idapi.IdApi = undefined;

pub var logicvar_vt: Oop = null; // a logic variable (payload = its name symbol)
pub var logicvar_factory_vt: Oop = null;
pub var logicvar_factory: Oop = null; // the "LogicVar" facade ([LogicVar named: #p])
pub var entity_vt: Oop = null; // a blank minted entity
pub var collector_vt: Oop = null; // the liveTriplesDo: accumulator block

// Cached selectors.
var s_value3: Oop = null;
var s_size: Oop = null;
var s_at: Oop = null;
var s_atput: Oop = null;
var s_new: Oop = null;
var s_add: Oop = null;
var s_currentValue: Oop = null;
var s_liveTriplesDo: Oop = null;
// Comparison selectors a predicate clause SENDS to its left operand.
var s_lt: Oop = null;
var s_le: Oop = null;
var s_gt: Oop = null;
var s_ge: Oop = null;
var s_eq: Oop = null;
var s_plus: Oop = null; // `+`, for `sum:where:`

// Canonical logic vars, interned by their name symbol.
var g_logicvars: std.AutoHashMapUnmanaged(Oop, Oop) = .{};

// Collection factories (Vector/Map), imported lazily on first query.
var g_vector_factory: Oop = null;
var g_map_factory: Oop = null;
fn vectorFactory() Oop {
    if (g_vector_factory == null) g_vector_factory = g_api.import("Vector");
    return g_vector_factory;
}
fn mapFactory() Oop {
    if (g_map_factory == null) g_map_factory = g_api.import("Map");
    return g_map_factory;
}

fn send0(rcv: Oop, sel: Oop) Oop {
    return g_api.send(rcv, sel, &[_]Oop{}, 0);
}
fn send1(rcv: Oop, sel: Oop, a: Oop) Oop {
    return g_api.send(rcv, sel, &[_]Oop{a}, 1);
}
fn send2(rcv: Oop, sel: Oop, a: Oop, b: Oop) Oop {
    return g_api.send(rcv, sel, &[_]Oop{ a, b }, 2);
}

// -- logic variables ---------------------------------------------------------

fn lvNameOf(o: Oop) Oop {
    const slot: *Oop = @ptrCast(@alignCast(o.?));
    return slot.*;
}
fn isLogicVar(o: Oop) bool {
    return o != null and g_api.vtableOf(o) == logicvar_vt;
}

// GC root (F3): canonical logic vars are cached in a non-id hashmap, so a `?p`
// minted once must survive collection (a later `[LogicVar named: #p]` returns the
// same object). Mark them all as roots.
fn gcRoots(mark: idapi.MarkFn) callconv(.c) void {
    var it = g_logicvars.valueIterator();
    while (it.next()) |v| mark(v.*);
    // Registered rules (their param + body Vectors) are held only here.
    var rit = g_rules.valueIterator();
    while (rit.next()) |list| for (list.items) |r| {
        mark(r.params);
        mark(r.body);
    };
}

/// `[LogicVar named: #p]` — the canonical logic variable for symbol `#p` (interned
/// so equal names share identity, which is how the join matches them).
fn mNamed(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = n;
    const sym = args[0];
    if (g_logicvars.get(sym)) |lv| return lv;
    const lv = g_api.alloc(logicvar_vt, @sizeOf(Oop));
    const slot: *Oop = @ptrCast(@alignCast(lv.?));
    slot.* = sym;
    g_logicvars.put(ca, sym, lv) catch @panic("relational: oom");
    return lv;
}

fn mLvName(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    return lvNameOf(self);
}

/// `?name` — a logic variable renders with its `?` sigil.
fn mLvPrintString(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const nm = std.mem.span(g_api.symbolName(lvNameOf(self)));
    const buf = ca.alloc(u8, nm.len + 1) catch @panic("relational: oom");
    defer ca.free(buf);
    buf[0] = '?';
    @memcpy(buf[1..], nm);
    return g_api.mkString(buf.ptr, buf.len);
}

/// `#name` — a symbol renders with its `#` sigil (records show symbol keys/values).
fn mSymPrintString(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = a;
    _ = n;
    const nm = std.mem.span(g_api.symbolName(self));
    const buf = ca.alloc(u8, nm.len + 1) catch @panic("relational: oom");
    defer ca.free(buf);
    buf[0] = '#';
    @memcpy(buf[1..], nm);
    return g_api.mkString(buf.ptr, buf.len);
}

// -- entities ----------------------------------------------------------------

/// `[db entity]` — mint a fresh blank entity. Identity IS the entity (the floor's
/// pointer identity, the store's matching key); it carries facts via `assert:…`.
fn mEntity(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    return g_api.alloc(entity_vt, 0);
}

fn mEntityPrintString(self: Oop, a: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = self;
    _ = a;
    _ = n;
    const s = "anEntity";
    return g_api.mkString(s.ptr, s.len);
}

// -- the live-triple collector (a block the store drives) --------------------

const Triple = struct { e: Oop, a: Oop, v: Oop };
const Triples = std.ArrayList(Triple);

/// The store sends `value:value:value:` here once per live triple; we push it into
/// the Zig list whose address is stashed in the block's payload.
fn mCollect3(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const slot: *usize = @ptrCast(@alignCast(self.?));
    const lst: *Triples = @ptrFromInt(slot.*);
    lst.append(ca, .{ .e = args[0], .a = args[1], .v = args[2] }) catch @panic("relational: oom");
    return g_api.nilObj;
}

fn gatherTriples(store: Oop, out: *Triples) void {
    const blk = g_api.alloc(collector_vt, @sizeOf(usize));
    const slot: *usize = @ptrCast(@alignCast(blk.?));
    slot.* = @intFromPtr(out);
    _ = send1(store, s_liveTriplesDo, blk);
}

// -- the join ----------------------------------------------------------------

const Binding = struct { v: Oop, val: Oop };

fn lookup(sol: []const Binding, v: Oop) ?Oop {
    for (sol) |b| if (b.v == v) return b.val;
    return null;
}

/// Resolve a clause term against a solution: a logic var yields its binding (null
/// if still unbound), a constant yields itself.
fn resolveTerm(sol: []const Binding, term: Oop) ?Oop {
    if (isLogicVar(term)) return lookup(sol, term);
    return term;
}

/// Is `o` the symbol named `name`? (Used to recognize clause-lead keywords like
/// `#not`.)
fn isSym(o: Oop, name: []const u8) bool {
    if (o == null or g_api.vtableOf(o) != g_api.symbol_vt) return false;
    return std.mem.eql(u8, std.mem.span(g_api.symbolName(o)), name);
}

const PredOp = struct { sel: Oop, negate: bool };

/// If `o` is a symbol naming a comparison operator — named (`#lt #le #gt #ge #eq
/// #ne`) or glyph (`#< #<= #> #>= #= #~=`) — return the prelude selector a
/// predicate clause should SEND to its left operand, plus whether to negate the
/// result. Otherwise null: the clause is an ordinary triple pattern, not a
/// predicate. Ordering ops send `< <= > >=`; `ne` sends `=` and negates. Sending
/// (rather than untagging SmallInts here) reuses the prelude and extends to any
/// type that answers the comparison, while a non-comparable operand just yields a
/// non-true result (F7 error) and drops the row instead of crashing.
fn predicateOp(o: Oop) ?PredOp {
    if (o == null or g_api.vtableOf(o) != g_api.symbol_vt) return null;
    const nm = std.mem.span(g_api.symbolName(o));
    const eq = std.mem.eql;
    if (eq(u8, nm, "lt") or eq(u8, nm, "<")) return .{ .sel = s_lt, .negate = false };
    if (eq(u8, nm, "le") or eq(u8, nm, "<=")) return .{ .sel = s_le, .negate = false };
    if (eq(u8, nm, "gt") or eq(u8, nm, ">")) return .{ .sel = s_gt, .negate = false };
    if (eq(u8, nm, "ge") or eq(u8, nm, ">=")) return .{ .sel = s_ge, .negate = false };
    if (eq(u8, nm, "eq") or eq(u8, nm, "=")) return .{ .sel = s_eq, .negate = false };
    if (eq(u8, nm, "ne") or eq(u8, nm, "~=") or eq(u8, nm, "!=")) return .{ .sel = s_eq, .negate = true };
    return null;
}

/// Extend solution `sol` by matching clause positions `pos` against triple `tri`.
/// A logic-var position binds (or must agree with an existing binding); a constant
/// must equal (by identity). Returns the extended binding set, or null on mismatch.
fn tryClause(aa: std.mem.Allocator, sol: []const Binding, pos: [3]Oop, tri: Triple) ?[]Binding {
    var b: std.ArrayList(Binding) = .{};
    b.appendSlice(aa, sol) catch @panic("relational: oom");
    const vals = [3]Oop{ tri.e, tri.a, tri.v };
    for (pos, vals) |p, val| {
        if (isLogicVar(p)) {
            if (lookup(b.items, p)) |existing| {
                if (existing != val) return null;
            } else {
                b.append(aa, .{ .v = p, .val = val }) catch @panic("relational: oom");
            }
        } else if (p != val) return null;
    }
    return b.toOwnedSlice(aa) catch @panic("relational: oom");
}

// -- rules -------------------------------------------------------------------

const Rule = struct { params: Oop, body: Oop };
const RuleList = std.ArrayList(Rule);
const DerivedSet = std.ArrayList([]Oop);

// A rule NAME maps to several definitions; the relation it denotes is their UNION
// (the datalog reading — base and recursive cases are separate `defineRule:`
// calls). Persisted across queries (it is the schema).
var g_rules: std.AutoHashMapUnmanaged(Oop, RuleList) = .{};

// Rules being materialized in the CURRENT query: name → its (possibly still
// growing) head-tuple set. A recursive self-reference reads this partial set
// instead of recursing — the semi-naive trick — so recursion terminates with NO
// variable renaming: only head VALUES cross the rule boundary (into the set), never
// variables. Cleared at each top-level `runQuery` entry; its sets live in that
// query's arena.
var g_active: std.AutoHashMapUnmanaged(Oop, *DerivedSet) = .{};

/// `[db defineRule: #name params: #[?a …] body: #[ clause… ]]` — register one
/// (possibly recursive) definition of a rule. `params` are the head variables;
/// `body` is a conjunction of clauses (the full vocabulary, including
/// `#[#rule #name …]` for recursion). Definitions accumulate: a name's relation is
/// the union of all its bodies.
fn mDefineRule(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const gop = g_rules.getOrPut(ca, args[0]) catch @panic("relational: oom");
    if (!gop.found_existing) gop.value_ptr.* = .{};
    gop.value_ptr.append(ca, .{ .params = args[1], .body = args[2] }) catch @panic("relational: oom");
    return self;
}

fn tupleInSet(d: *const DerivedSet, t: []const Oop) bool {
    outer: for (d.items) |row| {
        if (row.len != t.len) continue;
        for (row, t) |a, b| if (a != b) continue :outer;
        return true;
    }
    return false;
}

/// Materialize a rule's head-tuple relation to a fixpoint and return it (memoized
/// in `g_active` for the rest of the query). Each definition's body is evaluated as
/// a conjunction and its head projected; the union of new tuples grows the set
/// until a pass adds nothing. A self-reference during evaluation reads the partial
/// set (see `g_active`), turning recursion into iteration. `max_iter` is a safety
/// net (a finite store reaches a fixpoint well before it).
fn materialize(aa: std.mem.Allocator, name: Oop, triples: []const Triple) *DerivedSet {
    if (g_active.get(name)) |d| return d; // already (being) built — memo / recursion
    const d = aa.create(DerivedSet) catch @panic("relational: oom");
    d.* = .{};
    g_active.put(ca, name, d) catch @panic("relational: oom");
    const rules = g_rules.getPtr(name) orelse return d; // unknown rule → empty

    var iter: usize = 0;
    while (iter < 1000) : (iter += 1) {
        const before = d.items.len;
        for (rules.items) |rule| {
            const arity: usize = @intCast(g_api.smallIntValue(send0(rule.params, s_size)));
            const nbody: usize = @intCast(g_api.smallIntValue(send0(rule.body, s_size)));

            var seed: std.ArrayList([]Binding) = .{};
            seed.append(aa, &[_]Binding{}) catch @panic("relational: oom");
            var sols: []const []Binding = seed.items;
            var c: usize = 0;
            while (c < nbody) : (c += 1) {
                const clause = send1(rule.body, s_at, g_api.mkSmallInt(@intCast(c)));
                sols = applyClause(aa, sols, clause, triples);
            }

            for (sols) |sol| {
                const tuple = aa.alloc(Oop, arity) catch @panic("relational: oom");
                var ok = true;
                var i: usize = 0;
                while (i < arity) : (i += 1) {
                    const pv = send1(rule.params, s_at, g_api.mkSmallInt(@intCast(i)));
                    tuple[i] = lookup(sol, pv) orelse {
                        ok = false;
                        break;
                    };
                }
                if (ok and !tupleInSet(d, tuple)) d.append(aa, tuple) catch @panic("relational: oom");
            }
        }
        if (d.items.len == before) break; // fixpoint reached
    }
    return d;
}

/// Match a `#[ #rule #name arg… ]` clause: extend each solution by unifying the
/// call args (positionally, like a triple) against every tuple of the rule's
/// materialized relation. An arg may be a bound var (must agree), a free var
/// (binds), or a constant (must equal by identity).
fn matchRuleClause(aa: std.mem.Allocator, sols: []const []Binding, clause: Oop, csz: usize, triples: []const Triple) []const []Binding {
    const name = send1(clause, s_at, g_api.mkSmallInt(1));
    const arity = csz - 2;
    const d = materialize(aa, name, triples);

    var next: std.ArrayList([]Binding) = .{};
    for (sols) |sol| {
        for (d.items) |row| {
            if (row.len != arity) continue;
            var b: std.ArrayList(Binding) = .{};
            b.appendSlice(aa, sol) catch @panic("relational: oom");
            var ok = true;
            var i: usize = 0;
            while (i < arity) : (i += 1) {
                const p = send1(clause, s_at, g_api.mkSmallInt(@intCast(2 + i)));
                const val = row[i];
                if (isLogicVar(p)) {
                    if (lookup(b.items, p)) |ex| {
                        if (ex != val) {
                            ok = false;
                            break;
                        }
                    } else b.append(aa, .{ .v = p, .val = val }) catch @panic("relational: oom");
                } else if (p != val) {
                    ok = false;
                    break;
                }
            }
            if (ok) next.append(aa, b.toOwnedSlice(aa) catch @panic("relational: oom")) catch @panic("relational: oom");
        }
    }
    return next.toOwnedSlice(aa) catch @panic("relational: oom");
}

/// Apply one `where:` clause to the running solution set, returning the new set
/// (allocated in `aa`). Dispatches on the clause's shape: `#or` UNIONs branches,
/// `#not` NEGATES (keep a solution iff its inner conjunction has no match), `#rule`
/// joins against a (possibly recursive) rule relation, a comparison-symbol middle
/// FILTERS, otherwise it is a TRIPLE pattern that joins/extends. Compound clauses
/// run their inner clauses back through this same function.
fn applyClause(aa: std.mem.Allocator, sols: []const []Binding, clause: Oop, triples: []const Triple) []const []Binding {
    const csz: usize = @intCast(g_api.smallIntValue(send0(clause, s_size)));
    const pos0 = if (csz > 0) send1(clause, s_at, g_api.mkSmallInt(0)) else g_api.nilObj;

    // Disjunction: `#[ #or branch… ]`. UNION the solution sets each branch
    // produces from the same incoming solutions (each branch is itself a clause —
    // triple, predicate, negation, or a nested `#or` — run back through this
    // function). Duplicate solutions are harmless: the final projection dedups by
    // value (set semantics).
    if (isSym(pos0, "or")) {
        var unioned: std.ArrayList([]Binding) = .{};
        var i: usize = 1;
        while (i < csz) : (i += 1) {
            const branch = send1(clause, s_at, g_api.mkSmallInt(@intCast(i)));
            const br = applyClause(aa, sols, branch, triples);
            unioned.appendSlice(aa, br) catch @panic("relational: oom");
        }
        return unioned.toOwnedSlice(aa) catch @panic("relational: oom");
    }

    // Rule invocation: `#[ #rule #name arg… ]`. Join against the rule's
    // materialized (possibly recursive) relation.
    if (isSym(pos0, "rule") and csz >= 2) {
        return matchRuleClause(aa, sols, clause, csz, triples);
    }

    // Negation as failure: `#[ #not innerClause… ]`. Keep a solution iff threading
    // it through the inner conjunction yields no extension. Binds nothing — the
    // negated variables should already be bound by earlier clauses.
    if (isSym(pos0, "not")) {
        var kept: std.ArrayList([]Binding) = .{};
        for (sols) |sol| {
            var inner: []const []Binding = &[_][]Binding{sol};
            var i: usize = 1;
            while (i < csz) : (i += 1) {
                const ic = send1(clause, s_at, g_api.mkSmallInt(@intCast(i)));
                inner = applyClause(aa, inner, ic, triples);
            }
            if (inner.len == 0) kept.append(aa, sol) catch @panic("relational: oom");
        }
        return kept.toOwnedSlice(aa) catch @panic("relational: oom");
    }

    var pos: [3]Oop = undefined;
    pos[0] = pos0;
    pos[1] = if (csz > 1) send1(clause, s_at, g_api.mkSmallInt(1)) else g_api.nilObj;
    pos[2] = if (csz > 2) send1(clause, s_at, g_api.mkSmallInt(2)) else g_api.nilObj;

    var next: std.ArrayList([]Binding) = .{};
    if (predicateOp(pos[1])) |op| {
        // A predicate clause FILTERS solutions and binds nothing. Both operands
        // must already be bound (an unbound one drops the row — predicates belong
        // after the clauses that bind their variables; no reorder planner yet).
        // Keep the row iff `lhs op rhs` is definitively true.
        for (sols) |sol| {
            const lhs = resolveTerm(sol, pos[0]) orelse continue;
            const rhs = resolveTerm(sol, pos[2]) orelse continue;
            const res = send1(lhs, op.sel, rhs);
            const pass = if (op.negate) (res == g_api.falseObj) else (res == g_api.trueObj);
            if (pass) next.append(aa, sol) catch @panic("relational: oom");
        }
    } else {
        // A triple pattern EXTENDS solutions: join against every live triple.
        for (sols) |sol| {
            for (triples) |tri| {
                if (tryClause(aa, sol, pos, tri)) |ns|
                    next.append(aa, ns) catch @panic("relational: oom");
            }
        }
    }
    return next.toOwnedSlice(aa) catch @panic("relational: oom");
}

/// Run a `where:` query and project `result_var`, deduped (set semantics) — the
/// shared core behind `findAll:where:` and the aggregates. Returns a Vector of the
/// distinct bound values (empty if nothing matches).
fn runQuery(self: Oop, result_var: Oop, clauses: Oop) Oop {
    // Each top-level query recomputes rule relations fresh against current facts;
    // the per-query memo/recursion sets live in this query's arena (below).
    g_active.clearRetainingCapacity();

    var triples: Triples = .{};
    defer triples.deinit(ca);
    gatherTriples(self, &triples);

    var arena = std.heap.ArenaAllocator.init(ca);
    defer arena.deinit();
    const aa = arena.allocator();

    // Begin with one empty solution; each clause filters/extends the set.
    var seed: std.ArrayList([]Binding) = .{};
    seed.append(aa, &[_]Binding{}) catch @panic("relational: oom");
    var sols: []const []Binding = seed.items;

    const nclauses: usize = @intCast(g_api.smallIntValue(send0(clauses, s_size)));
    var c: usize = 0;
    while (c < nclauses) : (c += 1) {
        const clause = send1(clauses, s_at, g_api.mkSmallInt(@intCast(c)));
        sols = applyClause(aa, sols, clause, triples.items);
    }

    // Project the result variable, dropping duplicates (set semantics).
    var out = send0(vectorFactory(), s_new);
    var seen: std.ArrayList(Oop) = .{};
    defer seen.deinit(ca);
    for (sols) |sol| {
        const val = lookup(sol, result_var) orelse continue;
        var dup = false;
        for (seen.items) |s| if (s == val) {
            dup = true;
            break;
        };
        if (dup) continue;
        seen.append(ca, val) catch @panic("relational: oom");
        out = send1(out, s_add, val);
    }
    return out;
}

/// `[db findAll: ?v where: clauses]` — every distinct binding of `?v` across all
/// joined solutions of the clause conjunction. `clauses` is a Vector of clause
/// Vectors (triple `#[e a v]`, predicate `#[lhs op rhs]`, negation `#[#not …]`, or
/// disjunction `#[#or …]`). Returns a Vector (empty if nothing matches).
fn mFindAll(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return runQuery(self, args[0], args[1]);
}

// The aggregates run the same query, then fold over the DISTINCT projected values
// (set semantics, per the Date discipline — equal values count once). `sum` folds
// with `+` from 0; `min`/`max` send `<`/`>`; all reuse the prelude's arithmetic so
// any numeric type works and `count` needs no numeric assumption at all.

/// `[db count: ?v where: clauses]` — how many distinct bindings of `?v`.
fn mCount(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return send0(runQuery(self, args[0], args[1]), s_size);
}

/// `[db sum: ?v where: clauses]` — the sum of the distinct bindings (0 if none).
fn mSum(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const r = runQuery(self, args[0], args[1]);
    const sz: usize = @intCast(g_api.smallIntValue(send0(r, s_size)));
    var acc = g_api.mkSmallInt(0);
    var i: usize = 0;
    while (i < sz) : (i += 1) {
        acc = send1(acc, s_plus, send1(r, s_at, g_api.mkSmallInt(@intCast(i))));
    }
    return acc;
}

/// `[db min: ?v where: clauses]` — the least distinct binding, or nil if none.
fn mMin(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const r = runQuery(self, args[0], args[1]);
    const sz: usize = @intCast(g_api.smallIntValue(send0(r, s_size)));
    if (sz == 0) return g_api.nilObj;
    var best = send1(r, s_at, g_api.mkSmallInt(0));
    var i: usize = 1;
    while (i < sz) : (i += 1) {
        const x = send1(r, s_at, g_api.mkSmallInt(@intCast(i)));
        if (send1(x, s_lt, best) == g_api.trueObj) best = x;
    }
    return best;
}

/// `[db max: ?v where: clauses]` — the greatest distinct binding, or nil if none.
fn mMax(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const r = runQuery(self, args[0], args[1]);
    const sz: usize = @intCast(g_api.smallIntValue(send0(r, s_size)));
    if (sz == 0) return g_api.nilObj;
    var best = send1(r, s_at, g_api.mkSmallInt(0));
    var i: usize = 1;
    while (i < sz) : (i += 1) {
        const x = send1(r, s_at, g_api.mkSmallInt(@intCast(i)));
        if (send1(x, s_gt, best) == g_api.trueObj) best = x;
    }
    return best;
}

/// `[db pull: e spec: #[ #attr … ]]` — materialize `e` into a record Map of each
/// requested attribute to its current value (absent attributes are omitted).
fn mPull(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const e = args[0];
    const spec = args[1];
    var m = send0(mapFactory(), s_new);
    const sz: usize = @intCast(g_api.smallIntValue(send0(spec, s_size)));
    var i: usize = 0;
    while (i < sz) : (i += 1) {
        const a = send1(spec, s_at, g_api.mkSmallInt(@intCast(i)));
        const v = send2(self, s_currentValue, e, a);
        if (v != null) m = send2(m, s_atput, a, v);
    }
    return m;
}

// -- bringup -----------------------------------------------------------------

pub fn bringUp(api: *idapi.IdApi) void {
    g_api = api;
    const store = api.import("Store") orelse @panic("relational: Store not loaded");
    const store_vt = api.import("StoreVt") orelse @panic("relational: StoreVt not loaded");

    s_value3 = api.intern("value:value:value:");
    s_size = api.intern("size");
    s_at = api.intern("at:");
    s_atput = api.intern("at:put:");
    s_new = api.intern("new");
    s_add = api.intern("add:");
    s_currentValue = api.intern("currentValue:attr:");
    s_liveTriplesDo = api.intern("liveTriplesDo:");
    s_lt = api.intern("<");
    s_le = api.intern("<=");
    s_gt = api.intern(">");
    s_ge = api.intern(">=");
    s_eq = api.intern("=");
    s_plus = api.intern("+");

    const reloading = api.isExported("db") != 0;
    if (reloading) {
        logicvar_vt = api.import("LogicVarVt");
        logicvar_factory_vt = api.import("LogicVarFactoryVt");
        logicvar_factory = api.import("LogicVar");
        entity_vt = api.import("EntityVt");
        collector_vt = api.import("RelCollectorVt");
    } else {
        logicvar_vt = api.proto(api.object_vt);
        logicvar_factory_vt = api.proto(api.object_vt);
        logicvar_factory = api.alloc(logicvar_factory_vt, 0);
        entity_vt = api.proto(api.object_vt);
        collector_vt = api.proto(api.object_vt);
    }

    // The query protocol is installed onto the STORE's own vtable — the same
    // object answers both the imperative and relational readings (D5.2).
    api.method(store_vt, api.intern("entity"), &mEntity);
    api.method(store_vt, api.intern("findAll:where:"), &mFindAll);
    api.method(store_vt, api.intern("count:where:"), &mCount);
    api.method(store_vt, api.intern("sum:where:"), &mSum);
    api.method(store_vt, api.intern("min:where:"), &mMin);
    api.method(store_vt, api.intern("max:where:"), &mMax);
    api.method(store_vt, api.intern("defineRule:params:body:"), &mDefineRule);
    api.method(store_vt, api.intern("pull:spec:"), &mPull);

    api.method(logicvar_factory_vt, api.intern("named:"), &mNamed);
    api.method(logicvar_vt, api.intern("name"), &mLvName);
    api.method(logicvar_vt, api.intern("printString"), &mLvPrintString);
    api.method(entity_vt, api.intern("printString"), &mEntityPrintString);
    api.method(collector_vt, s_value3, &mCollect3);

    // Symbols render with their sigil (records print symbol keys/values).
    api.method(api.symbol_vt, api.intern("printString"), &mSymPrintString);

    if (!reloading) {
        _ = api.exportObj("LogicVarVt", logicvar_vt);
        _ = api.exportObj("LogicVarFactoryVt", logicvar_factory_vt);
        _ = api.exportObj("LogicVar", logicvar_factory);
        _ = api.exportObj("EntityVt", entity_vt);
        _ = api.exportObj("RelCollectorVt", collector_vt);
        _ = api.exportObj("db", store); // the store, under the name the examples use
        api.gcRegisterRootProvider(&gcRoots); // the canonical logic-var cache
    }
}

fn moduleInit(api: *idapi.IdApi, state: ?*anyopaque) callconv(.c) ?*anyopaque {
    _ = state;
    bringUp(api);
    return null;
}
comptime {
    if (!@import("builtin").is_test) @export(&moduleInit, .{ .name = "module_init" });
}
