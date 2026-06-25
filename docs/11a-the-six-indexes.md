# The Six Indexes: How Facts and Objects Are Stored

Birnam keeps everything in **six** indexes. Five hold *facts*; the sixth holds
*objects*. This page builds all six from first principles — starting from "what
is a fact and what do you ask of it" and arriving at exactly the structures the
kernel maintains, and why there are six and not three or ten.

> Implementation: `src/kernel/fact_store.zig` (the five fact indexes),
> `src/kernel/object_table.zig` (the sixth), `src/kernel/fact.zig` (the cells),
> `src/kernel/hamt.zig` (the shared substrate). Concepts:
> [The Fact Model](11-fact-model.md), [Reification](12-reification.md),
> [Persistent Structures](16-persistent-structures.md),
> [The Performance Model](18-performance-model.md).

---

## 1. Why index at all

A fact is a tuple. The store holds millions of them. Every interesting operation
is really a *question* over that pile:

- "What do I know about entity 4271?"
- "Which entities have `:email = "a@b.com"`?"
- "What refers to this entity?"
- "Which facts belong to the module I'm unloading?"

A flat list answers all of these — by scanning all N facts every time. That is
O(N) per question, which is hopeless at scale. The fix is the same one every
database uses: keep the facts in a structure **sorted/keyed by the thing you
search on**, so the answer is a descent to a sub-tree instead of a scan.

But here is the catch that produces *multiple* indexes: **a structure keyed by
one column only answers questions that start with that column.** A map keyed by
entity finds "everything about entity e" instantly, but to answer "who has value
v" it must still scan every entity. Each *question shape* wants a *different key
order*. So you don't keep one index — you keep one **per class of question**, and
each is a complete copy of the facts under a different key ordering (a *covering*
index). The cost is writing every fact into several trees; the payoff is that
every supported question is a descent, never a scan.

The only design choices left are: **which key orderings are worth maintaining?**
Each answer below is one index. Birnam's answer is five, plus a sixth for a
different job entirely.

---

## 2. The coordinates of a fact

A datom carries six columns (see [The Fact Model](11-fact-model.md)):

```
[ e   a   v  |  tx   op   m ]
  entity      attribute  value     transaction  assert/retract  module
```

`tx`/`op` are *bookkeeping* — they ride inside each stored cell as history, not
as a key you search by. The columns you actually *index on* are four: **e, a, v,
m**. The five fact indexes are five useful orderings of those four; the choice of
ordering is driven entirely by the question each must answer.

---

## 3. The five fact indexes, derived one question at a time

Each index is **nested HAMTs** — a tree of maps, one level per key column, ending
in a *cell* (§5). Read `HAMT<e, HAMT<a, …>>` as "key by e, then within that key
by a."

### EAVT — "what do I know about this entity?"
The default question. To gather every attribute/value of one entity in one
descent, key by **entity first, then attribute**:

```
EAVT : HAMT<e, HAMT<a, ValueCell>>
```

This is the entity walk — the shape an object's slots come from, the shape
`attributesOf(e)` iterates. It is the closest thing to "a row" in a system that
has no rows.

### AEVT — "tell me about this attribute across everything"
Schema-level and column-level questions: every entity that has attribute a, every
value of a, "walk the `:name` column." Key by **attribute first, then entity**:

```
AEVT : HAMT<a, HAMT<e, ValueCell>>
```

Same `ValueCell` payload as EAVT, mirrored ordering. This backs `entitiesWith(a)`
and — importantly — value-bound queries: to find "entities where a = v" with the
entity unbound, the engine walks AEVT[a] and filters by value (it does **not**
need a value-keyed index for this; see [the datalog engine](29-datalog.md)).

### AVET — "which entities have attribute a equal to value v?"
Equality lookup, uniqueness checks, and sorted-by-value access. This question
starts with *two* columns bound (a and v) and wants the matching entities, so key
by **attribute, then value, then entity**:

```
AVET : HAMT<a, HAMT<v, HAMT<e, PresenceCell>>>
```

Three levels deep: it is one of the two *costly* indexes. The payload is a
`PresenceCell` (§5), not a value — at the (a, v, e) leaf the value is already in
the key, so all that remains to record is *is this fact present right now*. AVET
backs `entitiesMatching(a, v)`.

### VAET — "what points at this entity/value?"
The reverse-reference walk: given a value (typically an entity-id used as a
reference), find every fact whose value is it. Key by **value first**:

