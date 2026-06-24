# RFC: Transient Load Mode for File-In

**Status:** proposed, unscheduled.
**Depends on:** the per-attribute tier policy in
[Code-Fact Tiers](19a-code-fact-tiers-plan.md) §0.
**Motivates:** the third deferred lever in [Load-Time Memory](19-load-memory.md)
("Bulk/transient load").

This is the third source-level lever, carved out of the
[Code-Fact Tiers plan](19a-code-fact-tiers-plan.md) because — unlike the
`:no-history` and skip-VAET tiers, which are passive guards on the existing
assert path — it requires a new write *mode* and a new HAMT capability, with its
own atomicity and arena interactions.

> **Not a correctness fix.** The shipped prune levers
> ([Load-Time Memory](19-load-memory.md) §Levers 1–2) already bound load-time
> *peak* to ~800 MB regardless of program size. This RFC reduces the **allocation
> churn underneath** that ceiling — the per-assert path-copy garbage generated
> *during* a load, before the next prune fires. It is an optimization, sequenced
> after Levers 1–2.

---

## 1. The problem

[Load-Time Memory](19-load-memory.md) and
[The Performance Model](18-performance-model.md) identify the cost: the compiler
asserts one fact at a time, and each `assert` **path-copies all five HAMTs**
(`fact_store.zig:applyAssert`, `~931`), abandoning the intermediate spine nodes.
A `logue`-sized load is a few hundred thousand asserts; the dead spine nodes
accumulate until the next `compact` frees them wholesale. Levers 1–2 shrink each
fact (no history, one fewer index); this lever attacks the **per-assert
path-copy itself** — the property #2 of the load-memory page.

The fix in spirit is the one the load-memory page names: *"Build the indexes once
from the whole form batch (a transient HAMT builder frozen on publish) instead of
path-copying five HAMTs per assert."*

---

## 2. The dead-end: deferred-publish batching (and why it fails)

The obvious shape — accumulate a form's facts off to the side in a transient,
publish at a commit point — is **unsafe**, and discovering why is what produced
this RFC. Two failed anchor points:

1. **Batch around `eval_top_level`.** `cEvalTopLevel` calls `runMethod`
   (`id_api.zig:618`): it *runs user code*. A batch that hides writes until commit
   would serve stale reads to any lookup/dispatch/query inside `runMethod`. Also,
   the bulk of the facts — the AST nodes — are asserted *before* this, in
   `parseExpr:` (`lang.zig:2915`), so the batch would miss them.

2. **Batch around the parse phase.** This is worse, not better: **parsing reads
   the store mid-parse, including facts it just wrote.** `parseExprImpl` runs the
   PEG parser (writes CST facts) then `desugar` (`lang.zig:1129`), which reads
   those facts back through `cstRuleName` / `cstFirstChild` / `cstNextSibling` →
   `query_eavt` → `currentValue` (`lang.zig:900-927`), and reads its own
   just-asserted AST at `lang.zig:1311-1312`. Selector/symbol interning during
   parse also writes `[eid :name …]` facts that later lookups read. There is no
   point during parse at which writes can be deferred without breaking reads.

**Conclusion:** publication cannot be deferred. The win must come from cheaper
writes that remain *immediately visible to reads* — not from hiding them.

---

## 3. The mechanism: read-through transient load mode

Make the store's **live indexes themselves** transient (owned, mutable-in-place)
for the duration of a load. `assert` mutates in place instead of path-copying;
reads (`currentValue`, `query_eavt`, `hasFact`) pass through the *same* transient,
so `desugar` and interning see their own writes exactly as today. Freeze back to
persistent at load end.

```zig
// fact_store.zig — new
pub fn beginTransientLoad(self: *FactStore) !void;  // save persistent roots; swap in transients seeded from them
pub fn endTransientLoad(self: *FactStore) !void;    // freeze transients → persistent roots
pub fn abortTransientLoad(self: *FactStore) void;   // discard transients; restore the saved roots
```

- `beginTransientLoad` snapshots the current persistent roots
  (`eavt/aevt/avet/vaet/meat`) and replaces each with a `Transient` seeded from it
  (copy-on-first-touch — see §4).
- During the load, `assert` / `retract` route through `assocMut` on the
  transients (tier-aware: honors `no_history` and `skip_vaet` from
  [§0](19a-code-fact-tiers-plan.md)). Reads are unchanged — `Transient.get`
  answers from the in-flight structure.
- `endTransientLoad` calls `persistent()` on each transient and stores the frozen
  roots; the store is back to normal persistent operation.

The win: N asserts become N in-place writes plus one copy of each *touched* spine
node, instead of N full path-copies. Reads stay correct because there is no
deferral.

### 3.1 Wiring

`doFileInSource` (`lang.zig:2901`) brackets the whole form loop:
`beginTransientLoad` before, `endTransientLoad` after (before the existing
`prune_now`). A mid-load error path calls `abortTransientLoad` (§5). This needs
two ABI hooks (§6).

---

## 4. The CHAMP transient (`hamt.zig`)

`hamt.zig`'s `Node` (`:45-53`) has **no owner field**, `root` is `?*const Node`
(immutable by type, `:36`), and every `assoc` allocates fresh `kvs`/`subnodes`.
A Clojure-style transient adds:

- an **owner token** per `Node`;
- `assocMut(k, v)` — mutate in place when the node is owned by this transient;
  copy-once-and-tag-owned on first touch of a shared (persistent) node;
