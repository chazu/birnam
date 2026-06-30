# The relational reading — datalog over the id-stack store

> **Status:** Shipped (2026-06-30). The full datalog surface on the libid refounding,
> in `src/id/relational.zig` (a standard module) over the rung-2 store
> ([Rung 2 — cells](33-rung-2-cells.md)). Realizes the North Star —
> [The Destination](30-the-destination.md) §D5.2 — "the object model and the
> relational model are one store read two ways." The old fact-store kernel's
> equivalent is [Datalog](29-datalog.md); this is the re-port onto the id stack,
> built send-by-send through the object model.

## What it is

The store (`store.zig`) holds facts and answers **point reads**
(`currentValue:attr:`). This module adds the **relational reading** — pattern
queries over the same facts. It is a sibling module that imports the running
`Store` and installs its query protocol onto the store's *own* vtable, so the one
well-known object answers both `assert:attr:value:` and `findAll:where:`. That
object is exported under the name the examples use: **`db`**.

Everything here is reached through `api.send` and loaded as the standard dylib
`libbirnam_id_relational` (siphon `script_modules`). No floor, codegen, or grammar
changes were needed — the query language rides the collection literals (`#[…]`),
logic-variable syntax (`?p`), and symbol literals (`#sym`) that already parse.

## The vocabulary at a glance

```
[db entity]                              a fresh blank entity (a unique identity)
[db assert: e attr: a value: v]          (inherited from the store)

[db findAll: ?v where: clauses]          a Vector of every distinct binding of ?v
[db findAll: ?v where: cl asOf: t]        … against the snapshot at basis t
[db findAll: ?v where: cl since: t]       … only triples that changed after t
[db findAll: ?v where: cl between: lo and: hi]   … a transaction window [lo, hi)
[db basis]                               the current tx counter (a time handle)

[db count: ?v where: clauses]            how many distinct bindings of ?v
[db sum:  ?v where: clauses]             ┐ fold the DISTINCT bindings
[db min:  ?v where: clauses]             │ (set semantics, per the Date discipline)
[db max:  ?v where: clauses]             │
[db avg:  ?v where: clauses]             ┘ mean (integer division truncates)

[db defineRule: #name params: #[?a …] body: #[ clause… ]]   define a (recursive) rule

[db pull: e spec: #[ #attr | #[#ref <subspec>] … ]]   materialize e into a record Map
```

## Clauses — the `where:` list

A `where:` value is a Vector of **clauses**. The engine begins with one empty
solution and threads it through the clauses left to right; each clause extends or
filters the running set of solutions. The result projects one logic variable,
deduped (set semantics).

A **logic variable** is written `?p` in source. The grammar/codegen lowers `?p` to
`[LogicVar named: #p]`, which is *canonical per name* — the `?p` in the result
position and the `?p` inside a clause are the **same object**, so the join matches
them by identity. A **constant** in a clause matches by identity too, which is why
example data uses interned symbols (`#Boston`) for queryable values rather than
strings (two `"Boston"` literals are distinct objects and never match).

The engine dispatches on each clause's shape (`applyClause`), in this order:

### Triple pattern — `#[ e a v ]`

The base clause: three positions (entity, attribute, value), each a constant or a
logic variable. Joins against every live triple in the store, binding variables
and requiring constants to match.

```
[db findAll: ?p where: #[ #[?p #city #Boston] ]]        // everyone in Boston
[db findAll: ?c where: #[ #[?p #city ?c] ]]             // the distinct cities
```

Two clauses sharing a variable are a conjunction (a join on that variable):

```
[db findAll: ?p where: #[ #[?p #city #Boston] #[?p #pet #Dog] ]]   // Boston dog-owners
```

### Predicate — `#[ lhs op rhs ]`

A clause whose **middle position is a comparison-op symbol** filters the current
solutions instead of joining; it binds nothing. Both spellings work — named
(`#lt #le #gt #ge #eq #ne`) and glyph (`#< #<= #> #>= #= #~=`). The comparison is
*sent* to the left operand (`lhs op rhs`, kept iff the result is `true`), so it
reuses the prelude's arithmetic and extends to any comparable type; a
non-comparable operand yields an error value (not `true`), so the row simply drops.

```
[db findAll: ?p where: #[ #[?p #age ?a] #[?a #gt 18] ]]   // adults
```

Because a predicate only *filters*, it must come **after** the clauses that bind
its variables (there is no clause-reorder planner yet).

### Negation — `#[ #not innerClause… ]`

Negation as failure: keep a solution iff threading it through the inner conjunction
yields **no** match. It binds nothing, so the negated variables should already be
bound by earlier clauses. The inner clauses run back through the same dispatch, so
a negated branch may itself contain predicates or further negation.

```
[db findAll: ?p where: #[ #[?p #city #Boston] #[#not #[?p #pet #Cat]] ]]   // Boston non-cat-owners
```

### Disjunction — `#[ #or branch… ]`

The union of the solutions each branch produces from the same incoming solutions.
Each branch is itself a clause (triple, predicate, negation, or a nested `#or`).

```
[db findAll: ?a where: #[ #[?p #age ?a] #[#or #[?a #lt 20] #[?a #gt 35]] ]]   // the extremes
```

### Rule invocation — `#[ #rule #name arg… ]`

Joins against a rule's relation (see below). Arguments are positional and may bind,
agree with an existing binding, or match a constant.

## Rules — `defineRule:params:body:`

```
[db defineRule: #ancestor params: #[?x ?y] body: #[ #[?x #parent ?y] ]]
[db defineRule: #ancestor params: #[?x ?y]
   body: #[ #[?x #parent ?z] #[#rule #ancestor ?z ?y] ]]
```

