# Implementation Plan: The Three Deferred Code-Fact Tiers

This is the build plan for the three optimizations
[Load-Time Memory](19-load-memory.md) defers under *"What is deliberately
deferred."* Where levers 1–2 of that page reclaim garbage **after the fact** at
compaction boundaries (`prune.maybePrune` / `prune.pruneNowIfSafe`), the three
levers here attack the same root-cause properties **at the source** — they stop
the waste from being written in the first place.

> **This plan was adversarially reviewed and corrected.** An earlier draft made
> three load-bearing errors about the code (the query engine's index choice, the
> `:many` read path, and where file-in's asserts physically occur). Those are
> documented in [§8 Corrections](#8-corrections-from-the-adversarial-review) so
> the reasoning trail survives; the body below is the corrected design. The net
> effect: **Lever 1 ships as-is, Lever 2 is roughly half its claimed size, and
> Lever 3's mechanism is re-anchored.**

| # | Lever | Root-cause property it kills | Status |
|---|---|---|---|
| 1 | `:no-history` tier for code attrs | per-fact history (RRB append per assert) | sound, ship first |
| 2 | Index selectivity — **skip VAET** for code attrs | the reverse-ref triple-nested HAMT | safe; skip-AVET deferred |
| 3 | Bulk / transient load of the **parse phase** | one-fact-at-a-time HAMT path-copy | largest; CHAMP-core change |

> Background: [The Fact Store](22-fact-store.md), [Reification](12-reification.md),
> [The Performance Model](18-performance-model.md). Source of truth for what
> ships: `ROADMAP.md`.

The three are **additive but not independent**:

- **Lever 1 ∩ Lever 3 overlap on the load path, but neither subsumes the other.**
  `frozenInsert` (`fact_store.zig:895`) writes *one* history entry per fact (no
  *churn*, not *no history* — it still calls the unconditional `push` at
  `fact_store.zig:1515`/`:1548`). So even a transient batch path keeps writing
  1-element history vecs unless Lever 1's skip is also in effect. Lever 1's unique
  value is the paths Lever 3 will never cover: live redefinition and runtime code
  mutation. Build both.
- **Lever 3 must honor Lever 2's policy.** A transient builder is told *which*
  indexes to build; if it isn't, it builds all five and Lever 2's saving
  evaporates. Lever 2's per-attribute policy is a prerequisite for Lever 3.

Everything below preserves the tenet from [Reification](12-reification.md): code
stays **queryable, diffable, replaceable, module-attributed**. The only narrowed
capability is **reverse-ref lookup over code attributes** (§2) and `asOf:` over
the compiler's intermediate states — neither of which anyone queries.

---

## 0. Shared foundation — the per-attribute policy

All three levers need one new thing the store lacks: **a way to ask "what tier is
this attribute?"** on the hot assert path. The store has exactly one piece of
per-attribute metadata today — `CardinalitySchema` (`fact_store.zig:51-57`),
queried per-assert by `cardinalityOfAttr` (`fact_store.zig:1002-1008`). That is
the template. We add a sibling.

### 0.1 The resolver

```zig
// fact_store.zig — new
pub const AttrTier = packed struct {
    no_history: bool = false,   // Lever 1
    skip_vaet:  bool = false,   // Lever 2 (first cut)
    skip_avet:  bool = false,   // Lever 2 follow-up — see §2.4; default false
};

pub fn attrTier(self: *const FactStore, a: u64) AttrTier {
    if (kernel_eids.codeAttrTier(a)) |t| return t;   // static, no fact lookup
    return .{};                                       // user opt-in deferred — see §0.3
}
```

`codeAttrTier` lives next to the EID constants so the list can never drift from
the constants it names.

### 0.2 The built-in code-attribute set (`kernel_eids.zig`)

From the reification pipeline (`compiler.zig`, `lang.zig:newAstNode`,
`materialize`), the **exclusively-code** attributes occupy contiguous bands:

- **AST-node payload** (`kernel_eids.zig:302-378`): `ATTR_AST_RULE_NAME`(91)
  through `ATTR_AST_PARSE`(96), and `ATTR_AST_VALUE`(118) through
  `ATTR_AST_STMT_FIRST`(134). *(The 97/98 gap is `ATTR_PEG_*`, correctly
  excluded.)*
- **Method / bytecode** (`kernel_eids.zig:429-486`): `ATTR_BYTECODE_STREAM`(165)
  through `ATTR_RANGE_AST`(178), and `ATTR_METHOD_CAPTURES_PTR`(183).

```zig
// kernel_eids.zig — new
pub fn codeAttrTier(a: u64) ?fact_store.AttrTier {
    const code =
        (a >= 91 and a <= 96) or (a >= 118 and a <= 134) or
        (a >= 165 and a <= 178) or a == ATTR_METHOD_CAPTURES_PTR;
    if (!code) return null;
    // Write-once, never reverse-ref-walked: drop history + the VAET index.
    // skip_avet stays FALSE — see §2.4 (it breaks :many reads and retract).
    return .{ .no_history = true, .skip_vaet = true, .skip_avet = false };
}
```

> ⚠️ **Do NOT include `ATTR_VTABLE`(19).** It is asserted on every AST node and
> method (`lang.zig:935`, `compiler.zig:618`) **but also on every data entity and
> cell**, and dispatch reads it constantly (`vtableEidOf`). It is a shared
> attribute, not a code attribute. Flagging it would silently change behavior for
> all objects.
>
> ⚠️ **The `165–178` band contains three `:many` attributes** —
> `ATTR_METHOD_SENDS`(168), `ATTR_METHOD_READS_SLOT`(169),
> `ATTR_METHOD_AST_RANGE`(170) (`kernel_eids.zig:440-446`). They are **safe under
> `skip_vaet`** (`:many` reads use AVET, not VAET — see §2.1) but would be
> **unsafe under `skip_avet`** (§2.4). This is the single reason the static table
> must never blanket-set `skip_avet` over the range.

### 0.3 User opt-in (deferred)

A Datomic-style `:db/noHistory` / `:db/index false` per user attribute (a new
`AttrPolicySchema` mirroring `CardinalitySchema`, looked up like
`cardinalityOfAttr`) keeps facts as the source of truth and is queryable, but
costs a `rawCurrentValue` lookup per assert. The static set covers 100% of the
`logue`-class load problem, so **ship 0.1–0.2 only** for the first cut and defer
0.3 until a workload asks for it. Until then `attrTier` returns `.{}` for user
attrs — zero cost.

---

## 1. Lever 1 — the `:no-history` tier

**Goal:** for an attribute with `no_history`, keep `current_v` (and, for `:many`,
the presence-cell `present` flag) but never allocate an RRB history entry.
Attacks property #1 directly, at the source.

### 1.1 Where history is born

`applyAssert` (`fact_store.zig:931-947`) updates EAVT/AEVT via `updateValueLeaf`
and MEAT via `updateMeat`; AVET/VAET via `updatePresenceLeaf`. The history
allocation is a single line in each leaf updater — `cell.history.push(new_hist)`
at `fact_store.zig:1515` (value) and `:1548` (presence).

### 1.2 The change

Thread the tier into the leaf updaters and make the push conditional:

```zig
// updateValueLeaf / updatePresenceLeaf gain a `no_history: bool` param
const new_hist_vec = if (no_history)
    cell.history                       // reuse the (empty) vec — never grows
else
    try cell.history.push(new_hist);
```

`updatePresenceLeaf` keeps setting the `present` bool **unconditionally** — only
the history `push` is gated. So a `no_history` `:many` attr still reports its
current values correctly (presence flags intact), and a `no_history` `:one` attr
still updates `current_v`. Resolve `tier` once at the top of `applyAssert` and
pass `tier.no_history` to all five leaf calls (and the mirror calls in
`applyRetract`).

### 1.3 Semantics & blast radius

- **`asOf:` / `history` on a no-history attr** → returns only the current value.
  This is the intended loss.
- **`compact` / `prune` unaffected** — both walk *current values*
  (`compact` via `meatIter`, `fact_store.zig:1344`; `prune` via
  `currentValue`/`entitiesWith`, `prune.zig:94,102`), never history depth.
  Verified: nothing in the prune root-walk reads a history vector.
- **Cardinality `:one` implicit-retract** (the `cardinalityOfAttr` branch in
  `applyAssert`) reads the prior value from EAVT (`rawCurrentValue`), not from
  history, so it is unaffected; it simply appends no retract *history* entry.

> The earlier draft's instruction here ("confirm the presence flag still flips")
> conflated Lever 1 with Lever 2 and was incoherent. It is removed: Lever 1 never
> touches whether the presence cell exists — only whether its history grows.

### 1.4 Test

`fact_store_test.zig`: (1) assert a no-history `:one` attr N times → `history.len
== 0`, `currentValue == last`; (2) assert a no-history `:many` attr → all current
values still returned via `currentValuesIter`; (3) a *data* attr still
historizes (guards against over-broad tiering); (4) `asOf:` a no-history attr
returns current.

---

## 2. Lever 2 — index selectivity (skip VAET)

**Goal:** for a code attribute, never insert it into **VAET** (the reverse-ref
index). VAET is one of the two triple-nested HAMTs (`fact_store.zig:40-44`); code
values are never reverse-ref-walked, so this is pure dead weight for them.

> **Scope correction.** The earlier draft skipped both AVET and VAET and gated
> the lever on a query-planner fallback that **already exists** (§8, M3). AVET is
> *not* safe to skip for code (§2.4). This lever now skips **VAET only**. The win
> is smaller — one of the two costly indexes, not both — but it is correct.

### 2.1 Why VAET is safe and AVET is not

The read paths that depend on each index, verified:

- **VAET is read by exactly one path:** `pointingAt` (`fact_store.zig:1229`, which
  does `roots.vaet.get(v)` at `:1463`) and its ABI export `cReverseRefs`
  (`id_api.zig:789`). Nothing in dispatch, promotion/demotion, `compact`, or
  `prune` reads VAET. `prune`'s "find methods" root-walk goes **forward** —
  `currentValue(ATTR_BYTECODE_STREAM)` (`prune.zig:94`) and `entitiesWith`
  (`prune.zig:102`), both EAVT/AEVT. So dropping code attrs from VAET breaks only
  *reverse-ref queries over code attrs*, which we declare unsupported (§2.2).
- **AVET is read by hot, load-bearing paths:** `:many` current-value reads go
  through `materializeManyCurrent`, which iterates **AVET only**
  (`fact_store.zig:1018-1019` → `:1027-1041`); `currentValuesIter`
  (`:1060-1063`) and `hasFact` (`:1144-1149`) are AVET-only; `applyRetract` gates
  on `hasFact`. Skipping AVET would silently empty `:many` reads and no-op every
  code-fact retract. Hence skip_avet stays off (§2.4).

### 2.2 The narrowed capability (state it honestly)

The eid-valued code attrs (`:ast-receiver`, `:ast-first-child`,
`:block-parent-method`, `:method-body-ast`, the range eids) are genuine refs;
after this lever, `reverseRefs(attr, eid)` over them returns empty. No internal
caller does this today (the only `cReverseRefs` consumers are the ABI surface and
tests), and a tool that wants "which AST nodes point at entity X" can still walk
forward via EAVT/AEVT. This is the documented tradeoff, in the spirit of Datomic
not indexing an attribute — not a silent correctness hole.

### 2.3 The change

In `applyAssert` (`fact_store.zig:931-947`), guard the VAET presence update (and
the VAET half of `:one` implicit-retract), the symmetric guard in `applyRetract`,
and **the same guard in `frozenInsert` (`:895`) and `compactInsert` (`:911`)** so
a compaction does not silently re-materialize VAET for skipped attrs:

```zig
if (!tier.skip_vaet)
    self.vaet = try updatePresenceLeaf(self.indexAlloc(), self.vaet, v, a, e, true, .{ ... });
```

AVET is left untouched (`tier.skip_avet == false`), so `:many` reads, `hasFact`,
and retract are unchanged.

### 2.4 Why skip-AVET is deferred, not abandoned

A future skip-AVET tier is possible but needs a real design, not a range check:

1. It must exclude every `:many` attr (AVET is their only current-value index).
2. It must answer `hasFact`/`applyRetract` for skipped attrs without AVET — e.g.
   route `:one` `hasFact` through EAVT (`rawCurrentValue(e,a) == v`), which is a
   genuine change to the retract gate.
3. It must audit the other AVET consumers — `entitiesMatching`/`entitiesWith`
   used by `predicate.zig`, `rule.zig`, `saved_query.zig`, `loader.zig`,
   `freeze.zig`, `ffi_module.zig` — and confirm none target a skipped attr.

That is a separate work item with its own tests. The VAET-only cut here is the
safe, shippable portion of Lever 2.

### 2.5 Test

`fact_store_test.zig`: after N code-tier asserts, `vaet` root is still empty while
`eavt`/`aevt`/`avet`/`meat` are populated; `currentValue` (both cardinalities),
`hasFact`, and retract of a code fact all still work; `reverseRefs` over a code
attr returns empty while over a data attr it returns the referrers. Plus an
image round-trip test (§7) — `freeze`/`loadFrozen` use MEAT/EAVT
(`image.zig:269,375`), not VAET, so the round-trip must survive provided
`frozenInsert` carries the same guard.

---

## 3. Lever 3 — bulk / transient load of the parse phase

**Goal:** stop path-copying five HAMTs once per assert during file-in. Build the
indexes for a write-only batch in a **transient** (mutable-until-frozen) HAMT and
publish once. Attacks property #2 at the source. Largest item; the only one that
touches `hamt.zig`.

### 3.1 Where the asserts actually are (the mechanism correction)

The earlier draft proposed batching "kernel-side behind `eval_top_level`." That
is wrong on two counts (§8, B1/B2):

- `eval_top_level` **runs user code** — `cEvalTopLevel` calls `runMethod`
  (`id_api.zig:618`) right after `materialize`. A deferred-publish batch around it
  would interleave reads (variable lookups, dispatch, queries inside `runMethod`)
  with un-published writes → stale mid-eval reads.
- The **bulk of the facts — the AST nodes — are asserted during `parseExpr:`,
  not `eval_top_level`.** `doFileInSource` parses first (`lang.zig:2915`,
  `send1(sel_parse_expr_g)`) and only then evaluates (`:2918`). Each AST node is
  a `newAstNode` assert (`lang.zig:932-938`). Batching `eval_top_level` would
  miss them entirely.

So the batchable, genuinely write-only phase is **the parse**: `newAstNode` only
allocates eids and asserts — it reads nothing back from the store within a form
(the parser links children by in-memory eid, not by querying). The corrected
boundary per form is:

```
begin batch → parseExpr: (asserts the AST)  → commit batch
            → evalTopLevelStmt (materialize + runMethod — reads see published AST)
            → maybe_prune
```

Materialize (a handful of method/bytecode asserts per closure — far fewer than
the AST) stays on the normal path in the first cut; batching it too is a
follow-up that requires splitting `compileTopLevel`+`materialize` (write-only)
from `runMethod` (not) inside `eval_top_level`.

> **Verify before building:** confirm the PEG parser (`peg.zig`) and `newAstNode`
> never read the store mid-parse. If any rule queries back an asserted node, the
> parse-phase batch must publish at that point. This is the gating check for
> Lever 3.

### 3.2 The transient HAMT is a CHAMP-core change, not a bolt-on

`hamt.zig`'s `Node` (`:45-53`) has **no owner/generation field**, `root` is
`?*const Node` (immutable by type, `:36`), and every `assoc` allocates fresh
`kvs`/`subnodes` slices. A Clojure-style transient therefore needs:

- an **owner token** on every `Node`;
- a mutable path (`*Node` under ownership; first touch of a shared node copies +
  tags it owned, subsequent touches mutate in place);
- threading through **all nesting levels** — AVET/VAET are triple-nested and MEAT
  is HAMT-of-HAMT-of-HAMT, so the transient must descend every level, not just
  the outer map;
- interaction with `index_arena` (`indexAlloc`, `fact_store.zig:273-274`):
  transient nodes allocate from the same arena as the live store.

```zig
// hamt.zig — new
pub fn Transient(comptime ...) type { return struct {
    pub fn from(persistent: Hamt(...), owner: *anyopaque) @This()
    pub fn assocMut(self: *@This(), k, v) !void   // in-place when owned, copy-once otherwise
    pub fn persistent(self: *@This()) Hamt(...)    // freeze; no further mutation
}; }
```

This is a real, risky piece of infrastructure — size it accordingly.

### 3.3 The batch API on FactStore

```zig
pub const Batch = struct {
    eavt_t: EAVTish.Transient, aevt_t: ..., avet_t: ..., vaet_t: ..., meat_t: ...,
    touched_attrs: std.AutoHashMapUnmanaged(u64, void),
    count: u64,
};
pub fn beginBatch(self: *FactStore) Batch;       // seed transients from current roots
pub fn batchAssert(self: *FactStore, b: *Batch, e, a, v, m) !void;  // tier-aware
pub fn commitBatch(self: *FactStore, b: *Batch) !void;  // freeze, swap roots, flush hooks
```

`batchAssert` is `applyAssert` against the transients, consulting `attrTier(a)`
(skip history per Lever 1, skip VAET per Lever 2) and preserving `:one`
implicit-retract by tracking per-`(e,a)` prior values seen **within the batch**
plus the live store. `commitBatch` freezes all five transients, assigns the new
roots, bumps `next_tx`/`basis_t` by `count`, and fires `on_attr_mutate` **once
per distinct touched attr** (mcache flush).

### 3.4 Safety checklist

- Parse must be confirmed write-only (§3.1); commit happens **before** any
  `runMethod`, so eval reads always see published facts.
- `:one` implicit-retract must match `applyAssert` exactly, including the
  within-batch case (rare during pure parse, but required if materialize is later
  folded in).
- `on_attr_mutate` must fire for every touched attr at commit, or a stale mcache
  survives a redefinition.
- Transient nodes must never escape into a published root un-frozen.
- **Atomic-on-error is partial, state it honestly:** roots swap only at commit, so
  visible state rolls back on a mid-batch error (matching `continue_on_error ==
  false`, `lang.zig:2925`). But the aborted batch's transient nodes are allocated
  from `index_arena` and are **not reclaimed until the next `compact`** — only the
  visible state is atomic, not the allocation.

### 3.5 Test

(1) A parse batch yields byte-identical current values to the same asserts via
`assert`; (2) eval after commit sees the published AST; (3) a parse error
mid-batch leaves the store at its pre-batch roots; (4) integration: file in
`logue` and confirm peak RSS and allocation churn drop below the Lever-1+2
baseline.

---

## 4. What each lever buys (against the measured bands)

From [Load-Time Memory](19-load-memory.md): no-prune peak grows linearly
(3.2 GB → 13.8 GB across 400 → 1600 forms); the shipped prune levers flatten it
to ~800 MB. These three lower the constant factor and steady state underneath
that flat line:

- **Lever 1** removes per-fact RRB history for ~all code facts — the bulk of
  retained code-fact bytes that survive a compaction.
- **Lever 2 (VAET-only)** drops code facts from 5 → 4 indexes — removes one of the
  two triple-nested HAMTs. Roughly a **quarter** of code-fact index memory, not
  the half the earlier draft claimed (that assumed AVET too). A future skip-AVET
  tier (§2.4) would reclaim the rest.
- **Lever 3** removes the transient path-copy garbage *during* the parse phase —
  lowers allocation churn (and thus peak between prune fires), complementing the
  prune levers rather than replacing them.

---

## 5. Sequencing & dependencies

```
§0 policy foundation (static codeAttrTier: no_history + skip_vaet)   ← prerequisite for all
        │
        ├──► Lever 1 (no-history)                ← independent, ship first, no read-side risk
        │
        ├──► Lever 2 (skip VAET)                 ← needs §2.3 frozenInsert/compactInsert guards
        │
        └──► Lever 3 (parse-phase transient)     ← needs §0 + Lever 2 policy; CHAMP-core work
                 │
                 └──► (follow-up) fold materialize into the batch (split eval_top_level)
        ┄┄► (follow-up) Lever 2b skip-AVET (§2.4) and §0.3 user opt-in
```

Recommended order: **§0.1–0.2 → Lever 1 → Lever 2 (VAET) → Lever 3 (parse
batch)**, each a self-contained, separately-tested commit per the phase-as-commit
discipline. The two follow-ups (materialize batching, skip-AVET) are independent
and unscheduled.

---

## 6. ABI & append-only checklist

Lever 3's parse-phase batch needs the `lang` module to open/close a batch across
the dylib boundary, so it **does** add ABI surface (the earlier "no new ABI"
framing was based on the wrong mechanism — §8, B1). Two new hooks,
`begin_parse_batch` / `commit_parse_batch`, following the `maybe_prune` /
`prune_now` precedent:

- New `IdApi` fields **append at the END** of the `extern struct`
  (`module_abi.zig:14-16` — append-only; older compiled modules read a shorter
  prefix and must stay layout-compatible).
- Each: a `*const fn (...) callconv(.c)` field with a doc comment marked
  *"(Appended at END per the append-only ABI discipline.)"*, a `cXxx` wrapper in
  `id_api.zig` retrieving kernel state via `send_mod` singletons (pattern:
  `cMaybePrune`/`cPruneNow`, `id_api.zig:752-777`), and a `.xxx = &cXxx,` entry in
  `populate()`.
- Errors swallowed (`catch return`) — a failed batch must degrade to the
  per-assert path, never abort the file-in.

Alternatively, batch entirely inside the kernel's parse entrypoint if one exists
that the module calls as a single send — but `parseExpr:` is a module-level send
(`sel_parse_expr_g`) whose body does the `newAstNode` asserts, so the batch
boundary is naturally module-side. Confirm during implementation.

---

## 7. Risks & non-goals

- **`ATTR_VTABLE` contamination** (§0.2) — the highest-risk shared-attribute trap.
  A test asserting a *data* entity still historizes and still VAET-indexes its
  `:vtable` guards this.
- **`:many` attrs in the skip set** (§0.2) — safe under skip_vaet, fatal under a
  future skip_avet. The static table must keep `skip_avet == false`.
- **Lever 2 reverse-ref narrowing** (§2.2) — a behavior change, not a bug, but it
  must be documented in `DATALOG.md` (reverse-ref queries do not cover code
  attrs).
- **Lever 3 parse-is-write-only assumption** (§3.1) — the gating check. If any PEG
  rule reads the store mid-parse, the batch boundary must move.
- **Lever 3 transient scope** (§3.2) — a CHAMP-core change threaded through
  triple-nested indexes; budget it as such.
- **Non-goal: AST granularity** (deferred lever #4 in `19-load-memory.md`) —
  reduces *fact count*, a different axis than these three (per-fact cost). Out of
  scope.
- **Non-goal: changing the fact model.** Reflection, live mutation, module
  attribution, and the five-index *read* surface (minus code reverse-refs) are
  unchanged.

---

## 8. Corrections from the adversarial review

The first draft of this plan was reviewed against the source. The load-bearing
errors it caught, preserved here so the corrected design's reasoning is auditable:

- **B1/B2 — Lever 3 mechanism.** Batching `eval_top_level` was wrong: it runs
  `runMethod` (`id_api.zig:618`, reads interleave with writes) and the AST bulk is
  asserted earlier in `parseExpr:` (`lang.zig:2915`), outside it. → Re-anchored to
  the parse phase (§3.1).
- **M1 — `skip_avet` breaks `:many` reads.** `materializeManyCurrent`
  (`fact_store.zig:1027`) iterates AVET only; attrs 168/169/170 in the skip range
  are `:many`. → skip-AVET dropped from the first cut (§2.1, §2.4).
- **M2 — `skip_avet` breaks retract; §1.3 self-contradiction.** `hasFact` reads
  AVET (`:1145`) and gates `applyRetract`; the old §1.3 wanted the AVET index gone
  yet its presence flag maintained. → skip-AVET deferred; §1.3 rewritten.
- **M3 — Lever 2's gating premise was false.** The query engine already resolves
  value-bound clauses via an AEVT walk + value filter (`query.zig:444`, Case C);
  the "AVET probe" Case D is dead code (`query.zig:470`). The fallback the draft
  demanded already exists. → §2.2 replaced with the real VAET-consumer audit.
- **N1/N2 — overstatements.** `frozenInsert` writes one history entry, not zero
  (intro); transient atomic-on-error rolls back visible state but not arena
  allocations (§3.4). → both corrected.
- **VERIFIED and kept:** the EID bands are contiguous and code-only; `ATTR_VTABLE`
  must be excluded; `CardinalitySchema` is a faithful template; Lever 1 does not
  break compact/prune; image save/load uses MEAT/EAVT (Lever 2 survives the
  round-trip); the append-only ABI discipline.