- `get(k)` — identical to the persistent read (transients answer reads);
- `persistent()` — clear the owner, return the now-frozen root;
- threading through **all nesting levels**: AVET/VAET are triple-nested and MEAT
  is HAMT-of-HAMT-of-HAMT, so the transient descends every level.

**Copy-on-first-touch is what makes §5 rollback possible:** the saved persistent
roots from `beginTransientLoad` keep pointing at the *original* nodes, which a
transient never mutates (it copies them before the first edit). So the pre-load
state remains intact and restorable for free.

History vectors (RRB) during load: code attrs are `no_history` (Lever 1) so they
allocate none; the non-code facts a load writes (interner `:name`, scope
bindings) still want history — either give RRB a transient too, or accept that
those few facts path-copy their history vec normally (they are a small minority of
a code load). Recommend the latter for the first cut.

---

## 5. Atomicity

Unlike deferred-publish (which rolled roots back trivially), read-through mutates
the live indexes in place — but rollback is still clean **because of
copy-on-first-touch (§4)**:

- **Commit:** `endTransientLoad` freezes; new persistent roots reflect the load.
- **Abort:** `abortTransientLoad` drops the transient roots and restores the saved
  pre-load persistent roots. The original nodes were never mutated, so the store
  is exactly as it was before `beginTransientLoad`. This preserves file-in's
  atomic-on-error contract (`continue_on_error == false`, `lang.zig:2925`).
- **Caveat (honest):** the transient's copied/allocated nodes from an aborted load
  are arena garbage not reclaimed until the next `compact` — the *visible* state
  is atomic, the *allocation* is not. Same caveat the deferred-publish design
  had.

---

## 6. Safety gate

Read-through in-place mutation is sound only when nothing else observes the
indexes mid-load. The gate mirrors `prune`'s (`prune.zig:276-278`):

- **Single-threaded / quiescent** — file-in holds the VM at form boundaries; no
  concurrent reader.
- **Current-only retention, no live snapshot** — a retained epoch or snapshot
  pins a root and could observe a half-mutated index. If `retention_policy !=
  current_only` or `retained_epochs.items.len > 0`, **skip transient mode** and
  fall back to the normal per-assert path. (Lossless: you lose the optimization,
  not correctness.)
- **No `asOf:`/snapshot taken during the load** — the in-flight transient has no
  coherent historical basis. File-in does not do this.
- **Interaction with `index_arena`/`compact`:** `compact`'s epoch swap
  (`fact_store.zig:1352-1378`) must not run mid-load. It only fires at line/form
  boundaries *after* eval (`lang.zig:2942`), never inside a form, so a load
  bracket that spans the form loop must ensure no `maybe_prune`/`compact` fires
  between `beginTransientLoad` and `endTransientLoad` — i.e. the existing per-form
  `maybe_prune` (`lang.zig:2942`) moves *outside* the transient bracket, or is
  suppressed while a transient load is active.

---

## 7. ABI & append-only checklist

Two new hooks let the `lang` module bracket the load across the dylib boundary,
following the `maybe_prune` / `prune_now` precedent:

- `begin_transient_load` / `end_transient_load` (and an abort, or fold abort into
  end-with-error-flag), appended at the **END** of the `IdApi` extern struct
  (`module_abi.zig:14-16` append-only rule).
- Each: a `*const fn (...) callconv(.c)` field with a doc comment marked
  *"(Appended at END per the append-only ABI discipline.)"*, a `cXxx` wrapper in
  `id_api.zig` via `send_mod` singletons (pattern: `cMaybePrune`/`cPruneNow`,
  `id_api.zig:752-777`), a `.xxx = &cXxx,` entry in `populate()`.
- `begin` is a no-op (returns "not entered") when the safety gate (§6) fails, so
  the module always pairs begin/end and the kernel decides whether transient mode
  actually engaged.

---

## 8. Open questions

1. **RRB transient** — needed only if non-code load facts' history churn proves
   to matter; measure first (§4).
2. **Per-form vs whole-file bracket** — a single bracket over the whole form loop
   maximizes in-place reuse but means a load that trips the safety gate partway
   (e.g. a snapshot is somehow taken) must degrade gracefully. A per-form bracket
   is safer but reseeds transients each form (more copy-on-touch). Start
   whole-file; measure.
3. **Measurement** — the load-memory page's reproduction harness (real
   `FactStore` + `prune` + epoch arena) should be extended to count *allocator
   bytes churned* during load, not just peak RSS, since this lever targets churn
   under the already-flat peak.
4. **Does it even pay** after Levers 1–2 + prune? Levers 1–2 shrink each fact and
   prune bounds peak; this lever only removes transient path-copy *churn*. Build
   it only if profiling shows that churn (not retained size) is the remaining
   bottleneck.

---

## 9. Relationship to the other levers

- Honors the [§0 tier policy](19a-code-fact-tiers-plan.md) — `batchAssert`/`assocMut`
  consult `attrTier` so a transient load still skips history and VAET for code.
- Subsumes neither Lever 1 nor Lever 2: those shrink *what* is written (and apply
  to live redefinition and runtime mutation, which a load bracket never covers);
  this changes *how* it is written during a load.
- Strictly after them in sequence — it is the highest-risk, lowest-certainty
  item, and the prune levers already prevent the catastrophic-peak failure mode.
