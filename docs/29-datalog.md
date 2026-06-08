# Datalog: Queries, Rules, Pull, Predicates, Aggregates

The query engine is how Birnam introspects itself — "reflection is query" (tenet
6). This page covers the implementation across `query.zig`, `rule.zig`,
`pull.zig`, `predicate.zig`, `aggregate.zig`, `saved_query.zig`, and the
`modules/query` surface. For the surface *language* (pattern-sends, the full
semantics), see `DATALOG.md`.

> Implementation: `src/kernel/{query,rule,pull,predicate,aggregate,saved_query}.zig`,
> `src/modules/query/query.zig`. Surface design: `DATALOG.md`. Concept:
> [The Fact Model](11-fact-model.md).

## Query representation — `query.zig`

A query is a `find` (what to project) over a `where` (a list of clauses).

```zig
const Term = union(enum) { constant: Ref, variable: u32, wildcard };
const FindElement = union(enum) { var_idx: u32, aggregate: AggregateSpec };
```

Variables are integer indices allocated by `Query.variable(name)`. A `Binding` is
a flat `[]Ref` indexed by variable, with a `UNBOUND` sentinel. Clause kinds:

- **Triple** — `e`/`a`/`v` terms, plus an optional `as_var` to reify the matched
  fact's eid.
- **PredicateClause** — `eq`/`ne`/`lt`/`le`/`gt`/`ge` over two terms.
- **Negation** — inner clauses that must produce *no* results (negation as
  failure).
- **Disjunction** — branches whose result sets union.
- **RuleInvocation** / **Subquery** / **PredCall** (boolean) / **FnCall**
  (computed binding).

History modifiers (`asof`, `since`, `between`, `in_module`) scope a query in time
or to a module.

## Evaluation

Triple evaluation chooses an index by what's bound:

- **e + a bound** → direct `currentValue` (handles lazy fact-eid reification).
- **e bound, a free** → walk `attributesOf(e)` (asOf-aware via
  `currentValueAsOf`).
- **a bound, e free** → AEVT walk via `entitiesWith(a)`.
- **a + v bound** → AVET via `entitiesMatching`.
- **nothing bound** → full scan, deferred with a warning.

`evalAllClauses` is a nested-loop join: it threads the binding list through the
clauses, each clause extending or filtering bindings (a Cartesian product across
the where-list). The final projection applies `find`; without aggregates it runs
a **set-semantics dedup** (content-hash per tuple) — no bag semantics, per the
Date discipline. Negation passes a binding iff the inner clauses are empty;
disjunction unions the branches; predicates filter.

## Rules and recursion — `rule.zig`

```zig
const Rule = struct { n_params: u32, body: []const Clause, max_var: u32 };
```

Rules are stored as fact-entities (`:rule-name`, `:rule-params`, `:rule-max-var`,
`:rule-body`) in a `Registry`; `define`/`lookup` go through facts.
`renameClauses` offsets variable indices for per-invocation freshness.
`checkStratified` builds the rule-dependency graph and rejects negative cycles
(no unstratified negation). Recursive rules are evaluated **semi-naively**: a
fixpoint loop materializes derived tuples into a content-hash `DerivedSet`,
iterating until no new tuples appear, bounded by `max_iter` (100) and `max_tuples`
(1M) safety caps.

## Pull — `pull.zig`

Pull projects hierarchical results from an entity. A `PullElement` is an `attr`
(fetch a leaf), a `recurse` (walk an eid-valued attribute up to a depth, with
cycle detection), or a `sub` (fetch an eid attribute then pull *that* entity with
a nested spec). Results are tagged (`leaf`, `nested`, `null_value`,
`cycle_marker`, `depth_marker`) — `null_value` rather than NIL, per the No-NULL
discipline. A visited set tracks the current path; default `max_depth` is 100.

## Predicates and functions — `predicate.zig`

`resolvePred`/`resolveFn` look up an entity by `:predicate-name`/`:fn-name` and
return a native box (`NativePredFn`/`NativeFnFn`, C calling convention) or a
closure ref (user-defined, deferred). Built-ins include arithmetic (`+ - * /
mod`, SmallInt) and `min2`/`max2`, registered at bootstrap. A predicate returns
`bool` (filter); a function returns a `Ref` (computed binding via `FnCall`).

## Aggregates — `aggregate.zig`

```zig
const AggregateKind = enum { count, sum, min, max, avg, collect };
```

`compute(kind, values)` runs over already-deduplicated values (distinct by
default, per Date). `sum`/`min`/`max`/`avg` require SmallInts (else
`NonNumericInAggregate`); `collect` builds an RRB wrapped in an `RRB_VT` object.
Group-by partitions bindings by the non-aggregate `find` positions; if every
find element is an aggregate, there is one universal group.

## Saved queries — `saved_query.zig`

A saved query is an entity with `:saved-query-name` and `:saved-query-spec` (a
pointer Ref to a `*Query`). `saveQuery(name, q)` registers or updates it (latest
wins); `lookupQuery(name)` returns the `*Query`. The registry takes ownership.

## The language surface — `modules/query/query.zig`

The query module exposes the pattern-send surface to Birnam. `db` answers
`execute:` (wrapping a built `*Query` and returning a `Result` object that
answers `count`/`arity`/`at:`). The C `IdApi` surface
([Modules](2c-modules.md)) provides the builder calls
(`make_query`, `query_add_triple`, `query_add_predicate`, `query_add_find`,
the negation/disjunction begin/end pairs, `query_add_find_aggregate`,
`query_set_asof`/`since`/`between`/`in_module`, `query_execute`) and the RRB
accessors (`rrb_count`/`rrb_at`) for `collect` results. The kernel-reserved
`RRB_VT` gets `count`/`at:` methods so aggregate results are sendable objects.

## Where to go next

- The surface language and semantics in full: `DATALOG.md`.
- Driving queries from the REPL: [Querying Facts](33-querying.md) (Part 3).
- The indexes queries walk: [The Fact Store](22-fact-store.md).