```
VAET : HAMT<v, HAMT<a, HAMT<e, PresenceCell>>>
```

The mirror of AVET, also three levels, also `PresenceCell`. It answers "who
references X" — graph-backwards navigation — and is read by `pointingAt(v)`.

### MEAT — "which facts belong to this module?"
Provenance. Every fact is asserted *by* some module (kernel bootstrap, a loaded
dylib, user code under `M_USER`). To unload a module you must find and retract
exactly its facts; to snapshot only user data you must select by provenance. Key
by **module, then entity, then attribute**:

```
MEAT : HAMT<m, HAMT<e, HAMT<a, ValueCell>>>
```

`ValueCell` payload (the value matters here), three levels. MEAT is what makes
module load/unload and the image writer's "persist only `M_USER`" possible —
provenance is a first-class index, not a side annotation.

### Why these five and no others
e, a, v, m give 4! = 24 possible orderings; Birnam keeps the five that each pay
for a distinct, real question shape — entity walk (EAVT), schema walk (AEVT),
value equality (AVET), reverse refs (VAET), provenance (MEAT). The remaining
orderings answer nothing the five don't already cover by descent-plus-filter, so
maintaining them would be pure write cost. Five is the minimal covering set for
the questions the system actually asks.

---

## 4. The sixth index: the object table

The five above store *facts*. But Birnam is also an object system: to **run**, it
sends messages, and dispatch needs the concrete in-memory object behind an
identity *now* — its vtable, its slots — without reconstructing it from facts on
every send. That is a different question with a different answer:

> "Given entity id `e`, where is its materialized object?"

```
Object table : HAMT<eid, Ref>     // src/kernel/object_table.zig
```

Keyed by eid, valued by a `Ref` — a tagged pointer to the materialized
`[vtable][slots…]` struct ([Refs](14-refs.md),
[Promotion/Demotion](25-promotion.md)). It is the index from **identity to live
representation**, and it completes the "fact/object" pairing: five indexes for
what is *true*, one for what is *running*.

Three properties make it the sixth index rather than a cache bolted on the side:

- **Sparse and read-through.** An eid is in the table only if it has been
  *materialized* (promoted because it got hot). Absence means "cold — go
  reconstruct from the fact store." So the object **table** (the eid→ptr map) is a
  projection, never an independent source of truth. Lose it and you lose nothing
  durable; rebuild it by re-materializing from EAVT.
- **It mints identity.** `alloc` hands out the monotonic eids every fact then
  refers to. Identity is decoupled from memory address precisely so a fact can
  name an entity (`e`, and any `v` that is a ref) stably across relocation,
  compaction, snapshot, and AOT freeze — the object table is where that
  indirection lives. **But the eid is a *handle*, not the anchor — see the
  amendment below.**

> **Amendment (ADR-image-1 D5.1, 2026-06-25) — identity is relative; the
> interned name is the durable anchor.** The "projection, rebuild from EAVT" claim
> above is true for the **eid→ptr map** and for **anonymous entities** (runtime
> objects with no name), but it is *not* the whole story for **named entities**.
> The integer eid is an *arbitrary, relocatable encoding* — assigned by load
> order, renumbered across an independent numbering — not meaning-bearing data.
> What is durable is the **interned name** (`[eid :name "…"]` in the interner's
> name↔eid bijection): identity is grounded **relationally**, not in the integer.
> Three tiers: **directly-named** (selectors, attributes, vtables — anchored by
> their own name); **relationally-named** (methods, constant pools, AST nodes —
> anonymous eids reached by a *name-path*, e.g. a method *is* what you get
> resolving selector S on vtable V, both named); **anonymous** (the eid simply
> *is* the identity, replayed verbatim within an image, not cross-image-portable).
> So promote the **name layer** (the interner) to the primary identity anchor; the
> eid stays a fast per-numbering handle. Nothing becomes mutable; the object table
> stays a projection. See ROADMAP.md ADR-image-1 §D5 / §D5.1.
- **Same substrate.** It is a HAMT too (§5), so it snapshots and shares structure
  like the other five.

The relationship is the whole performance story
([The Performance Model](18-performance-model.md)): **the five fact indexes are
the durable, queryable, time-traveled truth; the object table is the fast mutable
projection you actually compute against.** Promotion fills it; demotion writes
changes back as facts and drops the entry.

---

## 5. What lives at a leaf

The indexes differ not just in key order but in what they store at the bottom:

