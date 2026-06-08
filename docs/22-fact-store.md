# The Fact Store

`src/kernel/fact_store.zig` is the database at the center of Birnam — the
concrete implementation of the model in [The Fact Model](11-fact-model.md). This
page documents its struct, its public API, how a write flows through the indexes,
and how compaction reclaims.

> Implementation: `src/kernel/fact_store.zig`. Concepts:
> [The Fact Model](11-fact-model.md), [Time-Travel](15-time-travel.md),
> [Persistent Data Structures](16-persistent-structures.md).

## The five indexes, concretely

Each index is nested HAMTs (see [HAMT and RRB](23-hamt-rrb.md)). The value type
at the leaf is a `ValueCell` (current value + history) or a `PresenceCell`
(boolean + history), depending on whether the value is part of the key path:

| Index | Concrete type | Leaf |
|---|---|---|
| **EAVT** | `HAMT<e, HAMT<a, ValueCell>>` | ValueCell |
| **AEVT** | `HAMT<a, HAMT<e, ValueCell>>` | ValueCell |
| **AVET** | `HAMT<a, HAMT<v, HAMT<e, PresenceCell>>>` | PresenceCell |
| **VAET** | `HAMT<v, HAMT<a, HAMT<e, PresenceCell>>>` | PresenceCell |
| **MEAT** | `HAMT<m, HAMT<e, HAMT<a, ValueCell>>>` | ValueCell |

The two counters are atomic (forward-compat with the Datomic-shape transactor):
`next_tx` (per-epoch, resets on compact) and `basis_t` (durable, never resets).
See [Time-Travel](15-time-travel.md) for the tx/basis distinction.

Other notable struct fields: `index_arena` (the Phase-15.I epoch arena),
`retained_epochs` / `retention_policy` (15.R), `read_only` (the Phase-14 freeze
gate), and two hooks — `on_attr_mutate` (mcache flush) and `on_entity_mutate`
(demote). The `fact_eids` map backs lazy reification.

## Public API surface

### Mutation

```zig
pub fn assert(self: *FactStore, e: u64, a: u64, v: Ref, m: u64) !u64
pub fn retract(self: *FactStore, e: u64, a: u64, v: Ref, m: u64) !u64
```

Both bump `next_tx` and `basis_t`, apply across the indexes, fire the hooks, and
return the assigned tx. `assert` panics in Debug if `v == NIL` (the No-NULL
discipline) and rejects when `read_only`.

### Point reads

```zig
pub fn currentValue(self, e, a) ?Ref            // :one → value; :many → RRB of values; null = absent
pub fn currentValueAsOf(self, e, a, as_of_tx) ?Ref
pub fn currentValueAsOfBasis(self, e, a, basis) ?Ref
pub fn rawCurrentValue(self, e, a) ?Ref         // skips cardinality polymorphism
pub fn lastTouchTx(self, e, a) ?u64
```

`currentValue` reads `cell.current_v` directly — O(1), no history walk. The
`asOf` variants walk `cell.history` backward to the latest entry at/under the
requested coordinate.

### History

```zig
pub fn valueHistory(self, e, a) !Ref            // RRB of every asserted value, oldest→newest
```

Backs `[cell history]` (see [Reference Cells](2a-cells.md)).

### Index iteration

```zig
pub fn attributesOf(self, e) EntityAttrIter            // EAVT walk: all (a,v) of e
pub fn entitiesWith(self, a) AttrEntityIter            // AEVT walk: all (e,v) for a
pub fn entitiesMatching(self, a, v) AttrValueEntityIter// AVET equality lookup
pub fn pointingAt(self, v, scratch) !ValueRefIter      // VAET reverse-ref walk
pub fn factsOfModule(self, m, scratch) !ModuleFactIter // MEAT: every fact of module m
```

`entitiesMatching` is the workhorse for reflection and the query engine;
`factsOfModule` is what module unload uses to find facts to retract.

### Snapshots, compaction, freeze

```zig
pub fn snapshotNow(self) !*Snapshot
pub fn snapshotAsOf(self, basis) !*Snapshot           // error.HistoryNotRetained if too old
pub fn compact(self, scratch, ctx, comptime keep) !CompactStats
pub fn enableIndexArena(self) !void                   // call on empty store, post-init
pub fn freeze(self) void                              // Phase 14 — lock read-only
pub fn currentTx(self) u64
pub fn currentBasisT(self) u64
```

A `Snapshot` is a by-value view of the index roots plus the basis it captured; it
pins its epoch and must be `deinit`'d to unpin. See
[Time-Travel](15-time-travel.md). Retention/excision (`setRetention`,
`exciseEntity`, `exciseTxRange`, `maybeSpillEpochs`) round out the 15.R surface.

## How a write flows

`assert(e, a, v, m)` bumps both counters then calls `applyAssert`, which updates
each index by structural path-copy (HAMT assoc through the index arena):

1. **EAVT[e][a]** — update the `ValueCell`: append a `ValueHist{v, tx, op, basis}`
   to its RRB history and set `current_v = v`.
2. **AEVT[a][e]** — same `ValueCell` update on the schema-walk path.
3. **MEAT[m][e][a]** — same, under the module path.
4. **Cardinality** — if the attribute is `:one` and a different prior value
   existed, retract it from the presence indexes first (implicit overwrite).
5. **AVET[a][v][e]** and **VAET[v][a][e]** — set the `PresenceCell` present, with
   a `PresenceHist{tx, op, basis}`.

Then the hooks fire: `on_attr_mutate(a)` flushes the [mcache](26-dispatch.md) for
that selector, and `on_entity_mutate(e, a)` lets [promotion](25-promotion.md)
patch or demote the materialized struct.

Because every index update is path-copy, the prior roots remain valid — that is
what makes a [snapshot](15-time-travel.md) a single root pointer, and it is also
why a write touches five trees plus history (the
[performance](18-performance-model.md) cost).

## Compaction and reclamation

`compact` collapses history and frees the old index epoch wholesale:

1. **Snapshot facts by value** — walk MEAT, copy out `(m, e, a, v)` tuples (no
   pointers into the old index).
2. **Swap epochs** — capture the old roots and arena into a `RetainedEpoch`,
   allocate a fresh arena, bump the epoch generation, set the collapse basis.
3. **Reset all five indexes** empty on the new arena.
4. **Re-insert kept facts** — for each tuple where `keep(ctx, m, e, a, v)` is
   true, `compactInsert` it under a single collapse basis (no implicit-retract,
   no per-fact history). `next_tx` restarts at the count.
5. **Free or retain the old epoch** — if no live snapshot pins it and it is below
   the retention horizon, `arena.deinit()` frees every old node and history
   vector in one sweep; otherwise it joins `retained_epochs`.

This is the only mechanism that reclaims index memory, and it is why sustained
mutation must prune in a loop — see [Memory Management](2f-memory.md) and
[Benchmarking](37-benchmarking.md).

## Transactions

`src/kernel/tx_engine.zig` is the consolidated commit path. Today it is a thin
wrapper — `commit(store, op)` switches on `opAssert`/`opRetract` and calls
`store.assert`/`retract` directly. Under the future Datomic shape it becomes a
channel send to a designated transactor thread. Atomicity at the language level
(`db transact:`) is currently provided by the single-writer discipline — see
[Transactions](2b-transactions.md).

## Where to go next

- The trees underneath the indexes: [HAMT and RRB](23-hamt-rrb.md).
- The mutate hooks' consumers: [Dispatch](26-dispatch.md), [Promotion](25-promotion.md).
- The temporal surface this enables: [Reference Cells](2a-cells.md).
