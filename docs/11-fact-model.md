# The Fact Model

Everything in Birnam reduces to *facts*. This page defines what a fact is, how
facts are indexed, and the vocabulary (entity, attribute, value, transaction,
cardinality) the rest of the documentation assumes.

> Implementation: `src/kernel/fact.zig`, `src/kernel/fact_store.zig`,
> `src/kernel/kernel_eids.zig`. Design: `VISION.md` §Data Model.

## The six-column tuple

A fact is a six-column tuple:

```
[ e   a   v   tx   op   m ]
  |   |   |   |    |    |
  |   |   |   |    |    source module EID
  |   |   |   |    assert | retract
  |   |   |   monotonic transaction counter (u64)
  |   |   typed value: ref | int | str | bytes | keyword | bool — NEVER NIL
  |   attribute (itself an entity; schema lives in the db)
  entity id (monotonic u64, decoupled from memory address)
```

- **`e` — entity.** A `u64` identity (an *eid*). Not a memory address; it indexes
  into the object table. Stable across compaction, relocation, snapshot, and AOT
  freeze.
- **`a` — attribute.** Also an eid. Attributes are entities, so the schema is
  itself facts (self-describing). Selectors double as attributes — already
  interned, so they cost nothing extra.
- **`v` — value.** A tagged [`Ref`](14-refs.md). One of six primitive shapes:
  `ref`, `int`, `str`, `bytes`, `keyword`, `bool`. **A value is never NIL** (see
  No-NULL below).
- **`tx` — transaction.** A monotonic counter. Every assert/retract is stamped,
  which is what makes history and time-travel free.
- **`op` — operation.** `assert` (1) or `retract` (2).
- **`m` — module.** The eid of the module that asserted the fact. This makes
  module ownership a first-class property of *every* fact: unloading a module is
  a MEAT walk for `m = <module-eid>` (the module-keyed index) followed by
  retraction of the results.

### The reification gap (honest limitation)

The six-column tuple records the *source* of each fact but does not yet make
facts themselves into addressable entities. You cannot currently assert metadata
*about* a fact (provenance beyond source-module, justification, belief markers).
Full reification — facts as first-class entities with their own eids — is a
planned **additive, non-breaking** change. Until then, the `m` column is the only
per-fact metadata.

## The four (plus two) indexes

Every fact is filed into multiple sorted indexes so that any access pattern is a
direct walk rather than a scan. Edge bidirectionality is architectural and on by
default — though write-once code facts (AST/bytecode) opt out of the reverse
index they are never walked through (see [The Six Indexes §7](11a-the-six-indexes.md)).

| Index | Answers |
|---|---|
| **EAVT** | entity walks — "what does this entity look like?" |
| **AEVT** | schema walks — "all entities with attribute X" |
| **AVET** | value lookup / uniqueness — "who has value V for attribute A?" |
| **VAET** | reverse-reference walks — "who points at this entity?" |
| **MEAT** | module-scoped walks — "every fact owned by module M" |

(The five-index Datomic set plus MEAT, which the `m` column makes first-class.)
All indexes are persistent HAMTs — structural sharing across transactions makes
snapshots cheap. See [Persistent Data Structures](16-persistent-structures.md).

### Two cell kinds at the leaves

Not every index path needs to store the value, because some paths already encode
it in the key. `fact.zig` defines two leaf cells accordingly:

- **`ValueCell`** sits at the leaf of an `(entity, attribute)` path (EAVT, AEVT,
  MEAT — paths that don't include `v`). It carries the current value plus a
  history vector of `ValueHist` entries.
- **`PresenceCell`** sits at the leaf of an `(entity, attribute, value)` path
  (AVET, VAET). The value is already known from the key, so the cell only
  records whether the triple is currently present, plus a `PresenceHist`.

Both history vectors are RRB-Trees, and each history entry carries a durable
`basis` coordinate (Phase 15.R) so that exact `asOf:` survives compaction. See
[Time, History, and Temporal Queries](15-time-travel.md).

## Cardinality

An attribute is `:one` by default: asserting a new value for an existing
`(e, a)` implicitly retracts the old one (overwrite semantics). Opting an
attribute into `:many` (`[attr :cardinality :many]`) lets multiple values
coexist under the same `(e, a)`. Cardinality is itself a fact on the attribute
entity.

## No-NULL discipline

Following Codd and C. J. Date, the fact store contains **no NULL values, ever.**
This is a deliberate soundness choice, not an oversight:

- **Absence of a fact** models "no value known." A query returns a value or an
  empty result — never a NULL marker.
- **`nil` the entity** (well-known eid `0`) is a real, dispatchable object — the
  singleton "nothing object." `[nil isNil] → true`. It is *not* absence.
- **`nil` as a stored value** — `[e :a nil]` — is **forbidden**. `assert`
  panics on a NIL value in Debug builds (`FactStore.assert: NIL value
  forbidden`); release builds skip the check, but asserting nil is still a
  discipline violation. The distinguishable thing is the nil *entity* as a real
  object you can hold and dispatch on — not nil as a stored `v` standing in for
  absence.
- **What is forbidden** is letting one token mean both "the nothing object" and
  "no value known." That conflation is the 3-valued-logic trap. So absence
  always rides a *separate channel*: an empty result, an `ifAbsent:` block, or a
  raise — never a value. `at:ifAbsent:` is the primary keyed access; bare `at:`
  is `at:ifAbsent:[raise]`.

The payoff: queries stay sound 2VL, diffs are exact (no spurious NULL↔value
transitions), and reflection returns only what was actually asserted. This works
*because* an EAV store has no fixed row shape — an attribute is asserted or it
isn't — so Date's principle is what the substrate already wants. The full
treatment is in [Tagged Pointers and the Ref Encoding](14-refs.md).

## Kernel eids vs. user eids

Identity space is partitioned (`kernel_eids.zig`):

- **`0`** — `NIL`, the nothing-entity.
- **`1..15`** — kernel vtables (`VTABLE_VT`, `OBJECT_VT`, `SELECTOR_VT`,
  `CLOSURE_VT`, `VECTOR_VT`, …).
- **`16..255`** — kernel selectors and well-known attributes, allocated phase by
  phase (e.g. `SEL_LOOKUP = 16`, `ATTR_VTABLE = 19`, `CELL_VT = 202`).
- **`256+`** — user-allocated, handed out monotonically.

This block is an **append-only ABI.** Kernel eids are hardcoded constants; a
saved image stores eids, so renumbering the kernel block is a breaking
image-format change. New kernel eids are always appended at the next free slot,
never inserted mid-block. (See the Dylib ABI discipline note in the project
memory for how this same append-only rule governs the module `IdApi` struct.)

## Where to go next

- How code becomes facts: [Reification](12-reification.md).
- The value encoding in the `v` column: [Refs](14-refs.md).
- The `tx` column at work: [Time, History, and Temporal Queries](15-time-travel.md).
