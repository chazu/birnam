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
//! The surface (the full datalog reading — predicates, negation, disjunction,
//! aggregation, recursive rules, asOf time travel, and nested/`:many` pull):
//!
//!   [db entity]                       → a fresh blank entity (a unique identity)
//!   [db assert: e attr: a value: v]   → (inherited from the store)
//!   [db findAll: ?v where: clauses]   → a Vector of every binding of ?v
//!   [db findAll: ?v where: cl asOf: t]→ the snapshot at basis t (also since: /
//!                                        between:and: for change windows)
//!   [db basis]                        → the current tx counter (time-travel handle)
//!   [db count: ?v where: clauses]     → how many distinct bindings of ?v
//!   [db sum:/min:/max:/avg: ?v where:]→ fold over the distinct bindings of ?v
//!   [db groupBy: ?g aggregate: #k of: ?v where: cl]  → a Map {group → aggregate}
//!   [db pull: e spec: #[ #attr … ]]   → a record Map; #[#ref <subspec>] recurses,
//!                                        :many attrs yield Vectors
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
var s_assert: Oop = null; // the store's "assert:attr:value:" — rule facts (docs/42 §D2)
var s_rule_name: Oop = null; // attribute tagging a definition-pair with its rule name
var s_at: Oop = null;
var s_atput: Oop = null;
var s_new: Oop = null;
var s_add: Oop = null;
var s_currentValue: Oop = null;
var s_liveTriplesDo: Oop = null;
var s_liveTriplesAsOfDo: Oop = null;
var s_liveTriplesSinceDo: Oop = null;
var s_liveTriplesBetweenAndDo: Oop = null;
// Comparison selectors a predicate clause SENDS to its left operand.
var s_lt: Oop = null;
var s_le: Oop = null;
var s_gt: Oop = null;
var s_ge: Oop = null;
var s_eq: Oop = null;
var s_plus: Oop = null; // `+`, for `sum:where:`
var s_div: Oop = null; // `/`, for `avg:where:`
var s_cardinality: Oop = null; // `cardinality:`, to render :many in pull
var s_many: Oop = null; // the `#many` cardinality symbol

// The Vector INSTANCE vtable, imported lazily — pull tells a nested ref spec
// (a Vector `#[#refattr <subspec>]`) from a leaf attribute (a symbol).
var g_vector_vt: Oop = null;
fn vectorVt() Oop {
    if (g_vector_vt == null) g_vector_vt = g_api.import("VectorVt");
    return g_vector_vt;
}
fn isVector(o: Oop) bool {
    return o != null and g_api.vtableOf(o) == vectorVt();
}

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

