# Load-Time Memory: Scaling Code-as-Facts

Loading and compiling a non-trivial Birnam program is expensive in memory. The
`examples/logue` roguelike (~900 lines across three files) took **~4.4 GB** of
RAM just to load and compile to bytecode — before a single turn of play. This
page explains *why*, what is and isn't inherent to "everything is facts," and
the two levers now shipped to keep load-time memory bounded.

> Implementation: `src/kernel/prune.zig`, `src/kernel/fact_store.zig`,
> `src/modules/lang/lang.zig` (`doFileInSource`), `src/kernel/id_api.zig`.
> Background: [The Performance Model](18-performance-model.md),
> [Reification](12-reification.md), [The Fact Store](22-fact-store.md).

## Where the memory goes

Per [Reification](12-reification.md), the compilation pipeline is fact-shaped:
source → CST → AST → bytecode, and **every stage materializes as facts** —
one entity per AST node, a method entity per closure carrying
`:bytecode-stream` / `:constant-pool`, plus each top-level `let`'s scope
binding. [The Performance Model](18-performance-model.md) measures a fact write
at **~10 KB across the five indexes** (path-copy of five HAMTs plus a
never-freed history entry). A logue-sized program is a few hundred thousand such
facts, and the bulk of them is **transient**: each top-level form compiles a
run-and-discard *wrapper method* (its CST, AST, and bytecode) that is dead the
moment the form's value is computed.

The prior in-repo measurement (`fml`) localized it precisely: bare interpreter
~679 MB; filing in `logue-core.birnam` alone added ~2.7 GB (~30 MB **per
top-level form**), while generating a dungeon and playing 2000 turns added only
~tens of MB each. **The cost is loading the program, not running it.** And it
was super-linear because nothing reclaimed the per-form garbage *during* the
file-in — every form's dead wrapper tree accumulated to peak.

## What is *not* inherent to "everything is facts"

The instinct is that the fact model itself doesn't scale. It does — what doesn't
scale is three properties that currently ride on *every* fact, including
write-once code facts, none of which "everything is facts" actually requires:

1. **Per-fact history.** Every `assert` appends an RRB history entry
   (`docs/22-fact-store.md` — "How a write flows"). AST and bytecode are written
   **once and never mutated**; their history is pure waste — you will never
   `asOf:` the bytecode emitter's intermediate states.
2. **Incremental five-index path-copy.** The compiler asserts one fact at a time
   (`compiler.zig`), path-copying five HAMTs per assert and abandoning the
   intermediate spine nodes. Loading hundreds of thousands of facts this way
   generates millions of dead spine nodes that aren't freed until a compaction.
3. **All five indexes, unconditionally.** `AVET`/`VAET` are triple-nested HAMTs
   — the costly two. You essentially never query "which entity has
   `:bytecode-byte = 0x42`" (AVET) or reverse-ref-walk into a constant-pool
   integer (VAET); for code, two of the five indexes are dead weight.

The spirit (the tenets in [Reification](12-reification.md)) is that code is
**queryable, diffable, replaceable, module-attributed** — facts as the *source
of truth*. It does **not** require code facts to be temporal, eagerly
five-way-indexed, or path-copied one at a time. Datomic — Birnam's explicit
model — already separates these with `:db/noHistory` and non-indexed
attributes. So the fix is to adopt, for the meta-level (code) facts, the parts
of that model Birnam has so far deferred — not to abandon facts.

## What shipped

The substrate already has the right primitive: `FactStore.compact`
(`docs/22-fact-store.md`) rebuilds the five indexes into a fresh epoch arena,
frees the old epoch **wholesale**, and re-inserts surviving facts under a single
basis with **no per-fact history** (`compactInsert`). `prune.pruneReachable`
drives it from the live roots (top-level scope, installed methods, saved
queries, FFI wraps), sweeping unreachable `M_USER` entities — exactly the
run-and-discard wrapper trees. The REPL already runs a threshold-gated
`maybePrune` at each line boundary; the gap was that **file-in never pruned**,
and a `birnam script.birnam` run exits before the REPL loop is ever reached.

### Lever 1 — prune between file-in forms (bounds *peak*)

`doFileInSource` now calls a new ABI hook `maybe_prune` at each top-level form
boundary (the VM is quiescent there, the same boundary the REPL prunes at). It
is threshold-gated (`PRUNE_GROWTH_THRESHOLD`, 30k new entities), so it actually
compacts only periodically — bounding peak instead of carrying every form's dead
tree to the end. The shell baselines the growth counter (`resetAutoPrune`)
before the script loop so the threshold tracks the *script's* growth, not the
fixed kernel+grammar+prelude init.