| Index | Leaf payload | Why |
|---|---|---|
| EAVT, AEVT, MEAT | **`ValueCell`** = `{ current_v: ?Ref, history }` | the value is the answer; keep it plus its timeline |
| AVET, VAET | **`PresenceCell`** = `{ present: bool, history }` | the value is already in the key — only presence remains to record |
| Object table | **`Ref`** | a raw tagged pointer to the live object |

`current_v` is `?Ref` because absence is a real state (a retracted or
never-asserted fact) — Birnam has no NULL value; absence rides the *shape* of the
cell, not a sentinel ([Refs §No-NULL](14-refs.md)). The `history` vector (an RRB,
see [Persistent Structures](16-persistent-structures.md)) is what makes
`asOf:`/`history` possible: every assert/retract appends `{tx, op, basis}` so any
past state is reconstructable. Time-travel is not a separate index — it is a
column *inside* every cell of the five fact indexes.

---

## 6. Why all six are HAMTs

Every index — the five fact trees and the object table — is a **Hash Array
Mapped Trie** ([HAMT and RRB](23-hamt-rrb.md)), and that is not incidental. A
write does not mutate the tree in place; it **path-copies** the spine from the
touched leaf to the root and returns a new root that *shares* every untouched
sub-tree with the old one (structural sharing). Two consequences fall out for
free, and they are exactly what a historied, snapshot-able fact store needs:

- **Snapshots are O(1).** Capturing the five roots by value is a complete,
  immutable view of the whole store — a query or a `asOf:` reader holds old roots
  while writers advance new ones, no copying, no locking.
- **History is cheap to retain and cheap to drop.** Old roots remain valid until
  nothing references them; a compaction rebuilds into a fresh epoch arena and
  frees the old one wholesale ([The Fact Store §Compaction](22-fact-store.md)).

The price is that each write touches several trees and abandons the old spine
nodes — which is the central cost the performance and load-memory work manages
(§7).

---

## 7. You do not always need all six

Maintaining six covering structures per fact is the system's dominant write cost,
so a natural question is: *does every fact need every index?* It does not, and the
store now exploits that.

- **Code facts skip what they never use.** AST nodes and bytecode are write-once
  and are never reverse-ref-walked, so they are dropped from **VAET** and from
  per-fact **history** entirely ([Code-Fact Tiers](19a-code-fact-tiers-plan.md)).
  They keep EAVT/AEVT/AVET/MEAT — everything that is actually read — and shed the
  two costs nothing reads. This is the same first-principles logic as §1, applied
  per attribute: *don't maintain an ordering for a question this fact will never
  be asked.*
- **The object table is populated lazily.** Only hot entities materialize; cold
  ones cost nothing until touched.

So the honest count is "up to six." Every fact lives in EAVT, AEVT, AVET, MEAT
and (if it is a normal data fact) VAET, with history in each cell; a hot entity
additionally has an object-table entry. The indexes are not a fixed tax — they
are a menu of question-answering structures, and the store maintains exactly the
ones a given fact's questions justify.

---

## 8. Summary

| # | Index | Key order | Answers | Leaf |
|---|---|---|---|---|
| 1 | **EAVT** | e → a | everything about an entity | ValueCell |
| 2 | **AEVT** | a → e | everything about an attribute (schema/column walk, value-bound queries) | ValueCell |
| 3 | **AVET** | a → v → e | which entities have a = v (equality, uniqueness) | PresenceCell |
| 4 | **VAET** | v → a → e | what references this value (reverse refs) | PresenceCell |
| 5 | **MEAT** | m → e → a | which facts a module owns (provenance, unload, snapshot) | ValueCell |
| 6 | **Object table** | eid → ptr | where the live object for this identity is | Ref |

Five answer *what is true*; the sixth answers *what is running*. All six are
HAMTs, so all six snapshot and share structure. Facts are the source of truth;
the object **table** (eid→ptr) is a projection of them. (Durable *identity*,
though, is the interned **name**, not the eid — the eid is a relocatable handle;
see §4's D5.1 amendment.) And no fact pays for an index that answers a question it
will never be asked.

> See also: [The Fact Store](22-fact-store.md) (the concrete API and write flow),
> [Promotion/Demotion](25-promotion.md) (how the object table fills and drains),
> [Time, History, and Temporal Queries](15-time-travel.md) (the history column),
> [The Performance Model](18-performance-model.md) (truth vs. projection cost).
