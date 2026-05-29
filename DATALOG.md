# Datalog in Birnam — Design Document

This document specifies the full Datalog system across Phases 7, 7.1, 7.2, 7.3, and 7.4. Query is as central to birnam as the object model: it is how the system introspects itself, how reflection becomes uniform, and how malleability scales. The design here aims for Datomic-level expressiveness, not a toy `find/where` subset.

Phasing exists for implementability; the surface and semantics described here are coherent across all phases.

---

## 0. Premises

### 0.1 Why Datalog

Birnam is built on a relational fact store (EAV with txs). Without a real query language:

- Reflection collapses to hand-coded walks. Every "find all X" is bespoke Zig/birnam code.
- The promise of "everything is queryable" becomes a marketing claim, not a deliverable.
- Compound questions (transitive properties, set ops, aggregations) become intractable.

Datalog is what relational databases became when they grew up: declarative, recursive, decidable, composable. It matches our substrate exactly.

### 0.2 Reference target: Datomic

Datomic's query language is mature and proven over the same EAV-with-tx substrate. It has:

- Find / in / where structure
- Multiple data sources
- Disjunction (`or`), negation (`not`), conjunction (`and`)
- Predicate (`[(< ?a ?b)]`) and function (`[(+ ?a 1) ?b]`) expressions
- Aggregation in find: `(count ?m)`, `(sum ?v)`
- Rules with recursion
- Pull syntax for hierarchical results
- Time travel: `:as-of`, `:since`, `:history`

Birnam aims for parity. The differences are syntactic — pattern-sends instead of bracket-quoted EDN — and semantic where birnam's substrate already gives more (e.g., MEAT index makes module-scoped queries first-class).

### 0.3 Surface syntax (locked, see VISION.md §Query Surface)

Pattern-sends. A bracket-form containing one or more `?name` variables is a quoted clause, not an evaluated send.

```
[db findAll: ?m where: #(
    [?vt lookup: ?m]
    [?vt name: 'Point']
)]
```

This document specifies the semantics that surface; the parser implementing the surface is part of Phase 7's module.

### 0.4 Five-phase delivery

| Phase | Adds |
|---|---|
| 7 | Selection, conjunction, multi-var projection, negation, disjunction, inequality predicates, history (`asOf:`), set semantics, RRB results |
| 7.1 | Rules, recursion, stratified-negation check, semi-naive evaluation |
| 7.2 | Aggregation (count/sum/min/max/avg/distinct/collect), implicit group-by |
| 7.3 | Subqueries, query-as-Ref, saved queries, pull syntax |
| 7.4 | Inline predicates beyond inequalities, computed bindings, user-registered predicates |

Each phase delivers a usable subset. The overall design is what's described here; phases are implementation milestones.

---

## 1. Core Data Model for Queries

### 1.1 Facts as queryable atoms

