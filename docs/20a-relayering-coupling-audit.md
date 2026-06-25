# Re-Layering Coupling Audit (W6 / ADR-image-1 · D5)

The concrete current-state input for **W6** — the identity/fact-store re-layering
decided in ADR-image-1's addendum (D5): *promote the persistent object-identity
layer to a first-class layer the fact store sits on, rather than "a projection
rebuildable from EAVT."* Before that can be planned, one question must be answered
with evidence: **what in the frozen kernel reaches up into the fact store?** That
is the refactor surface. This page maps it.

> Method: kernel import-graph sweep + per-coupling `file:line` read, 2026-06-24,
> against `src/kernel/*.zig` (tests excluded). Decision: `ROADMAP.md` →
> ADR-image-1 (Decisions D5, open questions Q1–Q3, work item W6). Background:
> [Kernel Overview](20-kernel-overview.md), [The Six Indexes](11a-the-six-indexes.md),
> [Dispatch](26-dispatch.md), [The Performance Model](18-performance-model.md).

## The target layering (what we are auditing against)

Arrows point **down only** — a lower layer must never reach up:

```
L4  modules / language / image    everything fact-shaped on top
L3  fact store as POLICY           which indexes, tx, history, tiers   (frozen primitives + injectable policy)
L2  dispatch                       bind / mcache / -1 trick            (frozen mechanics; see Q2)
L1  identity                       object table + eid alloc + tag/untag (frozen primitive; ALREADY independent)
L0  persistent structures          HAMT, RRB                           (frozen primitive)
```

A coupling is a **violation** when an L0/L1/L2 file imports/uses `fact_store`
(L3). L4 and same-layer (L3↔L3) uses are expected and fine.

## Layer classification of every kernel file

| Layer | Files | Reaches up into `fact_store`? |
|---|---|---|
| **L0 structures** | `hamt`, `rrb` | **clean** |
| **L1 identity/value** | `object_table`, `ref`, `kernel_eids`, `strings`, `boxed`, `interner` | `object_table` / `ref` / `strings` / `kernel_eids` clean; **`boxed` + `interner` reach up (shallow)** |
| **L2 dispatch/object** | `mcache`, `object`, `closure`, `vm`, `bytecode`, `send`, `dispatch` | `mcache` / `object` / `closure` / `vm` / `bytecode` clean; **`send` + `dispatch` reach up (deep)** |
| **L3 fact store** | `fact_store`, `fact`, `tx_engine` | same-layer — fine |
| **L4 facts-shaped / facade** | `compiler`, `query`, `predicate`, `rule`, `pull`, `saved_query`, `prune`, `promotion`, `gc`, `image`, `freeze`, `loader`, `macho`, `elf`, `ffi`, `ffi_module`, `module`, `peg`, `aggregate`, `bootstrap`, `id_api` | expected — fine |

**The entire reach-up surface is four files.** L0, the core of L1
(`object_table` owns eid allocation and imports nothing upward), and most of L2
(`mcache`, `object`, `closure`, `vm`, `bytecode`) are **already layer-clean**.

### `fact_store`'s own dependencies are down-only

`fact_store.zig` imports `hamt`, `rrb`, `ref`, `fact`, `boxed`, and
`object_table` (the last only via `eid_alloc_table` / `setEidAllocTable`, to mint
eids — `object_table.zig:76` `alloc()` = `fetchAdd` on `next_eid` from
`USER_EID_START`). No edge up into `dispatch` / `compiler` / `query`. So **the
floor is structurally close to the target already**; this is a re-frame + a few
cuts, not a teardown.

## The two DEEP couplings — `send` + `dispatch` (this is Q2)

The metacircular-dispatch seam. Dispatch's *source of truth* is the fact store —
not incidental, by design.

**`send.zig` (L2 → L3) — method resolution is EAVT reads:**

| Site | Call | What it resolves |
|---|---|---|
| `send.zig:131` | `currentValue(vt, selector)` | method lookup `[vt sel ?method]` (mcache fronts it; cold path hits facts) |
| `send.zig:215` | `currentValue(receiver, ATTR_VTABLE)` | vtable resolution (`vtableEidOf`) |
| `send.zig:143`, `:178` | `currentValue(vt, ATTR_DELEGATE)` | delegation / inheritance walk |
| `send.zig:162` | `currentValue(vt, ATTR_BIND)` | custom `bind`-is-a-method |

**`dispatch.zig` (L2 → L3) — method *body* reads + frame-reification writes:**

