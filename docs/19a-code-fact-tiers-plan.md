# Implementation Plan: The Three Deferred Code-Fact Tiers

This is the build plan for the three optimizations
[Load-Time Memory](19-load-memory.md) defers under *"What is deliberately
deferred."* Where levers 1–2 of that page reclaim garbage **after the fact** at
compaction boundaries (`prune.maybePrune` / `prune.pruneNowIfSafe`), the three
levers here attack the same root-cause properties **at the source** — they stop
the waste from being written in the first place.

| # | Lever | Root-cause property it kills | Primary file(s) |
|---|---|---|---|
| 1 | `:no-history` tier for code attrs | per-fact history (RRB append per assert) | `fact_store.zig` |
| 2 | Index selectivity for code attrs | all-five-indexes path-copy per assert | `fact_store.zig` |
| 3 | Bulk / transient load | one-fact-at-a-time HAMT path-copy | `fact_store.zig`, `hamt.zig`, `lang.zig` |

> Background: [The Fact Store](22-fact-store.md), [Reification](12-reification.md),
> [The Performance Model](18-performance-model.md). Source of truth for what
> ships: `ROADMAP.md`.

The three are **additive but not independent**. Two couplings drive the
sequencing (§5):

- **Lever 1 ∩ Lever 3 overlap on the load path.** `frozenInsert`
  (`fact_store.zig:895`) is already the "current-values-only" shape of lever 3
  and writes no history — so once a transient batch path exists, it subsumes
  lever 1's benefit *for batch loads*. Lever 1's unique value is the paths
  lever 3 will never cover: live single-form file-in that **redefines** a method,
  and runtime code mutation. Build lever 1 anyway; it is cheap and it is the
  correct general mechanism.
- **Lever 3 must honor Lever 2's policy.** A transient builder has to be told
  *which* indexes to build; if it isn't, it builds all five and lever 2's saving
  evaporates. So lever 2's per-attribute policy is a **prerequisite** for lever 3.

Everything below preserves the tenet from [Reification](12-reification.md): code
stays **queryable, diffable, replaceable, module-attributed**. We give up only
`asOf:`-over-the-compiler's-intermediate-states and value-indexed lookup of
bytecode bytes — neither of which anyone queries.

---

## 0. Shared foundation — the per-attribute policy

All three levers need one new thing the store does not have today: **a way to
ask "what tier is this attribute?"** on the hot assert path. The store has
exactly one piece of per-attribute metadata now — `CardinalitySchema`
(`fact_store.zig:47-57`), queried per-assert by `cardinalityOfAttr`
(`fact_store.zig:1000-1008`). That is the template. We add a sibling.

### 0.1 Two sources of policy, one query

Code attributes come in two flavors and we treat them differently to keep the
hot path cheap:

1. **Built-in code attributes** — the fixed kernel EIDs enumerated below. These
   are compile-time constants, so membership is a branch-free range/set test —
   **no fact lookup**. This is the bulk of the win and costs nothing per assert.
