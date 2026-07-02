# Reference Cells (Temporal Instances)

Reference cells are Phase 16's lead feature and the shipped instance of the
[performance model's](18-performance-model.md) tier-3 (temporal write-through)
representation. A cell is an entity whose single attribute is historied — so it
gets `asOf:` and `history` for free off the [15.R machinery](15-time-travel.md).

> Implementation: `src/modules/lang/lang.zig` (the cell handlers),
> `src/kernel/kernel_eids.zig` (`CELL_VT`, `ATTR_CELL_VALUE`),
> `src/kernel/fact_store.zig` (history). Concept: [Time-Travel](15-time-travel.md).

## A cell IS an entity

There is no separate cell struct. A cell is just:

```
[cell :vtable CELL_VT]          (CELL_VT = 202)
[cell :cell-value <v>]          (ATTR_CELL_VALUE = 203)
```

Because dispatch resolves an eid receiver via its `:vtable` fact, the cell eid is
directly sendable. Reads are `currentValue`; writes are `assert` (a new tx); and
since the `:cell-value` attribute accumulates history like any other, the cell
gets time-travel at no extra cost. This is what makes cells the natural T3
instance — they *are* write-through historied facts, by construction.

## Minting

The `db` prototype mints cells. `cell:`, `ref:`, and `atom:` all map to the same
handler (`dbCellImpl` on `DB_PROTO_VT`):

```zig
fn dbCellImpl(s, args, n) Ref {
    if (isNil(args[0])) return mkRuntimeError("cell cannot be seeded with nil");
    const e = api.alloc_eid();
    api.assert_fact(e, ATTR_VTABLE, mkEid(CELL_VT), M_USER);
    api.assert_fact(e, ATTR_CELL_VALUE, args[0], M_USER);
    return mkEid(e);
}
```

Three names, one mechanism: the single-transactor model subsumes Clojure's
atom/ref/agent distinction (there is no concurrency to coordinate). Note the
facts are `M_USER` — durable, so a cell survives [image save/load](2e-images-aot.md).
Seeding with `nil` is refused (No-NULL: a cell holds a value).

## Messages

Four methods on `CELL_VT`:

| Message | Handler | Implementation |
|---|---|---|
| `[cell value]` | `cellValueImpl` | `query_eavt(cell, :cell-value)` — current value |
| `[cell set: v]` | `cellSetImpl` | `assert_fact(cell, :cell-value, v, M_USER)`; returns the cell (chainable). The store auto-retracts the old value (`:one` cardinality) |
| `[cell asOf: tx]` | `cellAsOfImpl` | `current_value_as_of(cell, :cell-value, tx)` — historical read |
| `[cell history]` | `cellHistoryImpl` | `cell_history(cell, :cell-value)` — RRB of every value held, oldest→newest |

`asOf:` and `history` route through the IdApi functions appended in Phase 16 Tier
E — `current_value_as_of`, `cell_history`, `basis_now` — which thinly wrap
`FactStore.currentValueAsOf`, `valueHistory`, and `currentBasisT`. No special cell
history machinery exists; it is the ordinary fact history from
[The Fact Store](22-fact-store.md).

## Atomicity: `db transact:`

```zig
fn dbTransactImpl(s, args, n) Ref {     // [db transact: aBlock]
    return api.send0(args[0], sel_value);
}
```

Atomicity is **by construction**: the block's `set:` calls run sequentially with
no other writer able to interleave (single-transactor), so the whole block
commits as an indivisible sequence. This is the "atomic-by-single-transactor"
property the [vision](10-vision.md) names as a non-goal-deferral for concurrency.

## Metadata: `withMeta:` / `meta`

Installed on `OBJECT_VT`, so any object can carry metadata:

- `[obj withMeta: m]` asserts `[obj :meta m]` (`ATTR_META = 204`).
- `[obj meta]` reads it back.

Metadata is free — just another orthogonal fact — and equality ignores it.

## Why cells are the T3 reference implementation

The [performance model](18-performance-model.md) defines three tiers; T3 is
opt-in temporal write-through, the only tier that gets `asOf:`/`history`. A cell
is precisely a single-attribute T3 object: every write is a historied fact, so
the cost (a five-index path-copy plus history append, ~1.8 µs) is paid
deliberately, localized to the cell, in exchange for time-travel. Objects that
don't need history stay in the cheaper T2 tier. Cells are how you say "I want
this slot to remember its past."

## Where to go next

- The temporal model behind `asOf:`/`history`: [Time-Travel](15-time-travel.md).
- The cost trade cells make explicit: [Performance](18-performance-model.md).
- Using cells: [Reference Cells and Time-Travel](34-cells-usage.md) (Part 3).