Measured against a faithful reproduction of the file-in write pattern (real
`FactStore` + `prune` + epoch arena; surviving binding chain + per-form
throwaway subtree; peak RSS via `/proc/self/status`):

| top-level forms | entities | **peak, no prune** | **peak, lever 1** |
|---:|---:|---:|---:|
| 400 | 120,656 | 3,164 MB | 780 MB |
| 800 | 241,056 | 6,527 MB | 799 MB |
| 1,600 | 481,856 | 13,788 MB | 813 MB |

No-prune peak grows **linearly with program size**; with lever 1 it stays
**flat (~800 MB)**. That is the scalability fix: load-time peak decouples from
program size. The observed 4.4 GB sits squarely in the no-prune band.

### Lever 2 — one forced compaction at the load boundary (minimizes *steady state*)

The threshold gate means lever 1 always leaves a sub-threshold *tail* of garbage
(up to ~30k entities) and keeps the surviving program's write-through history.
After the form loop, `doFileInSource` now calls a second ABI hook `prune_now`
(`prune.pruneNowIfSafe` — same retention/snapshot safety gate as `maybePrune`,
but no threshold). It sweeps the residual tail and collapses the loaded
program's per-fact history to a single basis, minimizing the steady-state
footprint the program then runs in.

Its value depends on where lever 1's last sweep landed relative to load end. In
the worst case (load ends just before a threshold would fire), lever 1 alone
leaves the whole tail resident; lever 2 reclaims it:

| | end-of-load resident |
|---|---:|
| lever 1 only (last sweep ~95 forms before end) | 715 MB |
| lever 1 + lever 2 | 3 MB |

Both levers are safe by construction: they only run under current-only retention
with no live snapshot pinning an epoch, and they reuse `pruneReachable` — the
same mechanism `:prune` has always used — so reachable bindings, installed
methods, saved queries, and FFI wraps are kept; only unreachable `M_USER`
garbage is swept. Nothing about the fact model, reflection, or live mutation
changes; you lose only `asOf:` over the *compilation's* intermediate states,
which no one queries.

## What is deliberately deferred

These were considered and left for when a workload demands them. They are
additive and keep the facts as the source of truth:

- **A `:no-history` tier for code attributes.** Avoid allocating the history RRB
  for write-once meta-level facts *at the source*, rather than reclaiming it at a
  compaction boundary. Already on the roadmap (deferred from Phase 11.x). This
  attacks property #1 directly; levers 1–2 attack it after the fact.
- **Index selectivity for code attributes.** A per-attribute "don't AVET/VAET
  index this" policy (Datomic `:db/index false`) drops most code facts from five
  indexes to three (keeping EAVT/AEVT/MEAT, so module-unload still works) —
  roughly halving *retained* memory. Attacks property #3.
- **Bulk/transient load.** Build the indexes once from the whole form batch (a
  transient HAMT builder frozen on publish — the shipped `frozenInsert` is the
  current-values-only shape of this) instead of path-copying five HAMTs per
  assert. Attacks property #2 at the source. The hazard for *live* file-in is
  that `frozenInsert` skips implicit-retract and the mutate hooks, so it is
  unsafe where a load redefines an existing method; a transient builder that
  preserves those is the correct general form.
- **AST granularity.** Bytecode is already a byte-slice blob (good); the AST is
  one entity per node. Treating source as the durable form and the AST as a
  re-parseable cache would cut the node count, but partially betrays "the AST is
  live-editable facts," so it is a last resort, not a default.

## Relationship to ADR-perf-1 finding #1

[The Performance Model](18-performance-model.md) records that the first
benchmarking attempt crashed the machine: sustained mutation grows memory
without bound (never-freed history + five-index path-copy), and the discipline
is "prune in a loop plus the index epoch arena." File-in is exactly that loop
for *load* rather than *runtime* mutation; levers 1 and 2 apply the same
discipline at the file-in boundary. See also the punchlist's §1 ("unbounded
store growth in long-running loops") — load-time growth is its load-bearing
facet.

## Where to go next

- The cost bands these levers respond to: [The Performance Model](18-performance-model.md).
- The compaction mechanism they drive: [The Fact Store](22-fact-store.md) (§Compaction).
- Why code is facts in the first place: [Reification](12-reification.md).