/// Gather the store's live triples into `out` over a transaction window — `lo` and
/// `hi` are SmallInt bases or null. Both null = current; `hi` only = `asOf:` (the
/// snapshot before `hi`); `lo` only = `since:` (changed at/after `lo`); both =
/// `between:and:`. The whole query then runs against that historical view.
fn gatherTriples(store: Oop, out: *Triples, lo: Oop, hi: Oop) void {
    const blk = g_api.alloc(collector_vt, @sizeOf(usize));
    const slot: *usize = @ptrCast(@alignCast(blk.?));
    slot.* = @intFromPtr(out);
    if (lo != null and hi != null) {
        _ = g_api.send(store, s_liveTriplesBetweenAndDo, &[_]Oop{ lo, hi, blk }, 3);
    } else if (hi != null) {
        _ = send2(store, s_liveTriplesAsOfDo, hi, blk);
    } else if (lo != null) {
        _ = send2(store, s_liveTriplesSinceDo, lo, blk);
    } else {
        _ = send1(store, s_liveTriplesDo, blk);
    }
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
// calls). This is a REBUILT CACHE (docs/42 §D2): the canonical rule definitions
// live in the store as `[pair __rule-name__ name]` facts (queryable, historied,
// serialized). Write-through on `defineRule:` for query speed; rebuilt from the
// facts when they change out from under it (image load) via `rebuildRules`.
var g_rules: std.AutoHashMapUnmanaged(Oop, RuleList) = .{};

// Rules being materialized in the CURRENT query: name → its (possibly still
// growing) head-tuple set. A recursive reference reads this partial set instead of
// recursing — the semi-naive trick — so recursion terminates with NO variable
// renaming: only head VALUES cross the rule boundary (into the set), never
// variables. Cleared at each top-level query entry; its sets live in the query's
// arena.
var g_active: std.AutoHashMapUnmanaged(Oop, *DerivedSet) = .{};
// >0 while the global fixpoint loop is running (a nested rule reference then reads
// the partial set rather than launching its own loop). `done` short-circuits later
// rule references in the same query once the joint fixpoint has converged.
var g_fixpoint_depth: usize = 0;
var g_fixpoint_done: bool = false;

/// `[db defineRule: #name params: #[?a …] body: #[ clause… ]]` — register one
/// (possibly recursive) definition of a rule. `params` are the head variables;
/// `body` is a conjunction of clauses (the full vocabulary, including
/// `#[#rule #name …]` for recursion). Definitions accumulate: a name's relation is
/// the union of all its bodies.
fn mDefineRule(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const name = args[0];
    const params = args[1];
    const body = args[2];
    appendRule(name, params, body); // write-through cache
    // Canonical: a rule definition IS relational data (docs/42 §D2). Store the
    // (params, body) pair as a fresh entity tagged with its rule name, so the
    // definition becomes queryable, historied, and serialized into the image. Each
    // `defineRule:` mints a DISTINCT pair, so a name's several definitions
    // accumulate as distinct entities — no `:many` schema needed.
    var pair = send0(vectorFactory(), s_new);
    pair = send1(pair, s_add, params);
    pair = send1(pair, s_add, body); // #[params body]
    _ = g_api.send(self, s_assert, &[_]Oop{ pair, s_rule_name, name }, 3);
    return self;
}

/// Append one (params, body) definition to the `g_rules` cache under `name`.
fn appendRule(name: Oop, params: Oop, body: Oop) void {
    const gop = g_rules.getOrPut(ca, name) catch @panic("relational: oom");
    if (!gop.found_existing) gop.value_ptr.* = .{};
    gop.value_ptr.append(ca, .{ .params = params, .body = body }) catch @panic("relational: oom");
}

/// Rebuild the `g_rules` cache from the store's canonical rule facts — used when the
/// facts change under the cache (an image load replaces the whole fact stream). Each
/// `[pair __rule-name__ name]` fact reconstitutes one definition (params/body ride
/// the pair). Idempotent.
fn rebuildRulesFrom(store: Oop) void {
    var it = g_rules.valueIterator();
    while (it.next()) |list| list.deinit(ca);
    g_rules.clearRetainingCapacity();
    var triples: Triples = .{};
    defer triples.deinit(ca);
    gatherTriples(store, &triples, null, null);
    for (triples.items) |t| {
        if (t.a != s_rule_name) continue;
        const pair = t.e; // #[params body]
        appendRule(t.v, send1(pair, s_at, g_api.mkSmallInt(0)), send1(pair, s_at, g_api.mkSmallInt(1)));
    }
}

/// `[db rebuildRules]` — rebuild the rule cache from the canonical rule facts. Sent
/// after an image load (the fact stream was replaced wholesale).
fn mRebuildRules(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = args;
    _ = n;
    rebuildRulesFrom(self);
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

fn ensureSet(aa: std.mem.Allocator, name: Oop) *DerivedSet {
    if (g_active.get(name)) |d| return d;
    const d = aa.create(DerivedSet) catch @panic("relational: oom");
    d.* = .{};
    g_active.put(ca, name, d) catch @panic("relational: oom");
    return d;
}

fn totalActiveTuples() usize {
    var t: usize = 0;
    var it = g_active.valueIterator();
    while (it.next()) |dp| t += dp.*.items.len;
    return t;
}

/// Evaluate every definition of one rule once, projecting each solution's head and
/// adding any NEW tuples to the rule's set. A reference to another (or the same)
/// rule in a body reads that rule's current partial set (see `materialize`), so one
/// pass propagates one layer of derivation.
fn evalRuleInto(aa: std.mem.Allocator, name: Oop, triples: []const Triple) void {
    const rules = g_rules.getPtr(name) orelse return;
    const d = g_active.get(name).?;
    for (rules.items) |rule| {
        const arity: usize = @intCast(g_api.smallIntValue(send0(rule.params, s_size)));
        const sols = applyClauses(aa, rule.body, triples);
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
}

/// Return a rule's head-tuple relation, materialized to a fixpoint. The OUTERMOST
/// reference in a query runs a single JOINT fixpoint over every rule reached
/// (transitively): each pass evaluates all active rules once, and the set of active
/// rules grows as bodies reference new ones, until no rule's relation grows and no
/// new rule appears. Evaluating rules together (not one-then-freeze) is what makes
/// MUTUAL recursion correct, not just self-recursion. A nested reference during the
/// loop just reads the current partial set. `max_iter` is a safety net (a finite
/// store reaches the fixpoint well before it).
fn materialize(aa: std.mem.Allocator, name: Oop, triples: []const Triple) *DerivedSet {
    const d = ensureSet(aa, name);
    if (g_fixpoint_depth > 0 or g_fixpoint_done) return d; // mid-loop, or already converged

    g_fixpoint_depth += 1;
    var iter: usize = 0;
    while (iter < 1000) : (iter += 1) {
        const tuples_before = totalActiveTuples();
        const rules_before = g_active.count();
        // Snapshot the names: a body may add a newly-referenced rule to g_active.
        var names: std.ArrayList(Oop) = .{};
        defer names.deinit(ca);
        var it = g_active.keyIterator();
        while (it.next()) |kp| names.append(ca, kp.*) catch @panic("relational: oom");
        for (names.items) |nm| evalRuleInto(aa, nm, triples);
        if (totalActiveTuples() == tuples_before and g_active.count() == rules_before) break;
    }
    g_fixpoint_depth -= 1;
    g_fixpoint_done = true;
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
        // A predicate clause FILTERS solutions and binds nothing. The planner has
        // already ordered it after the clauses that bind its operands; if one is
        // still unbound (an unbindable var) the row drops. Keep iff `lhs op rhs`.
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

// -- the clause-reorder planner ----------------------------------------------
//
// Predicates and negation FILTER (they bind nothing); a triple/rule/disjunction
// BINDS. A filter run before its variables are bound would wrongly drop every row,
// so the planner sorts the clauses: it runs whichever clause is "ready" next — a
// binder always, a filter only once every variable it mentions is bound. This lets
// predicates and negation be written anywhere in the `where:` list.

fn varContains(set: []const Oop, v: Oop) bool {
    for (set) |x| if (valEq(x, v)) return true; // value-aware: boxed ints
    return false;
}

/// Collect the logic variables a clause mentions (recursing into `#or`/`#not`,
/// reading a `#rule`'s args, a predicate's operands, or a triple's positions).
fn collectClauseVars(clause: Oop, set: *std.ArrayList(Oop), aa: std.mem.Allocator) void {
    const csz: usize = @intCast(g_api.smallIntValue(send0(clause, s_size)));
    const pos0 = if (csz > 0) send1(clause, s_at, g_api.mkSmallInt(0)) else g_api.nilObj;
    const add = struct {
        fn f(o: Oop, s: *std.ArrayList(Oop), a: std.mem.Allocator) void {
            if (isLogicVar(o) and !varContains(s.items, o)) s.append(a, o) catch @panic("relational: oom");
        }
    }.f;
    if (isSym(pos0, "or") or isSym(pos0, "not")) {
        var i: usize = 1;
        while (i < csz) : (i += 1) collectClauseVars(send1(clause, s_at, g_api.mkSmallInt(@intCast(i))), set, aa);
        return;
    }
    if (isSym(pos0, "rule") and csz >= 2) {
        var i: usize = 2;
        while (i < csz) : (i += 1) add(send1(clause, s_at, g_api.mkSmallInt(@intCast(i))), set, aa);
        return;
    }
    const pos1 = if (csz > 1) send1(clause, s_at, g_api.mkSmallInt(1)) else g_api.nilObj;
    if (predicateOp(pos1) != null) {
        add(pos0, set, aa);
        if (csz > 2) add(send1(clause, s_at, g_api.mkSmallInt(2)), set, aa);
        return;
    }
    var i: usize = 0;
    while (i < csz and i < 3) : (i += 1) add(send1(clause, s_at, g_api.mkSmallInt(@intCast(i))), set, aa);
}

/// A binder (triple / rule / disjunction) extends solutions; a filter (predicate /
/// negation) only constrains them.
fn isBinderClause(clause: Oop) bool {
    const csz: usize = @intCast(g_api.smallIntValue(send0(clause, s_size)));
    const pos0 = if (csz > 0) send1(clause, s_at, g_api.mkSmallInt(0)) else g_api.nilObj;
    if (isSym(pos0, "not")) return false;
    if (isSym(pos0, "or")) return true;
    if (isSym(pos0, "rule") and csz >= 2) return true;
    const pos1 = if (csz > 1) send1(clause, s_at, g_api.mkSmallInt(1)) else g_api.nilObj;
    return predicateOp(pos1) == null; // triple, unless it's a predicate
}

/// Seed one empty solution, then run the clauses of `clausesVec` in a binding-aware
/// order (filters deferred until their variables are bound), threading solutions.
fn applyClauses(aa: std.mem.Allocator, clausesVec: Oop, triples: []const Triple) []const []Binding {
    const n: usize = @intCast(g_api.smallIntValue(send0(clausesVec, s_size)));
    const clauseObjs = aa.alloc(Oop, n) catch @panic("relational: oom");
    for (clauseObjs, 0..) |*co, i| co.* = send1(clausesVec, s_at, g_api.mkSmallInt(@intCast(i)));

    // Plan: pick the next ready clause (a binder always; a filter once its vars are
    // bound). Falls back to the first remaining clause if none is ready.
    const order = aa.alloc(usize, n) catch @panic("relational: oom");
    const used = aa.alloc(bool, n) catch @panic("relational: oom");
    @memset(used, false);
    var bound: std.ArrayList(Oop) = .{};
    var vbuf: std.ArrayList(Oop) = .{};
    var k: usize = 0;
    while (k < n) : (k += 1) {
        var chosen: usize = n;
        for (clauseObjs, 0..) |cl, j| {
            if (used[j]) continue;
            const ready = isBinderClause(cl) or blk: {
                vbuf.clearRetainingCapacity();
                collectClauseVars(cl, &vbuf, aa);
                for (vbuf.items) |v| if (!varContains(bound.items, v)) break :blk false;
                break :blk true;
            };
            if (ready) {
                chosen = j;
                break;
            }
        }
        if (chosen == n) {
            for (used, 0..) |u, j| if (!u) {
                chosen = j;
                break;
            };
        }
        used[chosen] = true;
        order[k] = chosen;
        if (isBinderClause(clauseObjs[chosen])) collectClauseVars(clauseObjs[chosen], &bound, aa);
    }

    var seed: std.ArrayList([]Binding) = .{};
    seed.append(aa, &[_]Binding{}) catch @panic("relational: oom");
    var sols: []const []Binding = seed.items;
    for (order) |idx| sols = applyClause(aa, sols, clauseObjs[idx], triples);
    return sols;
}

/// Value-equality for set-semantics dedup. Integers are boxed now (no immediates),
/// so equal-valued bindings are distinct objects — identity `==` would count them
/// as distinct. Match value types by value; everything else by identity (eids,
/// interned symbols). Mirrors store.zig's `eq`.
fn valEq(a: Oop, b: Oop) bool {
    if (a == b) return true;
    if (a == null or b == null) return false;
    if (g_api.isSmallInt(a) != 0 and g_api.isSmallInt(b) != 0)
        return g_api.smallIntValue(a) == g_api.smallIntValue(b);
    if (g_api.vtableOf(a) == g_api.string_vt and g_api.vtableOf(b) == g_api.string_vt) {
        var la: usize = 0;
        var lb: usize = 0;
        const pa = g_api.stringBytes(a, &la);
        const pb = g_api.stringBytes(b, &lb);
        return std.mem.eql(u8, pa[0..la], pb[0..lb]);
    }
    return false;
}

/// Run a `where:` query and project `result_var`, deduped (set semantics) — the
/// shared core behind `findAll:where:` and the aggregates. Returns a Vector of the
/// distinct bound values (empty if nothing matches).
fn runQuery(self: Oop, result_var: Oop, clauses: Oop, lo: Oop, hi: Oop) Oop {
    // Each top-level query recomputes rule relations fresh against current facts;
    // the per-query memo/recursion sets live in this query's arena (below).
    g_active.clearRetainingCapacity();
    g_fixpoint_done = false;

    var triples: Triples = .{};
    defer triples.deinit(ca);
    gatherTriples(self, &triples, lo, hi);

    var arena = std.heap.ArenaAllocator.init(ca);
    defer arena.deinit();
    const aa = arena.allocator();

    const sols = applyClauses(aa, clauses, triples.items);

    // Project the result variable, dropping duplicates (set semantics).
    var out = send0(vectorFactory(), s_new);
    var seen: std.ArrayList(Oop) = .{};
    defer seen.deinit(ca);
    for (sols) |sol| {
        const val = lookup(sol, result_var) orelse continue;
        var dup = false;
        for (seen.items) |s| if (valEq(s, val)) {
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
    return runQuery(self, args[0], args[1], null, null);
}

/// `[db findAll: ?v where: clauses asOf: tx]` — the same query against the snapshot
/// live as of transaction `tx` (a `[db basis]` value). Time travel: capture a
/// basis, mutate, then read the earlier world.
fn mFindAllAsOf(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return runQuery(self, args[0], args[1], null, args[2]);
}

/// `[db findAll: ?v where: clauses since: tx]` — only triples whose current value
/// was asserted at/after `tx`: the query restricted to what has CHANGED since that
/// basis.
fn mFindAllSince(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return runQuery(self, args[0], args[1], args[2], null);
}

/// `[db findAll: ?v where: clauses between: lo and: hi]` — the snapshot at `hi`
/// restricted to triples that changed at/after `lo` (a transaction window).
fn mFindAllBetween(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return runQuery(self, args[0], args[1], args[2], args[3]);
}

// The aggregates run the same query, then fold over the DISTINCT projected values
// (set semantics, per the Date discipline — equal values count once). `sum` folds
// with `+` from 0; `min`/`max` send `<`/`>`; all reuse the prelude's arithmetic so
// any numeric type works and `count` needs no numeric assumption at all.

/// `[db count: ?v where: clauses]` — how many distinct bindings of `?v`.
fn mCount(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return send0(runQuery(self, args[0], args[1], null, null), s_size);
}

/// `[db sum: ?v where: clauses]` — the sum of the distinct bindings (0 if none).
fn mSum(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const r = runQuery(self, args[0], args[1], null, null);
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
    const r = runQuery(self, args[0], args[1], null, null);
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
    const r = runQuery(self, args[0], args[1], null, null);
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

/// `[db avg: ?v where: clauses]` — the mean of the distinct bindings (sum sent `/`
/// count), or nil if none. With integer values this truncates (the prelude's `/`).
fn mAvg(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const r = runQuery(self, args[0], args[1], null, null);
    const sz: usize = @intCast(g_api.smallIntValue(send0(r, s_size)));
    if (sz == 0) return g_api.nilObj;
    var acc = g_api.mkSmallInt(0);
    var i: usize = 0;
    while (i < sz) : (i += 1) {
        acc = send1(acc, s_plus, send1(r, s_at, g_api.mkSmallInt(@intCast(i))));
    }
    return send1(acc, s_div, g_api.mkSmallInt(@intCast(sz)));
}

/// Fold a list of DISTINCT values per an aggregate kind symbol (`#count #sum #min
/// #max #avg`). count → the size; the others nil on empty (a group is never empty).
fn aggregateValues(kind: Oop, values: []const Oop) Oop {
    const cnt = values.len;
    if (isSym(kind, "count")) return g_api.mkSmallInt(@intCast(cnt));
    if (cnt == 0) return g_api.nilObj;
    if (isSym(kind, "sum") or isSym(kind, "avg")) {
        var acc = g_api.mkSmallInt(0);
        for (values) |x| acc = send1(acc, s_plus, x);
        return if (isSym(kind, "avg")) send1(acc, s_div, g_api.mkSmallInt(@intCast(cnt))) else acc;
    }
    if (isSym(kind, "min") or isSym(kind, "max")) {
        const sel = if (isSym(kind, "min")) s_lt else s_gt;
        var best = values[0];
        for (values[1..]) |x| if (send1(x, sel, best) == g_api.trueObj) {
            best = x;
        };
        return best;
    }
    return g_api.nilObj; // unknown kind
}

const Group = struct { g: Oop, vals: std.ArrayList(Oop) };

/// `[db groupBy: ?g aggregate: #kind of: ?v where: clauses]` — partition the
/// solutions by the value of `?g` and, within each group, fold the DISTINCT values
/// of `?v` by `#kind` (`#count #sum #min #max #avg`). Returns a Map from each group
/// value to its aggregate.
fn mGroupBy(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    const gvar = args[0];
    const kind = args[1];
    const vvar = args[2];
    const clauses = args[3];

    g_active.clearRetainingCapacity();
    g_fixpoint_done = false;
    var triples: Triples = .{};
    defer triples.deinit(ca);
    gatherTriples(self, &triples, null, null);

    var arena = std.heap.ArenaAllocator.init(ca);
    defer arena.deinit();
    const aa = arena.allocator();

    const sols = applyClauses(aa, clauses, triples.items);

    // Bucket distinct `?v` values under each distinct `?g` value (group order =
    // first-appearance order).
    var groups: std.ArrayList(Group) = .{};
    for (sols) |sol| {
        const g = lookup(sol, gvar) orelse continue;
        const v = lookup(sol, vvar) orelse continue;
        var grp: ?*Group = null;
        for (groups.items) |*gp| if (valEq(gp.g, g)) {
            grp = gp;
            break;
        };
        if (grp == null) {
            groups.append(aa, .{ .g = g, .vals = .{} }) catch @panic("relational: oom");
            grp = &groups.items[groups.items.len - 1];
        }
        if (!varContains(grp.?.vals.items, v)) grp.?.vals.append(aa, v) catch @panic("relational: oom");
    }

    var m = send0(mapFactory(), s_new);
    for (groups.items) |grp| m = send2(m, s_atput, grp.g, aggregateValues(kind, grp.vals.items));
    return m;
}

// The dedicated logic var pull uses to gather an attribute's values via the join.
fn pullVar() Oop {
    return mNamed(logicvar_factory, &[_]Oop{g_api.intern("__pullv")}, 1);
}

// All CURRENT values of (e, a) as a Vector — `findAll: ?v where: #[ #[e a ?v] ]`,
// reusing the join so a :many attribute yields every member.
fn attrValues(self: Oop, e: Oop, a: Oop) Oop {
    const vv = pullVar();
    var clause = send0(vectorFactory(), s_new);
    clause = send1(clause, s_add, e);
    clause = send1(clause, s_add, a);
    clause = send1(clause, s_add, vv);
    const clauses = send1(send0(vectorFactory(), s_new), s_add, clause);
    return runQuery(self, vv, clauses, null, null);
}

fn isMany(self: Oop, a: Oop) bool {
    return send1(self, s_cardinality, a) == s_many;
}

/// Materialize `e` into a record Map per `spec`. Each spec element is either a leaf
/// attribute symbol `#a` (→ its current value, or a Vector of members for a `:many`
/// attribute) or a NESTED ref spec `#[ #refattr <subspec> ]` (→ pull the entity the
/// attribute points at with `<subspec>`; a Vector of sub-records for a `:many` ref).
/// Absent attributes are omitted (no NULL).
fn pullEntity(self: Oop, e: Oop, spec: Oop) Oop {
    var m = send0(mapFactory(), s_new);
    const sz: usize = @intCast(g_api.smallIntValue(send0(spec, s_size)));
    var i: usize = 0;
    while (i < sz) : (i += 1) {
        const elem = send1(spec, s_at, g_api.mkSmallInt(@intCast(i)));
        if (isVector(elem)) {
            // Nested ref spec: #[ #refattr <subspec> ].
            const refattr = send1(elem, s_at, g_api.mkSmallInt(0));
            const subspec = send1(elem, s_at, g_api.mkSmallInt(1));
            const refs = attrValues(self, e, refattr);
            const rc: usize = @intCast(g_api.smallIntValue(send0(refs, s_size)));
            if (rc == 0) continue;
            if (isMany(self, refattr)) {
                var subs = send0(vectorFactory(), s_new);
                var j: usize = 0;
                while (j < rc) : (j += 1) {
                    const ref = send1(refs, s_at, g_api.mkSmallInt(@intCast(j)));
                    subs = send1(subs, s_add, pullEntity(self, ref, subspec));
                }
                m = send2(m, s_atput, refattr, subs);
            } else {
                const ref = send1(refs, s_at, g_api.mkSmallInt(0));
                m = send2(m, s_atput, refattr, pullEntity(self, ref, subspec));
            }
        } else {
            // Leaf attribute symbol.
            const vals = attrValues(self, e, elem);
            const cnt: usize = @intCast(g_api.smallIntValue(send0(vals, s_size)));
            if (cnt == 0) continue;
            if (isMany(self, elem))
                m = send2(m, s_atput, elem, vals) // a Vector of members
            else
                m = send2(m, s_atput, elem, send1(vals, s_at, g_api.mkSmallInt(0)));
        }
    }
    return m;
}

/// `[db pull: e spec: #[ #attr | #[#refattr <subspec>] … ]]` — see `pullEntity`.
fn mPull(self: Oop, args: [*]const Oop, n: usize) callconv(.c) Oop {
    _ = n;
    return pullEntity(self, args[0], args[1]);
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
    s_liveTriplesAsOfDo = api.intern("liveTriplesAsOf:do:");
    s_liveTriplesSinceDo = api.intern("liveTriplesSince:do:");
    s_liveTriplesBetweenAndDo = api.intern("liveTriplesBetween:and:do:");
    s_lt = api.intern("<");
    s_le = api.intern("<=");
    s_gt = api.intern(">");
    s_ge = api.intern(">=");
    s_eq = api.intern("=");
    s_plus = api.intern("+");
    s_div = api.intern("/");
    s_cardinality = api.intern("cardinality:");
    s_many = api.intern("many");
    s_assert = api.intern("assert:attr:value:");
    s_rule_name = api.intern("__rule-name__");

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

    // Exact payload layouts (F3): a logic variable's payload is its name symbol
    // (one Oop); a collector's is a raw pointer to the Zig triple list.
    api.setLayout(logicvar_vt, null, 0, idapi.slot_oop);
    api.setLayout(collector_vt, null, 0, idapi.slot_raw);

    // The query protocol is installed onto the STORE's own vtable — the same
    // object answers both the imperative and relational readings (D5.2).
    api.method(store_vt, api.intern("entity"), &mEntity);
    api.method(store_vt, api.intern("findAll:where:"), &mFindAll);
    api.method(store_vt, api.intern("findAll:where:asOf:"), &mFindAllAsOf);
    api.method(store_vt, api.intern("findAll:where:since:"), &mFindAllSince);
    api.method(store_vt, api.intern("findAll:where:between:and:"), &mFindAllBetween);
    api.method(store_vt, api.intern("count:where:"), &mCount);
    api.method(store_vt, api.intern("sum:where:"), &mSum);
    api.method(store_vt, api.intern("min:where:"), &mMin);
    api.method(store_vt, api.intern("max:where:"), &mMax);
    api.method(store_vt, api.intern("avg:where:"), &mAvg);
    api.method(store_vt, api.intern("groupBy:aggregate:of:where:"), &mGroupBy);
    api.method(store_vt, api.intern("defineRule:params:body:"), &mDefineRule);
    api.method(store_vt, api.intern("rebuildRules"), &mRebuildRules);
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
