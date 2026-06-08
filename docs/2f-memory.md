# Memory Management

Birnam has *three* distinct reclamation concerns, and conflating them is the trap
the [vision](10-vision.md) §Memory warns against. This page covers the segmented
arenas (`arena.zig`/`arenas.zig`), the value-heap garbage collector (`gc.zig`),
and history pruning (`prune.zig`) — and, crucially, how the three stay separate.

> Implementation: `src/kernel/{arena,arenas,gc,prune}.zig`. Concept:
> [Persistent Data Structures](16-persistent-structures.md) (epoch reclaim),
> [Performance](18-performance-model.md) (the unbounded-mutation finding).

## Segmented arenas — `arena.zig` / `arenas.zig`

An `Arena` is a *recording* allocator: it forwards to a backing allocator but
records each live allocation in a side table keyed by address. It is
**payload-preserving** (no intrusive header — so `obj[-1]` arithmetic still works)
and **non-moving** (recorded addresses stay valid for the allocation's life).

The named segments (`Kind`):

| Segment | Holds |
|---|---|
| `strings` | interner dupes + string boxes |
| `boxes` | float / FFI-pointer boxes |
| `bytecode` | bytecode byte streams |
| `captures` | closure capture arrays / BytecodeClosures |
| `closures_vt` | `CLOSURE_VT` wrappers |
| `sig` | FFI `Sig` holders |
| `structs` | materialized objects (also freed via demote) |
| `index` | HAMT/RRB index nodes (epoch-freed on compact) |

Each entry tracks `len`, `alignment`, and a `marked` bit. The collector surface is
`mark(addr)` (atomic, returns true iff newly marked), `contains(addr)` (segment
ownership), `sweep()` (free unmarked, clear marks), and `iterate()`.

`arenas` is the global registry: `init(backing)`, `deinit()`, `ready()`, and
per-kind accessors (`arenas.strings()`, `arenas.index()`, …). It is initialized by
the *live* entry points (`shell.main`, the AOT driver), **not** by
`bootstrap.boot` — so unit tests that boot with `testing.allocator` don't entangle
allocations across a shared process; tests own the `init`/`deinit` pairing
explicitly.

## The value-heap GC — `gc.zig`

A non-moving mark-sweep over four arenas — `boxes`, `strings`, `captures`,
`closures_vt`. These hold ordinary language *values* where reachability *is*
liveness, so "GC" is the right word here (unlike the fact layer).

**Roots:**

- **R1** — every fact `v`-column value (`meatIter`). Over-marks history → safe;
  under a current-only store it's the exact live set. Struct slots mirror current
  facts.
- **R2** — owned FFI-pointer boxes parked in the object table (`:ffi-owner`).
- **R3** — constant-pool RRB elements (recursed in place).
- **R4** — object-table slots: marks direct boxes and live closures; live
  closures are enqueued and their capture tails scanned conservatively to reach
  captured boxes and nested closures.

`consider(ctx, r)` is the typed mark dispatcher (string box → strings arena;
`CLOSURE_VT` wrapper → mark + follow `data`; float/FFI box → boxes arena; RRB →
recurse). `scanWords` conservatively scans a live closure's allocation, marking
any word that looks like a box pointer (and capturing strings held only by a
closure).

**Triggering and safety.** GC runs only at a quiescent point — between REPL lines,
when the value stack is empty. It is **skipped while `ffi_pause > 0`** (an alien
callee may hold a raw box address — the same pin that guards
[demotion](25-promotion.md)) and skipped when the arena registry is absent. The
trigger is **growth-gated**: `maybeCollect` fires only after net arena growth
crosses `GC_GROWTH_THRESHOLD` (50,000 cells), updating its baseline to the
post-sweep live count.

## History pruning — `prune.zig`

Pruning is a *policy* decision about which facts survive, not a reachability
question about heap objects. Scope is **`M_USER` only** (CODE facts are
reconstructed every boot). `markReachable` does a BFS from the roots — the stable
top-level scope (`TOP_SCOPE_EID`), installed methods (any M_USER fact whose value
is a `:bytecode-stream` entity), saved queries, and M_USER FFI wraps — following
all eid-valued facts forward (constant-pool block children, `:method-body-ast`
subtrees, scope/binding chains). It is conservative: over-marking keeps
maybe-garbage; it never drops a live entity.

The sweep *is* [`FactStore.compact`](22-fact-store.md): rebuild the indexes
keeping CODE facts plus marked M_USER facts, collapsing history. Swept entities
are `demote`d *before* their object-table slot is cleared (Phase 15.4) so the
materialized struct heap is reclaimed too — clearing the slot alone would leak it.
The `--strip=malleable` AST-stripping pass (Phase 14 M2) is a variant that drops
parsed AST and bytecode→AST debug back-refs from a pure-VM binary.

## Three concerns, kept separate

| Concern | Mechanism | Target | Trigger |
|---|---|---|---|
| **Value-heap GC** | non-moving mark-sweep | boxes, strings, captures, closures | growth-gated, REPL line boundary; honors `ffi_pause` |
| **Index-epoch reclaim** | epoch arena freed wholesale | HAMT/RRB index nodes | `compact()` swaps + frees the old epoch |
| **Fact/history retention** | reachable-from-roots + history collapse | M_USER facts | manual `:prune`; AST strip on AOT |

These are deliberately *not* one system. Value GC is reachability of objects; the
index epoch arena is pure plumbing (the persistent index path-copies and abandons
spine nodes, freed in one shot on compact); retention is a temporal/semantic
policy (Datomic-shaped). Treating the whole story as "GC" mis-frames the fact
layer, which is a temporal database, not an object graph.

## The unbounded-mutation finding

This is also where [ADR-perf-1 finding #1](18-performance-model.md) lives: fact
*writes* grow memory without bound (never-freed history + five-index path-copy),
so sustained mutation is bounded **only** with prune-in-loop plus the index epoch
arena. A naive million-iteration mutation loop exhausts memory — it crashed the
machine on the first benchmarking attempt. The
[benchmark harnesses](37-benchmarking.md) enforce in-process memory caps so that
failure mode is impossible.

## Where to go next

- Why writes grow memory: [The Fact Store](22-fact-store.md) (compaction).
- The structures whose spine nodes get abandoned:
  [HAMT and RRB](23-hamt-rrb.md).
- The finding and the safe harnesses: [Performance](18-performance-model.md),
  [Benchmarking](37-benchmarking.md) (Part 3).