A fact `[e a v]` in the store is logically a six-tuple `(e, a, v, tx, op, m)` per VISION.md §Fact. Queries operate over the **current view** by default (only `op=assert` facts whose latest tx hasn't been superseded). `asOf:`/`since:` modifiers shift the view.

The five indexes (EAVT, AEVT, AVET, VAET, MEAT) all see the same fact set; queries select the index per clause via the index-selection rules in §2.9.

### 1.2 Terms

A term in a clause is one of:

```
Term ::= Constant(Ref)        # literal Ref of any tag
       | Variable(name, idx)  # pattern variable
       | Wildcard             # _ — match anything, don't bind
```

Where `name` is the surface identifier (for error messages, pull-syntax field naming) and `idx` is the variable's stable position in the query.

### 1.3 Bindings

A **binding** maps variable indices to Refs. The empty binding is `{}`. Bindings extend as clauses match.

A **binding-set** is a set of bindings — the intermediate state during execution. Set semantics: duplicate bindings collapse.

### 1.4 Variable scope

Variables are query-scoped. Two queries sharing a variable name `?x` have unrelated variables — the index assignment is per-query. Rules (Phase 7.1) introduce parameter binding, which is the only cross-scope mechanism.

Wildcard `_` is anonymous: every occurrence binds independently and is never reused.

### 1.5 History axis

Every query carries an implicit `:as-of` slot, default = "latest tx." Set via:

```
[db findAll: ?vt where: #(...) asOf: <tx_id>]
[db findAll: ?vt where: #(...) since: <tx_id>]    # changes after tx
[db findAll: ?vt where: #(...) between: from to]  # facts asserted/retracted in range
```

The fact store already carries `(tx, op)` per cell history; query just reads the right window.

---

## 2. Phase 7 — Relational Core

### 2.1 Query structure

```zig
pub const Query = struct {
    var_count: u32,           // number of distinct variables
    var_names: []const []const u8,  // for error messages
    find: []const FindElement,
    inputs: []const InputBinding,  // from `in:` clause
    where: []const Clause,
    asof: ?u64,
    since: ?u64,
    between: ?[2]u64,
};

pub const FindElement = union(enum) {
    var_idx: u32,             // single var projection
    pull: PullSpec,           // (Phase 7.3)
    aggregate: AggregateSpec, // (Phase 7.2)
};
```

`find` is a list because multi-var projection is `findAll: #(?a ?b)` — returns tuples.

### 2.2 Clause types

```zig
pub const Clause = union(enum) {
    triple: Triple,                 // [?e ?a ?v]
    negation: []const Clause,       // [not: #(<inner>)]
    disjunction: []const []const Clause,  // [or: #(<branch1> <branch2> ...)]
    predicate: PredicateClause,     // [?a < ?b], [?n != 0]
    rule_invocation: RuleInvocation, // (Phase 7.1)
};

pub const Triple = struct {
    e: Term, a: Term, v: Term,
};
```

Phase 7 implements: `triple`, `negation`, `disjunction`, `predicate`. Phase 7.1 adds `rule_invocation`.

### 2.3 Negation semantics

`[not: <clauses>]` succeeds iff the inner clauses produce **no bindings** when joined with the current binding. Negation is closed-world (absence of fact = negation succeeds).

A negated clause must have all its variables already bound by previous clauses (range-restriction / safety condition). Otherwise the query is ill-formed and rejected at planning time.

```
# Vtables without an init method:
[db findAll: ?vt where: #(
    [?vt attr_vtable VTABLE_VT]
    [not: #([?vt SEL_INIT ?m])]
)]
```

### 2.4 Disjunction semantics

`[or: #(<branch_a> <branch_b>)]` produces bindings that match any branch. Each branch is itself a list of clauses (a conjunction).

Constraint: every branch must bind the same set of variables. Otherwise downstream clauses see undefined variables. Caught at planning time.

```
# Entities named "Point" OR "Vector":
[db findAll: ?e where: #(
    [or: #(
        #([?e name "Point"])
        #([?e name "Vector"])
    )]
)]
```

### 2.5 Predicates (Phase 7 minimum)

Inequality on bound variables / constants:

```
PredicateClause = { op, lhs: Term, rhs: Term }
op ∈ { eq, ne, lt, le, gt, ge }
```

`eq` is rarely needed (variable identity does it) but useful for `[?a = constant]`.

Predicates succeed/fail per binding; they don't produce new bindings. Both operands must be bound.

Phase 7.4 extends to user predicates and computed bindings.

### 2.6 Time travel

`asOf: tx` filters every triple-match through the historical view at `tx`: a fact is visible iff its latest assert (with `tx <= asof_tx`) has not been retracted by `asof_tx`. Fact store's `currentValueAsOf` already does this for `[e a]`; extending to range queries (e bound only, a free) requires walking the history vector in AEVT cells.

`since: tx` matches facts whose latest mutation is after `tx`. Useful for change detection.

`between: from to` matches facts asserted/retracted in `[from, to]`. Equivalent to history projection.

### 2.7 Join engine

Standard variable-elimination join:

```
bindings = [{}]
for clause in query.where:
    new_bindings = []
    for b in bindings:
        for extension in evaluate(clause, b):
            new_bindings.append(merge(b, extension))
    bindings = new_bindings
return project(bindings, query.find)
```

Triple-clause evaluation chooses the cheapest index given the bound terms.

Negation, disjunction, and predicates are evaluated **after** their dependencies are bound — the planner reorders if necessary.

### 2.8 Clause ordering

For Phase 7 ship a naive left-to-right execution with two passes:

1. **Static reorder**: push predicates and negations after the clauses that bind their variables. Push disjunctions last if they're not required earlier.
2. **Heuristic cost**: for triples, estimate selectivity per index (cheap if AVET/EAVT with bound e/a/v; expensive if AEVT range scan with only `a` bound).

No true optimizer in Phase 7. Add cost-based join ordering later if profiling demands.

### 2.9 Index selection

```
Bound terms      | Index | Operation
-----------------|-------|---------------------------
e                | EAVT  | walk e's attribute map
a                | AEVT  | walk a's entity map
v                | VAET  | walk v's attribute map
e, a             | EAVT  | direct cell lookup
a, v             | AVET  | direct entity set
v, a             | VAET  | (same as above, different shape)
e, v             | EAVT  | walk e's attrs, filter by v
e, a, v          | EAVT  | hasFact presence test
(none)           | AEVT  | full scan, MARK SLOW
m (module col)   | MEAT  | module-scoped variants of above
```

The `m` column lets queries scope by module — useful for "what facts did module X assert that match pattern Y."

### 2.10 Set semantics

Results are sets, not bags. Duplicate bindings collapse. Implementation: post-process the binding-set through a HAMT-keyed-by-binding deduplication, or maintain set invariant during join.

Datomic and most Datalogs use set semantics by default. Bag semantics is opt-in via `find-list` form; defer to a later phase.

### 2.11 Result materialization

For single-var projection: RRB of Refs.

For multi-var projection: RRB of tuples. A tuple is itself a materialized object (RRB or fixed-arity struct, depending on size).

For aggregates (Phase 7.2): a single value or per-group RRB.

For pull (Phase 7.3): a materialized map per binding.

All results are first-class Refs that can be sent further messages: `[result count]`, `[result at: 0]`, etc.

### 2.12 IdApi additions (Phase 7)

```zig
// Query construction
make_query: fn (var_count: u32) -> *Query;
query_add_triple: fn (*Query, e: Term, a: Term, v: Term) -> void;
query_add_negation: fn (*Query, clauses: []const Clause) -> void;
query_add_disjunction: fn (*Query, branches: []const []const Clause) -> void;
query_add_predicate: fn (*Query, op: u8, lhs: Term, rhs: Term) -> void;
query_set_asof: fn (*Query, tx: u64) -> void;
query_set_since: fn (*Query, tx: u64) -> void;
query_add_find_var: fn (*Query, var_idx: u32) -> void;

// Execution
query_execute: fn (*Query) -> Ref;  // RRB of bindings
query_count: fn (*Query) -> u64;
query_first: fn (*Query) -> Ref;

// Result inspection
rrb_count: fn (Ref) -> u64;
rrb_at: fn (Ref, idx: u64) -> Ref;
rrb_iter: fn (Ref, callback, ctx) -> void;
```

### 2.13 Module shape

`libbirnam_query.dylib`:

- Mints `Query_vt`, `Result_vt` (for RRB-of-bindings results).
- Singleton `db` instance exposed as `[module :db <db_eid>]`.
- Methods on `db`:
  - `findAll: <find> where: <clauses>` → result Ref
  - `find: <find> where: <clauses>` → first binding or NIL
  - `count: <find> where: <clauses>` → SmallInt
  - `findAll: <find> where: <clauses> asOf: <tx>` (variants for since/between)

The module parses bracket-literal clauses from REPL input (using the Phase 6 parser, extended). For Phase 7 spike: simpler than full Smalltalk parsing — accept bracket-array of bracket-triples.

### 2.14 Phase 7 test coverage

- Identity: `[db findAll: ?e where: [[?e :name "Point"]]]` returns the right entities
- Multi-var: `[db findAll: #(?e ?n) where: [[?e :name ?n]]]` returns tuples
- Join: `[db findAll: ?m where: [[?vt :delegate ?p][?vt :name ?n][?p ?sel ?m]]]`
- Negation: vtables without `init`
- Disjunction: entities matching either name
- Predicate: `[db count: ?n where: [[?e :age ?n][?n < 30]]]`
- History: `asOf: <tx_before_change>` returns prior values
- Set semantics: duplicate-producing clause patterns collapse
- Multi-clause: full kernel reflection scenarios
- Failure modes: unbound negation, mismatched-binding disjunction

---

## 3. Phase 7.1 — Rules + Recursion

### 3.1 Rule representation

A rule is a parameterized query body. Stored as facts:

```
[rule_eid :rule-name "ancestor"]
[rule_eid :rule-params <param-vector-ref>]
[rule_eid :rule-body <body-clauses-ref>]
[rule_eid :rule-arity 2]
```

Multiple rules can share a name (definitional disjunction — Datomic-style):

```
[r1 :rule-name "ancestor" :rule-params #(?x ?y) :rule-body #([?x :parent ?y])]
[r2 :rule-name "ancestor" :rule-params #(?x ?z) :rule-body #([?x :parent ?y] (ancestor ?y ?z))]
```

Queries reference rules by name. The query planner resolves the name to the set of rule definitions at execution time.

### 3.2 Rule invocation in clauses

```zig
RuleInvocation = struct {
    name_eid: u64,    // EID of the rule-name string
    args: []const Term,
};
```

Surface: `(ancestor ?x ?z)` reads as a function-call form, distinguished from triples by lack of bracket and presence of round paren.

Birnam syntactic refinement: use bracket form with leading rule-name marker, e.g. `[#ancestor: ?x with: ?z]` to keep one bracket-form-style throughout. The exact surface is a Phase 10 question; semantics here are Datomic-compatible.

### 3.3 Stratification

A rule program is **stratified** if no rule recurses through a `not` clause. Birnam implements stratified Datalog: queries containing rules are checked at planning time. Non-stratified programs are rejected with an error.

The check:

1. Build the rule-dependency graph: rule R depends on rule S if any clause in R's body invokes S.
2. Annotate edges with "negative" if the invocation is inside a `not` clause.
3. Reject if any cycle in the graph contains a negative edge.

### 3.4 Semi-naive evaluation

The standard fixpoint algorithm:

1. Start with empty derived set Δ for each rule.
2. For each iteration:
   - Apply each rule using base facts ∪ Δ_previous.
   - New tuples = result − Δ_previous.
   - If empty for all rules, terminate.
   - Else Δ ← Δ ∪ new; repeat.
3. Project results over the original query.

Optimizations (magic sets, demand-driven evaluation) are deferred. Naive semi-naive is correct, just slow for large derivations.

### 3.5 Termination

Pure Datalog is decidable — every query terminates because the Herbrand universe is bounded by the facts in the store. The semi-naive fixpoint reaches a fixed point in finite iterations.

Adding aggregation, arithmetic, or non-stratified negation breaks decidability. Phase 7.1 keeps these out; predicates (Phase 7) can compare bound values but not introduce unbound ones.

User-set safety bound: max iterations / max derived facts as a query option, default 10^6. Hitting it raises an error rather than running forever.

### 3.6 Tests

- Transitive closure: ancestor / descendant
- Delegation chain: `[(delegateAncestor ?child ?ancestor) ...]`
- Mutual recursion: even/odd via integer steps
- Stratification rejection: rule with `not` recursion → error
- Performance bound: ridiculous fixpoint hits safety limit and errors

---

## 4. Phase 7.2 — Aggregation

### 4.1 Aggregates in find clause

Datomic-style. A find element can be an aggregate over a variable:

```
[db findAll: #(?vt (count ?m))
     where: #([?vt ?sel ?m])]
```

This returns tuples `(vt, method_count)` with implicit group-by on `?vt` (the non-aggregated find variable).

### 4.2 Supported aggregates

| Name | Semantics |
|---|---|
| `count` | number of values |
| `count-distinct` | distinct value count |
| `sum` | numeric sum |
| `min`, `max` | comparable min/max |
| `avg` | numeric avg |
| `collect` | gather into an RRB |
| `sample` | one arbitrary value |

Numeric aggregates require SmallInt operands; raise on non-numeric. `min`/`max` work on SmallInts and EIDs (lexicographic on tag-then-payload).

### 4.3 Group-by semantics

Implicit: any non-aggregated find variable is a group-by key. Result tuples have one row per distinct combination of group-by values.

Explicit `groupBy:` keyword is unnecessary in Datomic-style; we keep that convention.

### 4.4 Implementation

After producing the raw binding-set:

1. Partition bindings by their projection onto the non-aggregate find vars.
2. For each partition, apply the aggregate function to the bound values of the aggregate's variable.
3. Emit one tuple per partition.

### 4.5 Tests

- `(count ?m)` over `[?vt ?sel ?m]` for a known vtable
- `(sum ?v)` over numeric attribute
- `(max ?tx)` over history facts
- `(collect ?v)` returns an RRB of all values
- Multi-aggregate: `#(?vt (count ?m) (min ?tx))`

---

## 5. Phase 7.3 — Subqueries, Saved Queries, Pull

### 5.1 Query as a Ref

A Query becomes a materialized object. Its EID is a stable handle. Queries can be:

- Built once and reused.
- Stored as facts.
- Composed: `findAll: ?x where: #(... [in: ?other_query_result ?x] ...)`.
- Sent messages: `[q execute]`, `[q withInput: x]`.

### 5.2 Saved queries

```
[q1 :query-name "active-vtables" :query-spec <spec-ref>]
```

Resolved by name at the call site:

```
[db run: 'active-vtables']
[db run: 'active-vtables' withArgs: #(<arg>)]
```

Saved queries are themselves queryable: `[db findAll: ?q where: #([?q :query-name ?n])]`.

### 5.3 Subqueries

A clause can reference another query's result:

```
[db findAll: ?vt where: #(
    [?vt :name ?n]
    [in: <subquery-ref> ?n]    # ?n must appear in subquery's result column
)]
```

The subquery executes first; its result becomes a virtual table feeding the outer query.

### 5.4 Pull syntax

Pull returns nested entity-attribute maps instead of flat bindings:

```
[db findAll: (pull ?vt #(:name :delegate (#methods [:sel :body])))
     where: #([?vt :name "Point"])]
```

Result: for each matching `?vt`, an object with:
- `:name` set to its name attribute
- `:delegate` set to the EID Ref of its delegate
- `:methods` set to an RRB of submaps, each with `:sel` and `:body`

Pull is sugar over multiple queries: the pull spec is expanded into a tree of fetches, executed eagerly, materialized as a tree of objects.

### 5.5 Tests

- Saved query: assert + resolve + execute round-trip
- Subquery: pipeline two queries, verify identical result vs single big query
- Pull: vtable with methods returns nested structure
- Pull with recursion: `(pull ?vt #(:name (:delegate ...)))` returns nested delegate chain

---

## 6. Phase 7.4 — Predicates and Functions

### 6.1 Inline predicates beyond inequalities

User-defined predicates registered as facts:

```
[primeQ_eid :predicate-name "primeQ" :predicate-impl <closure-ref>]
```

Usage in queries:

```
[db findAll: ?n where: #(
    [?e :value ?n]
    [(primeQ ?n)]
)]
```

The predicate closure receives the bound values and returns true/false.

### 6.2 Computed bindings (functions)

A clause of the form `[(<fn> <args>) <target-var>]` invokes the function on bound args, binds the target var to its result:

```
[db findAll: ?b where: #(
    [?e :age ?a]
    [(+ ?a 1) ?b]
)]
```

Built-in functions: arithmetic (`+`, `-`, `*`, `/`, `mod`), comparison-as-result (`min2`, `max2`), string ops (`concat`, `substring`).

User functions registered as facts, same shape as predicates but returning a Ref.

### 6.3 Tests

- User predicate: registered, queried, hit/miss cases
- Built-in function: arithmetic over bound vars
- Function chain: `[(+ ?a 1) ?b] [(* ?b 2) ?c]`
- Predicate over function result: `[(+ ?a 1) ?b][?b > 10]`

---

## 7. Cross-Phase Concerns

### 7.1 Query parsing surface

Phase 7's REPL/dylib needs to accept query clauses. Two options:

- **A.** Build-time API: tests construct Query structs via Zig helpers; user queries go through the REPL's tiny parser (extended) to map bracket-array of bracket-triples → Query.
- **B.** Defer real query parsing to Phase 10 (full language); Phase 7 only accepts Zig-built Queries.

Lean: **A** for Phase 7 — basic bracket-array parsing in the query module, extended in Phase 10.

### 7.2 Variable representation

Phase 7 uses var-index integers (per-query). Variable names exist only for error reporting. Pattern-send parser maps `?name` to an index per occurrence-in-query.

Phase 7.1 rules introduce parameter binding: `(ancestor ?x ?z)` invocation maps caller's `?x`/`?z` to the rule's parameter vars by position.

### 7.3 Performance

Phase 7 ships with naive nested-loop join. Acceptable for small queries (<1000 binding sets). Phase 7+ optimizations as workload demands:

- Hash join for large intermediate sets
- Index intersection (when multiple clauses bind the same variable)
- Statistics-driven join reordering
- Magic sets for recursive queries (Phase 7.1+)
- Compiled query plans cached by query identity (Phase 7.3+)

### 7.4 Set vs bag semantics

Set by default. Bag semantics for `collect` aggregate and a future `findVector:` opt-in. Phase 7 doesn't ship bag mode.

### 7.5 Mcache and query interaction

A query that hits `(?, sel_eid, ?)` doesn't invalidate mcache — query is read-only. Only `assert`/`retract` invalidate mcache. Standard.

### 7.6 Module ownership of derived facts (Phase 7.1)

Rules compute new tuples but don't assert them as persistent facts. Derivation is transient — computed on demand per query, not stored. (Some Datalogs persist via materialized views; defer for birnam.)

### 7.7 Snapshot interaction

Saved queries are facts → survive snapshots. Query execution is pure over fact-store state → reload of snapshot gives same results.

### 7.8 Error handling

Errors raised:
- Unsafe negation: variable not bound when negation is reached.
- Disjunction branch arity mismatch.
- Non-stratified rule program (Phase 7.1).
- Unbound predicate operands (Phase 7).
- Type mismatch in predicate (Phase 7) / aggregate (Phase 7.2).
- Termination safety bound exceeded (Phase 7.1).
- Subquery returns variable not used by outer query.

All errors are diagnostic objects (entity with `:error-kind`, `:error-message`, `:error-clause`). User code can pattern-match on them via... queries.

### 7.9 Surface syntax: full design

The pattern-send surface (locked) covers Phase 7 cleanly. Rule definitions (Phase 7.1) use cascaded asserts:

```
[db rule: 'ancestor' params: #(?x ?y) body: #(
    [?x :parent ?y]
)]
[db rule: 'ancestor' params: #(?x ?z) body: #(
    [?x :parent ?y]
    [#ancestor: ?y with: ?z]
)]
```

Aggregates (Phase 7.2): inside find vector via bracket form `[count: ?m]` or `[(count ?m)]` — pick during Phase 7.2 design.

Pull (Phase 7.3): `[(pull ?e <spec>)]` — clarify syntax against birnam's bracket idiom in 7.3 design.

---

## 8. Open Design Decisions

Items to resolve before each phase starts. Tagged by phase.

### 8.1 Phase 7

1. **Variable index assignment** — parser-time or query-build-time? Lean: query-build (incremental as clauses added).
2. **Clause encoding in IdApi** — extern struct per clause (variable size, awkward C ABI) or opaque builder with handle? Lean: opaque builder; modules call `query_add_triple(q, ...)` instead of constructing structs.
3. **Result Ref identity** — every query returns a fresh result, or queries are pure and result Refs can be cached? Lean: fresh — caching is Phase 7.3+ via saved queries.
4. **Inequality on what types** — SmallInts only, or any Ref via tag-aware comparison? Lean: SmallInts now, generalize later.
5. **History query default** — `asOf: <latest_tx>` is the default; should we also support `:all-history` modifier returning every (e, a, v, tx, op) tuple ever asserted? Lean: yes, as a separate `historyOf:` method.
6. **MEAT-scoped query syntax** — `[db findAll: ?m where: <clauses> inModule: <module-eid>]`? Or `<clauses>` include a `m:` constraint per-triple? Lean: `inModule:` modifier; per-triple module filter is Phase 7.4 territory.

### 8.2 Phase 7.1

1. **Rule definition syntax** — assert-and-name as in §7.9, or `[db defineRule: <ast>]`? Lean: assert-and-name.
2. **Rule dispatch with no matches** — silent empty or explicit error? Lean: silent empty (matches Datomic).
3. **Stratification check timing** — at rule-assert time (eager) or at query-execute time (lazy)? Lean: lazy — programs may temporarily violate stratification during edits.
4. **Magic sets** — Phase 7.1 ships semi-naive only. Magic sets as Phase 7.1.x optimization later.

### 8.3 Phase 7.2

1. **Aggregate over set or bag** — distinct values within a group or with duplicates? Lean: distinct (matches set semantics globally). Bag aggregates as opt-in.
2. **Type coercion** — `(sum ?v)` where some `?v` are SmallInt and some are strings: error or skip non-numeric? Lean: error (type-strict).

### 8.4 Phase 7.3

1. **Pull recursion depth** — bounded explicitly in pull spec, or run-until-empty? Lean: bounded with explicit `...:` recursion marker, max depth a query option.
2. **Saved query argument binding** — positional or named? Lean: positional matching the rule-params convention from Phase 7.1.
3. **Subquery result variable shape** — must be single-var, or multi-var allowed? Lean: multi-var via tuple matching.

### 8.5 Phase 7.4

1. **Predicate registration scope** — module-tagged like other facts? Lean: yes, predicates are facts → owned by their asserting module → unload retracts them.
2. **Function purity requirement** — enforce side-effect-free? Lean: convention only; not statically checked.
3. **Predicate composition** — predicates within predicates? Lean: no recursion guard, but predicates must terminate (user responsibility); termination not formally checked.

---

## 9. Implementation Notes Per Phase

### 9.1 Phase 7

```
src/kernel/query.zig (~350 LOC)
  - Query, Term, Clause types
  - Builder API
  - Index selection table
  - Join engine
  - Negation + disjunction + predicate evaluators
  - History view wrappers

src/kernel/result.zig (~80 LOC)
  - Materialized RRB-of-Refs result type
  - Multi-var tuple results

src/kernel/id_api.zig (+~80 LOC)
  - Query builder + execute wrappers
  - Result iteration

src/modules/query/query.zig (~250 LOC)
  - db singleton + methods
  - Bracket-array parser
  - Method dispatch for findAll:/find:/count: family

src/modules/query/query_test.zig (~250 LOC)
```

### 9.2 Phase 7.1

```
src/kernel/rule.zig (~200 LOC)
  - Rule type, registry, name → rule-set lookup
  - Stratification checker
  - Semi-naive fixpoint loop
  - Termination guards

src/kernel/query.zig (+~100 LOC)
  - Rule-invocation clause handling
  - Recursion-aware planner

(test additions ~150 LOC)
```

### 9.3 Phase 7.2

```
src/kernel/aggregate.zig (~150 LOC)
  - Aggregate function registry
  - Group-by engine
  - Numeric/string operations

src/kernel/query.zig (+~50 LOC)
  - find-aggregate parsing & integration
```

### 9.4 Phase 7.3

```
src/kernel/saved_query.zig (~100 LOC)
  - Query as Ref
  - Name → query lookup
  - Subquery integration

src/kernel/pull.zig (~150 LOC)
  - Pull spec parser
  - Tree-walker / materializer
```

### 9.5 Phase 7.4

```
src/kernel/predicate.zig (~80 LOC)
  - Predicate registry
  - Function registry
  - Built-in arithmetic / comparison / string fns

src/kernel/query.zig (+~50 LOC)
  - Predicate / function clause evaluators
```

---

## 10. Estimated Scope

| Phase | New LOC | Test LOC | Build time | Est days |
|---|---|---|---|---|
| 7 | ~750 | ~250 | 1-2 ms | 3-5 |
| 7.1 | ~300 | ~150 | +1 ms | 2-3 |
| 7.2 | ~200 | ~100 | +1 ms | 1-2 |
| 7.3 | ~250 | ~150 | +1 ms | 2-3 |
| 7.4 | ~130 | ~80 | trivial | 1-2 |

Total: ~1600 LOC kernel + ~730 LOC tests for the full Datalog system, delivered in five phases over ~10-15 working days.

---

## 11. What's NOT Included

To name the omissions explicitly:

- **Magic sets / demand-driven evaluation** — top-down query rewriting that prunes irrelevant facts. Defer until naive evaluation is the bottleneck.
- **Stable model semantics for non-stratified programs** — birnam rejects non-stratified rule sets rather than computing well-founded models.
- **Tuple-based query languages (SQL-style)** — not in scope. Datalog is the surface.
- **Materialized views** — derived rule facts are transient. Materialization is a future feature, requires invalidation machinery.
- **Distributed query execution** — single-process image; concurrency is deferred.
- **External data sources** — Datomic's `:in $rules ?name` supports multiple databases. Birnam has one fact store per image; not needed yet.
- **Query optimization beyond naive index selection** — cost-based reordering, statistics, plan caching. Future.

These are real Datalog capabilities. They're recorded here so we don't pretend they're done.

---

## 12. Open Architectural Questions

These cross all phases and need answers before Phase 7 starts.

1. **Is a query a runtime object or a parse-time construct?** Lean: runtime object. Allows saved queries (Phase 7.3) without redesign. Performance cost: query construction overhead per execution. Mitigate via query caching when 7.3 lands.

2. **Where does the bracket-array parser live?** In the query module or shared utility for Phase 10? Lean: query module for now; Phase 10 absorbs and generalizes.

3. **How does negation/disjunction interact with as-of history?** Negation in past time: "vtables that did NOT have `init` at tx N." Disjunction across time? Lean: history modifiers apply to the whole query uniformly; per-clause history modifiers are Phase 7.3+.

4. **Result identity for caching** — two structurally equal queries return the same result Ref? Lean: no — every execution is a fresh result. Equality is by content, not identity. Cached saved-query results (Phase 7.3) are stale-vs-current.

5. **What's the syntax for the rule recursion form?** `(name args)` (Datomic) or `[#name: ...]` (birnam bracket idiom)? Lean: bracket form with selector convention `[#name: arg with: arg]`. Surface-level Phase 10 decision; semantics here are language-independent.

6. **Should the find clause support both single-var and tuple syntax in one call?** `findAll: ?x` and `findAll: #(?x ?y)`. Lean: yes — the parser distinguishes based on whether the arg is a variable or an array.

---

## 13. Summary

Datalog in birnam is a five-phase delivery yielding Datomic-level expressiveness:

- **Phase 7** ships the relational core, sufficient for ~80% of reflection queries (joins, projections, negation, disjunction, predicates, history).
- **Phase 7.1** adds rules and recursion, enabling transitive properties and rich derivations.
- **Phase 7.2** adds aggregation with implicit group-by.
- **Phase 7.3** adds composition: subqueries, saved queries, and pull syntax.
- **Phase 7.4** rounds out with user predicates and computed bindings.

Each phase is implementable in isolation but designed in coherence with the others. The total is the honest minimum for "Datalog as first-class language feature in birnam" — without it, the relational substrate becomes a fancy hashtable, and reflection has no language.

Surface (pattern-sends) is locked. Semantics (this document) are locked once reviewed. Implementation phases proceed in numerical order.