| Site | Call | What it does |
|---|---|---|
| `dispatch.zig:50/56/74` | `currentValue(method, ATTR_BYTECODE_STREAM \| ATTR_CONSTANT_POOL \| ATTR_METHOD_CAPTURES_PTR)` | reads the method body to run it (materialized into a `Closure` after first read) |
| `dispatch.zig:442–448` | `assert(frame, ATTR_FRAME_PC \| _METHOD \| _SELF \| _PARENT)` | **writes** the call stack / continuation as facts — **separable** (only on stack reification: non-local return / debug, not every call) |

**Q2 resolution space (planning decides, not pre-decided):**
- **(a) projection-populated** — L2 dispatches over an L1/L2 representation
  (materialized vtable structs + `mcache`), which the fact store *populates on
  miss / promotion*. The cold path becomes an explicit **L3 → populate-L2** flow,
  not an L2 → L3 read. `mcache` is already a partial instance of this, so the path
  exists.
- **(b) logical boundary** — accept dispatch is fact-backed; make D5's down-only
  rule *logical, not physical* for L2 (metacircularity reads L3 by permission).

The frame-reification writes (`:442–448`) are separable and should move to an
explicit L3+ continuation path regardless of (a)/(b).

## The two SHALLOW couplings — `boxed` + `interner` (easy wins, + a cycle)

**`boxed.zig` (L1 → L3) — one function.** `boxed.zig:200–211` asserts
`[box :vtable FFI_PTR_VT]` + `[box :ffi-owner …]` to register an owned
FFI-pointer box as a fact-entity. The rest of `boxed` (float / ptr / RRB / map
boxing) is fact-free. **Cut:** move that one register function up to L4 (`ffi`),
or inject a callback.

> **This is a true cycle.** `fact_store.zig:25` imports `boxed` (boxed values in
> the `v` column) **and** `boxed.zig:200` imports `fact_store`. Cutting the one
> register function **breaks the L1↔L3 cycle** — the highest-value first step.

**`interner.zig` (L1 → L3) — name publication.** `interner.zig:74/:100` assert
`[eid :name "str"]` so interned names are queryable/reflective. The bijection
itself (its own maps) is fact-free. **Cut:** inject an on-intern hook, or publish
names-as-facts at L4. After this, all of L1 (`object_table` + `interner` +
`boxed` + `ref` + `strings`) is fact-free — a genuinely independent identity layer.

## The W6 cut-list (ordered by difficulty — the plan spine)

1. **Break the `boxed`↔`fact_store` cycle** — move FFI-owner-box registration out
   of `boxed` (L1) up to `ffi` (L4). Mechanical; removes a real circular dep.
2. **Purify the interner** — bijection-only (L1); name-fact publication moves to
   an injected hook / L4. L1 becomes fact-free.
3. **Design the dispatch↔facts seam (Q2)** — projection-populated (a) vs.
   logical-boundary (b). The only deep work; the other items are bookkeeping.
4. **Separate frame reification** — move the `ATTR_FRAME_*` writes to an explicit
   L3+ continuation-reification path, out of core dispatch.
5. **Generalize the policy injection (Q3)** — extend the `AttrTier` / cardinality
   injected-predicate pattern so index-set / history / tier is swappable (the Q1
   "primitives frozen, policy injectable" line).

## Mapping to the ADR's open questions

- **Q1 (bootstrap / frozen-vs-policy line)** → cut #5. Viable: the policy seam
  already exists (`AttrTier`); `fact_store`'s primitives have clean down-deps, so
  "primitives frozen, policy injectable" is a generalization, not new machinery.
- **Q2 (dispatch cold-path)** → cuts #3 + #4, and the audit shows this is the
  *only* hard part — 8 `currentValue`/`assert` sites across `send` + `dispatch`,
  of which the frame writes are separable.
- **Q3 (how far to policy-ize)** → cut #5 scope decision (minimum: inject
  index/history/tier policy; maximum: the whole 5-index arrangement above the
  frozen line).

## Bottom line

The re-layering is **~80% mechanical** (cuts #1, #2, #4 + the policy plumbing #5)
and **~20% genuine design** (the Q2 dispatch seam, #3). The identity floor
(`object_table` + eid allocation) is **already independent**; the one load-bearing
decision is whether L2 dispatch reads facts directly or reads a fact-populated
projection. Nothing here contradicts the Datomic bargain — identity stays
persistent (a HAMT), nothing becomes mutable (the rejected trap); this is a
re-frame of *which layer is primary*, plus four seam cuts.