2. **User opt-in** — a user attribute that asks to be `:no-history` / `:no-index`
   via a schema fact (Datomic's `:db/noHistory` / `:db/index false`). This keeps
   facts as the source of truth and is queryable, but costs a `rawCurrentValue`
   lookup like cardinality does. Optional; ship it second if at all.

The store exposes one resolver that ORs the two:

```zig
// fact_store.zig — new
pub const AttrTier = packed struct {
    no_history: bool = false,
    skip_avet:  bool = false,
    skip_vaet:  bool = false,
};

pub fn attrTier(self: *const FactStore, a: u64) AttrTier {
    if (kernel_eids.codeAttrTier(a)) |t| return t;     // static, no lookup
    return self.userAttrTier(a);                        // schema lookup, or {} if no schema
}
```

`codeAttrTier` lives next to the EID constants so the list can never drift from
the constants it names.

### 0.2 The built-in code-attribute set (`kernel_eids.zig`)

From the reification pipeline (`compiler.zig`, `lang.zig:newAstNode`,
`materialize`), the **exclusively-code** attributes are:

- **AST-node payload** (`kernel_eids.zig:296-378`): `ATTR_AST_RULE_NAME`(91),
  `ATTR_AST_FIRST_CHILD`(92), `ATTR_AST_NEXT_SIBLING`(93), `ATTR_AST_BYTE_START`(94),
  `ATTR_AST_BYTE_END`(95), `ATTR_AST_PARSE`(96), and `ATTR_AST_VALUE`(118)
  through `ATTR_AST_STMT_FIRST`(134).
- **Method / bytecode** (`kernel_eids.zig:429-486`): `ATTR_BYTECODE_STREAM`(165),
  `ATTR_CONSTANT_POOL`(166), `ATTR_METHOD_BODY_AST`(167), `ATTR_METHOD_SENDS`(168),
  `ATTR_METHOD_READS_SLOT`(169), `ATTR_METHOD_AST_RANGE`(170),
  `ATTR_METHOD_SEND_SITE_COUNT`(171), `ATTR_METHOD_LOCAL_COUNT`(172),
  `ATTR_METHOD_ARG_COUNT`(173), `ATTR_BLOCK_PARENT_METHOD`(174),
  `ATTR_RANGE_METHOD`(175)–`ATTR_RANGE_AST`(178), `ATTR_METHOD_CAPTURES_PTR`(183).

These already cluster in two contiguous EID bands (91–96, 118–134) and
(165–178, 183), so `codeAttrTier` is two range checks plus one equality:

```zig
// kernel_eids.zig — new
pub fn codeAttrTier(a: u64) ?fact_store.AttrTier {
    const code =
        (a >= 91 and a <= 96) or (a >= 118 and a <= 134) or
        (a >= 165 and a <= 178) or a == ATTR_METHOD_CAPTURES_PTR;
    if (!code) return null;
    // Write-once, never queried by value: drop history + the two costly indexes.
    return .{ .no_history = true, .skip_avet = true, .skip_vaet = true };
}
```

> ⚠️ **Do NOT include `ATTR_VTABLE`(19).** It is asserted on every AST node and
> method (`lang.zig:935`, `compiler.zig:618`) **but also on every data entity and
> cell**, and dispatch reads it constantly (`vtableEidOf`). It is a shared
> attribute, not a code attribute. Flagging it would silently change behavior for
> all objects. Likewise leave `:cardinality` and any attribute that a `:many`
> method-metadata fact points *to* alone — only the attributes in the list above.

### 0.3 User opt-in (optional, second)

If/when wanted: define `ATTR_NO_HISTORY`, `ATTR_DB_INDEX` kernel attributes and a
`AttrPolicySchema { no_history_attr, index_attr, false_value }` injected struct
mirroring `CardinalitySchema` (`fact_store.zig:240`), with `userAttrTier` doing
the same `rawCurrentValue` lookup `cardinalityOfAttr` does. Until a workload
needs it, `userAttrTier` returns `.{}` and the schema is `null` — zero cost.

**Recommendation:** ship 0.1–0.2 only for the first cut. The static set covers
100% of the `logue`-class load problem. Defer 0.3.

---

## 1. Lever 1 — the `:no-history` tier

**Goal:** for an attribute with `no_history`, keep `current_v` but never allocate
an RRB history entry. Attacks property #1 directly, at the source.

### 1.1 Where history is born

`applyAssert` (`fact_store.zig:931-947`) updates EAVT and AEVT via
`updateValueLeaf`, and MEAT via `updateMeat`. The history allocation is a single
line inside `updateValueLeaf` (`fact_store.zig:1515`):

```zig
const new_hist_vec = try cell.history.push(new_hist);   // ← the RRB append we want to skip
```

`updatePresenceLeaf` (the AVET/VAET path) has the analogous
`cell.history.push` for `PresenceHist`.

### 1.2 The change

Thread the tier into the leaf updaters and make the push conditional:

```zig
// updateValueLeaf gains a `no_history: bool` param
const new_hist_vec = if (no_history)
    cell.history                       // reuse the (empty) vec — never grows
else
    try cell.history.push(new_hist);
const new_cell = ValueCell{ .current_v = new_current, .history = new_hist_vec };
```

In `applyAssert`, resolve the tier once and pass it down:

```zig
fn applyAssert(self, e, a, v, m, tx, bt) !void {
    const tier = self.attrTier(a);
    self.eavt = try updateValueLeaf(self.indexAlloc(), self.eavt, e, a, v,
        .{ .v = v, .tx = tx, .op = .assert, .basis = bt }, tier.no_history);
    self.aevt = try updateValueLeaf(self.indexAlloc(), self.aevt, a, e, v,
        .{ ... }, tier.no_history);
    self.meat = try updateMeat(..., tier.no_history);
    // AVET/VAET handled by Lever 2 below
}
```

For a `no_history` cell, `cell.history` stays the empty RRB (`ValueHistVec.empty`
from `fact.zig:59-69`), which allocates nothing. `current_v` updates normally, so
`currentValue` reads are byte-for-byte unchanged.

### 1.3 Semantics & blast radius

- **`asOf:` / `history` on a no-history attr** → returns only the current value
  (or empty history). This is the *intended* loss: no one time-travels the
  bytecode emitter's intermediate states.
- **`compact` / `prune`** are unaffected — they iterate current values via
  `meatIter` (`compact`, `fact_store.zig:1344`) and rebuild with `compactInsert`,
  which already writes minimal history. A no-history attr simply has less to
  collapse.
- **Cardinality `:one` implicit-retract** (`applyAssert`, the
  `cardinalityOfAttr` branch) still runs for value correctness; it just doesn't
  append a retract *history* entry when `no_history`. Confirm the presence cell's
  `present` flag still flips (it must, for index correctness) — only the history
  vec is skipped.

### 1.4 Test

`fact_store_test.zig`: assert a no-history attr N times, assert
`historyVec.len == 0` and `currentValue == last`; assert a normal attr still
historizes. Add a regression that `asOf:` a no-history attr returns current.

---

## 2. Lever 2 — index selectivity

**Goal:** for an attribute with `skip_avet` / `skip_vaet`, never insert it into
AVET / VAET. Drops most code facts from five indexes to three (EAVT, AEVT, MEAT —
keeping MEAT so module-unload still works). Attacks property #3. The doc estimates
this roughly **halves retained** memory for code, because AVET/VAET are the two
triple-nested HAMTs (`fact_store.zig:40-44`).

### 2.1 The change

In `applyAssert` (`fact_store.zig:931-947`), guard the two presence-leaf updates:

```zig
if (!tier.skip_avet)
    self.avet = try updatePresenceLeaf(self.indexAlloc(), self.avet, a, v, e, true,
        .{ .tx = tx, .op = .assert, .basis = bt });
if (!tier.skip_vaet)
    self.vaet = try updatePresenceLeaf(self.indexAlloc(), self.vaet, v, a, e, true,
        .{ ... });
```

Same guard on the implicit-retract presence updates in that function, and the
symmetric guards in `applyRetract`. `frozenInsert` and `compactInsert`
(`fact_store.zig:895`, `:911`) get the identical guard so a compaction does not
silently re-materialize the indexes we skipped on the live path.

### 2.2 The correctness obligation — the query planner

This is the one lever with a **read-side** consequence. Today the datalog engine
may resolve a value-bound clause (`#[?e :attr value]`) by probing AVET. For a
`skip_avet` attribute that index is now empty, so such a probe returns nothing
instead of erroring — a **silent wrong answer**.

Mitigation, in order of preference:

1. The planner already prefers EAVT/AEVT when the entity is bound; for a
   value-bound clause on a no-index attr it must fall back to an **AEVT scan +
   value filter** (linear over that attr's entities) rather than an AVET probe.
   Locate the index-choice in `query.zig` (the AVET/VAET selection) and gate it
   on `attrTier(a).skip_avet`.
2. Reverse-ref walks (`VAET`) are only meaningful for `ref`-valued attributes; the
   code attrs we skip are blobs/ints/structural eids that no one reverse-walks.
   Confirm `pull`'s reverse-attr path (`pull.zig`) never targets a skipped attr.

Without step 1 this lever is unsafe. **It is the gating task for Lever 2** — do
not ship the write-side skip without the read-side fallback.

### 2.3 Test

`query_test.zig`: assert a code-tier fact, run a value-bound query on it, assert
the planner returns the right row via the AEVT-scan fallback (not an empty AVET
probe). Plus a memory assertion in `fact_store_test.zig`: after N code-tier
asserts, `avet`/`vaet` roots are still empty.

---

## 3. Lever 3 — bulk / transient load

**Goal:** stop path-copying five HAMTs once per assert during file-in. Build the
indexes for a batch of facts in a **transient** (mutable-until-frozen) HAMT and
publish once. Attacks property #2 at the source. This is the largest item and
the only one that touches `hamt.zig`.

### 3.1 Why `frozenInsert` is not enough

`frozenInsert` (`fact_store.zig:895-901`) already inserts current-values-only
with no history and no hooks — but its doc warns it "skips implicit-retract and
the mutate hooks, so it is unsafe where a load redefines an existing method"
(`19-load-memory.md`). Live file-in *can* redefine: re-filing a source file that
reinstalls a selector. So the safe general form must (a) preserve implicit-retract
for `:one` attrs and (b) fire `on_attr_mutate` (mcache flush) — exactly what
`compactInsert` does but without the full-store rebuild.

### 3.2 The transient HAMT (`hamt.zig`)

Add a Clojure-style transient to the CHAMP HAMT (`hamt.zig`):

```zig
// Owns its freshly-allocated nodes; mutates them in place instead of path-copying.
pub fn Transient(comptime ... ) type { return struct {
    pub fn from(persistent: Hamt(...)) @This()   // shares existing nodes, copies on first touch (ownership tag)
    pub fn assocMut(self: *@This(), k, v) !void  // in-place when node is owned, copy-once otherwise
    pub fn persistent(self: *@This()) Hamt(...)   // freeze: stop allowing mutation, return persistent root
}; }
```

Ownership is tracked per node (an `owner` token / generation), the standard
transient trick: the first mutation of a shared node copies it and tags it owned;
subsequent mutations within the same batch mutate in place — turning N
path-copies into N in-place writes plus one copy of each touched spine node.

### 3.3 The batch API on FactStore

```zig
// fact_store.zig — new
pub const Batch = struct {
    eavt_t: EAVTish.Transient, aevt_t: ..., avet_t: ..., vaet_t: ..., meat_t: ...,
    touched_attrs: std.AutoHashMapUnmanaged(u64, void),  // for one mcache flush at commit
};

pub fn beginBatch(self: *FactStore) Batch;       // seed transients from current roots
pub fn batchAssert(self: *FactStore, b: *Batch, e, a, v, m) !void;  // tier-aware, in-place
pub fn commitBatch(self: *FactStore, b: *Batch) !void;  // freeze all five, swap roots, flush hooks
```

`batchAssert` is `applyAssert` rewritten against the transients: it consults
`attrTier(a)` (Levers 1+2 — skip history, skip avet/vaet), and it preserves
`:one` implicit-retract by tracking per-(e,a) prior values seen **within the
batch** plus the live store. `commitBatch` calls `persistent()` on each
transient, assigns the new roots, bumps `next_tx`/`basis_t` by the batch size,
and fires `on_attr_mutate` **once per distinct touched attr** (not per fact).

### 3.4 Wiring file-in

`doFileInSource` (`lang.zig:2901-2954`) currently evaluates each form, which
asserts facts one at a time, then calls `api.maybe_prune()` per form
(`lang.zig:2942`) and `api.prune_now()` after the loop (`lang.zig:2954`).

The batch path slots in **underneath** eval. Two options:

- **Per-form batch (lower risk):** wrap each form's compile+materialize in
  `beginBatch`/`commitBatch`. The wrapper method's run-and-discard subtree still
  gets created, but as one transient publish instead of hundreds of path-copies.
  `maybe_prune` still sweeps it afterward. Smaller blast radius; recommended
  first cut.
- **Multi-form batch (higher win):** batch K forms before committing. More
  saving, but a redefinition within the uncommitted window needs the
  within-batch prior-value tracking to be correct. Ship only after the per-form
  path is proven.

This needs a new ABI hook pair (`begin_batch` / `commit_batch`) appended to
`IdApi` per §6, since file-in lives in the `lang` module across the dylib
boundary. Alternatively, keep batching entirely kernel-side behind the existing
`eval_top_level` call (`id_api.zig:589`) so no new ABI surface is needed —
**preferred**, because it batches the compiler's own asserts (the bulk of the
facts) without the module having to orchestrate it.

### 3.5 Safety checklist (this lever can corrupt the store if rushed)

- Implicit-retract for `:one` attrs must match `applyAssert` exactly, including
  the within-batch case (form 2 redefines what form 1 asserted).
- `on_attr_mutate` (mcache flush) **must** fire for every touched attr, or a
  stale method cache survives a redefinition → wrong dispatch.
- Transient nodes must never escape into a published root un-frozen.
- On any error mid-batch, the partial transient is dropped and the live roots are
  untouched (transients are built off-to-the-side; roots swap only at commit) —
  this gives file-in its atomic-on-error property for free, matching the
  `continue_on_error == false` path (`lang.zig:2925`).

### 3.6 Test

`fact_store_test.zig`: (1) a batch of plain asserts produces byte-identical
current values to the same asserts via `assert`; (2) a batch where form 2
redefines form 1's `:one` value lands the *second* value and fires the attr hook;
(3) an error mid-batch leaves the store at its pre-batch roots. Then an
integration check: file in `logue` and confirm peak RSS drops below the
Lever-1+2 baseline.

---

## 4. What each lever buys (recap against the measured bands)

From [Load-Time Memory](19-load-memory.md) the no-prune peak grows linearly
(3.2 GB → 13.8 GB across 400 → 1600 forms) and the shipped prune levers flatten
it to ~800 MB. These three reduce the **constant factor and the steady state**
underneath that flat line:

- **Lever 1** removes the per-fact RRB history for ~all code facts — the bulk of
  retained code-fact bytes that survive a compaction.
- **Lever 2** drops code facts from 5 → 3 indexes — roughly halves *retained*
  code-fact memory (the two skipped indexes are the triple-nested ones).
- **Lever 3** removes the transient path-copy garbage *during* load — lowers the
  allocation churn (and thus peak between prune fires), complementing the prune
  levers rather than replacing them.

---

## 5. Sequencing & dependencies

```
§0 policy foundation (static codeAttrTier)        ← prerequisite for all
        │
        ├──► Lever 1 (no-history)                  ← independent, ship early
        │
        ├──► Lever 2 (index selectivity)           ← needs §2.2 query fallback
        │
        └──► Lever 3 (transient batch)             ← needs §0 + Lever 2 policy;
                                                      subsumes Lever 1 for batch loads
```

Recommended order:

1. **§0.1–0.2** — the static `codeAttrTier` set. Pure addition, no behavior change
   until a lever consumes it. Land + test in isolation.
2. **Lever 1** — smallest, highest direct retained-memory win, no read-side risk.
3. **Lever 2** — *together with* its query-planner fallback (§2.2); do not split.
4. **Lever 3** — per-form batch first (§3.4 option A), kernel-side (no new ABI),
   then consider multi-form.
5. **§0.3** user opt-in — only if a workload asks for it.

Each step is a self-contained commit with its own tests, per the project's
phase-as-commit discipline.

---

## 6. ABI & append-only checklist

If Lever 3 ends up exposing batch control across the dylib boundary (§3.4 option
that is *not* preferred), follow the discipline that landed `maybe_prune` /
`prune_now`:

- New `IdApi` fields **append at the END** of the `extern struct`
  (`module_abi.zig:14-16` — the append-only rule), never mid-struct; older
  compiled modules read a shorter prefix and must stay layout-compatible.
- Each hook: a `*const fn (...) callconv(.c)` field in `IdApi` with a doc comment
  marked *"(Appended at END per the append-only ABI discipline.)"*, a `cXxx`
  wrapper in `id_api.zig` retrieving kernel state via `send_mod` singletons
  (pattern: `cMaybePrune` / `cPruneNow`, `id_api.zig:752-777`), and a
  `.xxx = &cXxx,` entry in `populate()` (`id_api.zig:44-195`).
- Errors swallowed (`catch return`) — a failed batch/reclaim must never abort the
  file-in, matching the existing hooks.

**Preferred path keeps Lever 3 kernel-side** behind the existing
`eval_top_level` ABI call (`id_api.zig:589`), needing **no new ABI surface**.

---

## 7. Risks & non-goals

- **`ATTR_VTABLE` contamination** (§0.2) — the single most likely correctness
  bug. The code-attr set must exclude every shared attribute. A test that asserts
  a *data* entity still historizes and still indexes its `:vtable` guards this.
- **Lever 2 silent wrong answers** (§2.2) — shipping the write-skip without the
  read fallback. Gate them together.
- **Lever 3 implicit-retract / mcache flush** (§3.5) — the redefinition cases are
  where a transient batch can corrupt dispatch. Per-form batching first keeps the
  window to one form.
- **Non-goal: AST granularity** (deferred lever #4 in `19-load-memory.md`) —
  treating the AST as a re-parseable cache instead of one-entity-per-node. That
  partially betrays "the AST is live-editable facts" and is out of scope here; it
  reduces the *fact count*, a different axis than these three (which reduce
  per-fact cost). Revisit only if 1–3 prove insufficient.
- **Non-goal: changing the fact model.** Reflection, live mutation, module
  attribution, and the five-index *read* surface are unchanged. Code facts remain
  the source of truth; we change only how cheaply they are stored.