A rule name may have **several definitions**; the relation it denotes is their
**union** — so a base case and a recursive case are two `defineRule:` calls (the
datalog reading). Invoke a rule from any `where:` list with `#[ #rule #name arg… ]`.

Recursion is handled **set-at-a-time**, not by recursive descent: the engine
materializes a rule's head-tuple relation to a fixpoint, and a self-reference
during evaluation reads the *partial* set instead of recursing. Only head **values**
cross the rule boundary (into the derived set), never variables — so the
canonical-by-name logic variables never capture across recursion levels, and no
variable renaming is needed. A `max_iter` bound is a safety net (a finite store
reaches a fixpoint well before it). Self- and non-recursion are fully correct
(ancestor / transitive closure); mutual recursion is best-effort (see Deferred).

## Time travel — `basis`, `asOf:`, `since:`, `between:and:`

Every assertion advances the store's transaction counter. `[db basis]` captures the
current point as a **watermark** (the next-tx counter). The three time queries
scope a query to a transaction window `[lo, hi)` — the deciding fact for each
triple is the newest with `tx < hi`, and a triple is emitted only if that fact is
an assert with `tx >= lo`:

| query | window | meaning |
|-------|--------|---------|
| `asOf: t` | `[_, t)` | the snapshot live *before* anything committed at/after `t` |
| `since: t` | `[t, _)` | only triples whose current value was asserted at/after `t` (what changed) |
| `between: lo and: hi` | `[lo, hi)` | the snapshot at `hi` restricted to what changed at/after `lo` |

The whole query — joins, predicates, negation, disjunction, **rules** — runs against
the historical view (the snapshot is gathered once and flows through the engine).

```
let earlier := [db basis];
// … more assertions …
[db findAll: ?p where: #[ #[?p #city #Boston] ] asOf: earlier]   // who was there then
[db findAll: ?p where: #[ #[?p #city #Boston] ] since: earlier]  // who arrived since
```

A caveat from the store's design ([Rung 2 — cells](33-rung-2-cells.md) D6): a prior
**value** of a `:one` attribute is visible to `asOf:` only if that attribute opted
into history (`[a :history true]`); the default-off path overwrites in place to
avoid unbounded log growth. Entity and fact *additions* over time are always
visible regardless.

## Aggregates

`count:`/`sum:`/`min:`/`max:`/`avg:where:` run the same query, then fold the
**distinct** projected values — set semantics, per the Date discipline (equal
values count once). `sum` folds `+` from 0; `min`/`max` send `<`/`>`; `avg` is
`sum / count` (integer division truncates); `count` is just the size. An empty
query gives `count`/`sum` 0 and `min`/`max`/`avg` nil. There is no group-by yet
(one universal group). An aggregate over a historical snapshot is available
compositionally — e.g. `[[db findAll: ?v where: cl asOf: t] size]`.

## Pull — `pull:spec:`

`[db pull: e spec: aSpec]` materializes an entity into a record `Map`. Each spec
element is either a **leaf attribute** `#a` (→ its current value; a Vector of
members for a `:many` attribute) or a **nested ref spec** `#[ #refattr <subspec> ]`
(→ pull the entity the attribute points at with `<subspec>`; a Vector of
sub-records for a `:many` ref). Absent attributes are omitted (no NULL). Pull is
built on the join (each attribute's values come from `findAll: ?v where: #[ #[e a
?v] ]`) and reads cardinality from `[db cardinality: a]`.

```
[db pull: bob spec: #[ #name #[#friend #[ #name #city ]] ]]
  => {#name -> Bob, #friend -> {#name -> Alice, #city -> #Boston}}
```

## How it hangs together

```
runQuery(self, ?v, clauses, lo, hi)
  ├─ gatherTriples(store, …, lo, hi)   → the live (or historical) triple set
  ├─ for each clause: applyClause(…)   → #or → #not → #rule → predicate → triple
  └─ project ?v, dedup                 → a Vector

findAll/count/sum/min/max/avg          all wrap runQuery on the projection side
pull                                    recursive; gathers attribute values via runQuery
```

The store stays representation-free: it owns liveness semantics behind one
enumeration seam (`liveTriplesDo:` and its windowed siblings
`liveTriplesAsOf:/Since:/Between:and:do:`), and the engine accumulates through a
block, never touching the store's internal layout.

## Examples

- `examples/address-book.birnam` — `entity` / `assert:` / `findAll:where:` / `pull:`
- `examples/predicates.birnam` — predicates, negation, disjunction, aggregates
- `examples/ancestors.birnam` — recursive rules (transitive closure)
- `examples/time-travel.birnam` — `basis` / `asOf:` / `since:`
- `examples/pull.birnam` — nested-ref and `:many` pull

## Deferred (with rationale)

- **Group-by** — aggregates currently use one universal group. Grouping needs a
  find-spec surface (multiple find elements, partitioned by the non-aggregate
  positions); that is a larger surface change than a nicety, so it is its own arc.
- **`collect` aggregate** — redundant: `findAll:where:` already returns the Vector
  of distinct values.
- **Dedicated as-of aggregates** — available compositionally over
  `findAll:where:asOf:` (e.g. fold its result), so no separate selectors.
- **Rules as facts** — rules live in a module-side registry today. Storing them as
  facts (queryable, reflective) is a reflectivity refinement, not core.
- **Mutual recursion** — the per-rule fixpoint memo makes self- and non-recursion
  exact and keeps mutual recursion terminating, but mutual recursion is not
  guaranteed complete (it would want a stratified, whole-stratum fixpoint). The
  motivating cases (ancestor / transitive closure) are self-recursive and exact.
- **Clause-reorder planner** — predicates and negation must follow the clauses that
  bind their variables; there is no automatic reordering yet.
