# Time, History, and Temporal Queries

Because every assert and retract carries a transaction stamp, Birnam records its
own history for free. Time-travel ("what did the store look like as-of `tx`?") is
not a feature added on top — it falls out of the [fact model](11-fact-model.md).
This page covers the temporal model, the retention/snapshot/excision machinery
(the "Datomic bargain"), and the crucial decision that time-travel is now
*opt-in* per object.

> Implementation: `src/kernel/fact_store.zig`, history in `src/kernel/fact.zig`,
> the 15.R retention work. Design: `VISION.md` §Memory → Retention. Cost
> analysis: ADR-perf-1 (see [Performance](18-performance-model.md)).

## Transactions and basis

Two distinct coordinates track time:

- **`tx`** — a monotonic counter stamped on every operation. It is *epoch-local*:
  compaction resets it. Good for ordering within an epoch.
- **`basis`** (Phase 15.R) — a *durable* coordinate that never resets on compact.
  Exact `asOf:` walks history by `basis`, so time-travel survives compaction.

Each history entry (`ValueHist` / `PresenceHist` in `fact.zig`) carries `v` (for
value cells), `tx`, `op`, and `basis`. Entries written before basis tracking
existed (e.g. `frozenInsert`, which is read-only afterward) carry `basis = 0`.

## How history is stored

History is not a side log — it lives *in the index*, at the leaf cells:

- A `ValueCell` keeps `current_v` plus an RRB-Tree `history` of `ValueHist`
  entries. The current value is O(1); the past is an ordered walk.
- A `PresenceCell` (for AVET/VAET) keeps `present: bool` plus a `PresenceHist`.

So "the current store" and "every past store" are the same data structure viewed
at different depths. There is no separate replay log to keep consistent.

## The temporal query surface

Time-travel is exposed through the `db` prototype and reference cells:

- `db now` / `db asOf: <tx>` yield first-class **snapshot** values
  (`SNAPSHOT_VT`) — immutable handles to the store at a point in time.
- A snapshot answers ordinary Datalog queries, so *every* query is implicitly
  time-travelable: run it against `db now` or against `db asOf: t`.
- Reference cells get `[cell asOf: tx]` and `[cell history]` for free, because a
  cell is just an entity and its `:cell-value` attribute already has history. See
  [Reference Cells](2a-cells.md) (Part 2) and
  [Cells & Time-Travel usage](34-cells-usage.md) (Part 3).

The query language and its `asOf:`/`since`/`history` semantics are specified in
`DATALOG.md`; the surface is pattern-sends (`VISION.md` §Query Surface).

## Retention, snapshots, excision — the Datomic bargain

The persistent substrate's superpower is that **an index root pointer IS a
complete zero-copy snapshot of the whole store at its tx** (structural sharing —
see [Persistent Data Structures](16-persistent-structures.md)). Reclamation must
not throw that away. The model is tiered so it stays invisible until you reach
for history:

- **Default (current-only): invisible.** A user who only wants "now" never sees
  retention. History collapses continuously (auto-prune), index epochs free on
  compact, value-heap GC runs in the background. No DBA surface. This is today's
  behavior.
- **Snapshots are values, and holding one pins its storage.** Capturing
  `db asOf: t` yields an immutable handle that keeps the index epoch(s) it needs
  alive — exactly like any held object pins its heap. The epoch-arena free skips
  any epoch at-or-after the oldest retained basis-T. This is reachability over
  *roots*, not over nodes.
- **Retention horizon (policy, opt-in):** keep-last-N-txs / keep-since-T /
  keep-reachable-from-roots. Sets the floor below which history is collectible.
  Surfaced only when a user opts into time-travel.
- **Excision (explicit, rare):** hard-delete of named entities or tx ranges
  (GDPR-shaped). Distinct from prune — it removes facts *within* the retained
  window, on purpose, and is the one op that can invalidate held snapshots.
  Always user-invoked.

The 15.R work item (see project memory) adds the separate `basis_t`, a
refcounted epoch list, and an `oldestRetainedBasisT()` gate so an epoch is freed
only when no retained snapshot still references it.

## Time-travel is opt-in (ADR-perf-1)

> **Status:** the per-object tier model below is the *decided* design
> (ADR-perf-1, perf-1.3 design-resolved). It is **not yet implemented** —
> perf-1.4 (the write-back tier) is OPEN. Today every base-level user-object
> mutation is still a write-through historied `assert`; there is no `:temporal?`
> bit or `beTemporal` in the code. What *is* shipped and opt-in right now is the
> **retention horizon** (default collapses to "now"; you opt into deeper history
> via the retention policy) and **reference cells** (the T3 instance below).
> The rest of this section describes intent.

The most important decision: **history will be opt-in per object, not mandatory
for all state.**

The reason is cost. Writing a fact through five indexes plus history is roughly
100–1000× the cost of a native field write (measured ~1.8 µs/op). If *every*
object mutation were a write-through historied fact, mutation-heavy code would be
permanently capped at database-write speed. So the planned representation is
tiered (full detail in [The Performance Model](18-performance-model.md)):

- **T1 — transient/heap:** no eid, no facts (collections, scalars, locals,
  non-escaping objects). No history.
- **T2 — materialized write-back (planned default for escaped user objects):**
  the struct is the runtime truth; facts are a projection reconciled at
  query/commit/save. `asOf:`/`history` are empty/error here. *Not yet
  implemented — perf-1.4.*
- **T3 — temporal write-through (opt-in):** historied; gets `asOf:`/`history`.
  Reference cells are the shipped instance of this tier.

The planned opt-in is a single `:temporal?` bit at three granularities:
`[obj beTemporal]` (one instance), `[Account beTemporal]` (a prototype, stamped
at `new`), or `db cell:` (per-field — a cell is temporal by default; **this one
is shipped**). History *begins* at opt-in; the cost stays local to the entities
that asked for it.

**Meta-level facts (vtables, methods, AST, bytecode) stay write-through** — they
are written once and dispatch reads them, so they can't be naïvely write-back.
"Default T2" will apply to *base-level user objects only*. (Write-through does not
imply *historied*: AST and bytecode facts are a shipped **no-history** code tier —
`asOf:` over them returns null, since no one time-travels the compiler's
intermediate states — whereas vtables and methods keep full history. See
[Code-Fact Tiers](19a-code-fact-tiers-plan.md).)

## AOT prune: the most aggressive horizon

Ahead-of-time compilation sets the retention horizon to "now" permanently:
collapse all history into a single latest-tx snapshot, drop the `tx`/`op`
columns, freeze the indexes read-only, and demote objects to eid form (or
pre-materialize and freeze). The same temporal machinery, dialed to its minimum.

## Where to go next

- The structures that make snapshots zero-copy:
  [Persistent Data Structures](16-persistent-structures.md).
- The shipped temporal object: [Reference Cells](2a-cells.md) (Part 2).
- The full cost argument behind opt-in: [The Performance Model](18-performance-model.md).
