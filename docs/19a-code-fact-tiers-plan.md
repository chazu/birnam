# Implementation Plan: Code-Fact Tiers (no-history + index selectivity)

This is the build plan for two of the three optimizations
[Load-Time Memory](19-load-memory.md) defers under *"What is deliberately
deferred."* Where levers 1–2 of that page reclaim garbage **after the fact** at
compaction boundaries (`prune.maybePrune` / `prune.pruneNowIfSafe`), the levers
here attack the same root-cause properties **at the source** — they stop the
waste from being written in the first place.

The third deferred lever — bulk/transient load — was found to need a different
mechanism than the others (it cannot be a passive write-path guard), so it is
split into its own design: **[RFC: Transient Load Mode](19b-transient-load-rfc.md)**.

> **This plan was adversarially reviewed twice and corrected.** The prior
> corrections are recorded in [§7](#7-corrections-from-the-adversarial-reviews) so
> the reasoning trail survives. Both levers below are verified ready to build.

| # | Lever | Root-cause property it kills | Status |
|---|---|---|---|
| 1 | `:no-history` tier for code attrs | per-fact history (RRB append per assert) | sound, ship first |
| 2 | Index selectivity — **skip VAET** for code attrs | the (never-read) reverse-ref triple-nested HAMT | safe; skip-AVET deferred |
| 3 | Bulk / transient load | one-fact-at-a-time HAMT path-copy | → [separate RFC](19b-transient-load-rfc.md) |

> Background: [The Fact Store](22-fact-store.md), [Reification](12-reification.md),
> [The Performance Model](18-performance-model.md). Source of truth for what
> ships: `ROADMAP.md`.

Everything below preserves the tenet from [Reification](12-reification.md): code
stays **queryable, diffable, replaceable, module-attributed**. The only capability
touched is `asOf:` over the compiler's intermediate states — which no one queries
— and skip-VAET removes nothing observable at all (§2.1).

---

## 0. Shared foundation — the per-attribute policy

Both levers need one new thing the store lacks: **a way to ask "what tier is this
attribute?"** on the hot assert path. The store has exactly one piece of
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

> **Invariant (load-bearing): `attrTier` must be a pure function of the attribute
> EID — never of store state.** The epoch-replay vehicle (`replayEventsInto`,
> `fact_store.zig:498-508`) rebuilds faulted-in/excised epochs through a throwaway
> `FactStore` calling `applyAssert`. That replay stays index-consistent with the
> original write *only* because `codeAttrTier` reads `kernel_eids` constants, not
> facts. A fact-driven policy (§0.3) would be invisible to the replay store and
> would VAET-skip on the live store but VAET-insert on replay (or vice versa),
> corrupting the epoch. If §0.3 ever ships, the policy must be propagated into the
> replay vehicle (and into `compact`'s rebuild).

### 0.2 The built-in code-attribute set (`kernel_eids.zig`)

From the reification pipeline (`compiler.zig`, `lang.zig:newAstNode`,
`materialize`), the **exclusively-code** attributes occupy contiguous bands:

- **AST-node payload** (`kernel_eids.zig:302-378`): `ATTR_AST_RULE_NAME`(91)
  through `ATTR_AST_PARSE`(96), and `ATTR_AST_VALUE`(118) through
  `ATTR_AST_STMT_FIRST`(134). *(The 97/98 gap is `ATTR_PEG_*`; the 135–139 /
  162–164 gaps are M_USER bind/scope **data** attrs — all correctly excluded.)*
- **Method / bytecode** (`kernel_eids.zig:429-486`): `ATTR_BYTECODE_STREAM`(165)
  through `ATTR_RANGE_AST`(178), and `ATTR_METHOD_CAPTURES_PTR`(183).

```zig
// kernel_eids.zig — new
pub fn codeAttrTier(a: u64) ?fact_store.AttrTier {
    const code =
        (a >= 91 and a <= 96) or (a >= 118 and a <= 134) or
        (a >= 165 and a <= 178) or a == ATTR_METHOD_CAPTURES_PTR;
    if (!code) return null;
    // Write-once: drop history (Lever 1) + the never-read VAET index (Lever 2).
    // skip_avet stays FALSE — see §2.4 (it breaks :many reads and retract).
    return .{ .no_history = true, .skip_vaet = true, .skip_avet = false };
}
```

> ⚠️ **Do NOT include `ATTR_VTABLE`(19).** It is asserted on every AST node and
> method (`lang.zig:935`, `compiler.zig:618`) **but also on every data entity and
> cell**, and dispatch reads it constantly. It is a shared attribute, not a code
> attribute.
>
> ⚠️ **The `165–178` band contains three `:many` attributes** —
> `ATTR_METHOD_SENDS`(168), `ATTR_METHOD_READS_SLOT`(169),
> `ATTR_METHOD_AST_RANGE`(170) (`kernel_eids.zig:440-446`). Safe under
> `skip_vaet` (`:many` reads use AVET, §2.1) but **unsafe under `skip_avet`**
> (§2.4). The static table must never blanket-set `skip_avet` over the range.

### 0.3 User opt-in (deferred)

A Datomic-style `:db/noHistory` / `:db/index false` per user attribute (a new
`AttrPolicySchema` mirroring `CardinalitySchema`) keeps facts as the source of
truth but costs a `rawCurrentValue` lookup per assert **and** breaks the
replay-consistency invariant (§0.1) unless threaded into the replay vehicle. The
static set covers 100% of the `logue`-class load problem, so **ship 0.1–0.2 only**
and defer 0.3 until a workload asks for it.

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
still updates `current_v` (which is what the hot read paths consult —
`rawCurrentValue` reads `cell.current_v`, `fact_store.zig:994-997`). Resolve
`tier` once at the top of `applyAssert` and pass `tier.no_history` to all five
leaf calls (and the mirror calls in `applyRetract`).

### 1.3 Semantics & blast radius

- **`asOf:` / `history` on a no-history attr** → returns only the current value.
  The intended loss.
- **`compact` / `prune` unaffected** — both walk *current values* (`compact` via
  `meatIter`, `fact_store.zig:1344`; `prune` via `currentValue`/`entitiesWith`,
  `prune.zig:94,102`), never history depth. Verified: nothing in the prune
  root-walk reads a history vector.
- **`:one` implicit-retract** (the `cardinalityOfAttr` branch in `applyAssert`)
  reads the prior value from EAVT (`rawCurrentValue`), not from history, so it is
  unaffected; it simply appends no retract *history* entry.

### 1.4 Test

`fact_store_test.zig`: (1) assert a no-history `:one` attr N times → `history.len
== 0`, `currentValue == last`; (2) assert a no-history `:many` attr → all current
values still returned via `currentValuesIter`; (3) a *data* attr still
historizes (guards against over-broad tiering); (4) `asOf:` a no-history attr
returns current.

---

## 2. Lever 2 — index selectivity (skip VAET)

**Goal:** for a code attribute, never insert it into **VAET**. VAET is one of the
two triple-nested HAMTs (`fact_store.zig:40-44`); maintaining it costs a per-assert
path-copy and retained memory.

> **Scope.** This lever skips **VAET only**. AVET is *not* safe to skip for code
> (§2.4). The win is one of the two costly indexes, not both.

### 2.1 Why skipping VAET removes nothing observable

VAET is the reverse-ref index (`HAMT<v, HAMT<a, HAMT<e, PresenceCell>>>`). Its
only semantic reader in the entire codebase is `pointingAt` (`fact_store.zig:1229`,
doing `self.vaet.get(v)` at `:1231`) — and **`pointingAt` has zero callers outside
tests, and is not exported in `module_abi.zig`, so no module can reach it.** (The
only other `.vaet.get` site is `exciseEntityFromRoots` at `:1463`, which null-no-ops
on a skip-VAET attr — see §2.3.) Everything that *sounds* like it would use VAET
does not:

- The reverse-ref ABI `cReverseRefs` (`id_api.zig:789`) — used by `pull`
  (`lang.zig:3511,3521`) — calls `entitiesMatching` (`id_api.zig:794`), which
  reads **AVET** (`fact_store.zig:1202-1203`), not VAET. **Reverse-ref queries
  keep working over code attrs after skip-VAET** because they run on AVET, which
  we retain.
- `prune`'s "find methods" root-walk goes forward —
  `currentValue(ATTR_BYTECODE_STREAM)` (`prune.zig:94`) and `entitiesWith`
  (`prune.zig:102`), both EAVT/AEVT.
- Dispatch, promotion/demotion, `compact`, and image freeze/load read MEAT/EAVT,
  never VAET.

So skip-VAET for code attrs is free: it corrupts no read path because there is no
live VAET read path. (A broader "don't maintain VAET at all" is tempting but
out of scope — it would strand the genuine-but-unwired reverse-ref index; scoping
to code attrs is the conservative cut.)

### 2.2 The one forward-looking caveat

The eid-valued code attrs (`:ast-receiver`, `:block-parent-method`, the range
eids) are genuine refs. *If* `pointingAt` is ever wired into a feature (a true
VAET-backed reverse walk), it would not cover code attrs. That is hypothetical —
no such caller exists today — and the forward path (AEVT/`entitiesMatching`)
already answers reverse-ref questions over these attrs. **No `DATALOG.md` change
is warranted**; nothing user-visible is lost.

### 2.3 The change

Guard the VAET presence update (and the VAET half of `:one` implicit-retract) in
`applyAssert` (`fact_store.zig:931-947`), the symmetric guard in `applyRetract`,
**and the same guard in `frozenInsert` (`:895`) and `compactInsert` (`:911`)** so
a compaction does not silently re-materialize VAET for skipped attrs:

```zig
if (!tier.skip_vaet)
    self.vaet = try updatePresenceLeaf(self.indexAlloc(), self.vaet, v, a, e, true, .{ ... });
```

**Write-path inventory** (all must agree, since the tier is static §0.1):
`applyAssert`, `applyRetract`, `frozenInsert`, `compactInsert`, and — read-only,
safe by construction — `exciseEntityFromRoots` (`fact_store.zig:1462`), which does
`roots.vaet.get(p.v)` then dissocs; for a skip-VAET attr the `.get` returns null
and the dissoc is a no-op, so excision stays consistent **as long as no future
edit adds a "rebuild VAET defensively" branch there.** AVET is left untouched
(`tier.skip_avet == false`), so `:many` reads, `hasFact`, and retract are
unchanged.

### 2.4 Why skip-AVET is deferred, not abandoned

A future skip-AVET tier is possible but needs a real design, not a range check —
AVET backs load-bearing reads:

1. `:many` current-value reads iterate AVET only (`materializeManyCurrent`,
   `fact_store.zig:1027-1041`); a skip-AVET tier must exclude every `:many` attr.
2. `hasFact` reads AVET (`:1145`) and gates `applyRetract` (`:953`); a skip-AVET
   `:one` `hasFact` must be re-routed through EAVT (`rawCurrentValue(e,a) == v`) —
   a genuine change to the retract gate.
3. Audit the other AVET consumers — `entitiesMatching`/`entitiesWith` used by
   `predicate.zig`, `rule.zig`, `saved_query.zig`, `loader.zig`, `freeze.zig`,
   `ffi_module.zig`, and `cReverseRefs` — and confirm none target a skipped attr.

A separate work item with its own tests. The VAET-only cut here is the safe,
shippable portion of Lever 2.

### 2.5 Test

`fact_store_test.zig`:
1. After N code-tier asserts, the `vaet` root is still empty while
   `eavt`/`aevt`/`avet`/`meat` are populated.
2. `currentValue` (both cardinalities), `hasFact`, and **retract** of a code fact
   all still work.
3. **`reverseRefs`/`entitiesMatching` over a code attr still returns its
   referrers** (it runs on the retained AVET — assert this *works*, do not assert
   it returns empty).
4. **Post-`compact` regression:** assert code facts, run `compact`/prune (the
   hottest rebuild path — `pruneNowIfSafe` fires on every file-in), then assert
   the `vaet` root is *still* empty and current values survive.
5. Image round-trip: `freeze`/`loadFrozen` use MEAT/EAVT (`image.zig:269,375`),
   not VAET, so the round-trip must survive provided `frozenInsert` carries the
   guard.
6. No regression of the existing VAET suite: the current `pointingAt` tests
   (`fact_store_test.zig:174,609`) use non-code attrs (e.g. `A_PARENT=202`, well
   outside the code bands), so they stay VAET-indexed and must keep passing — do
   not "fix" them to expect empty.

---

## 3. Bulk / transient load → separate RFC

The third deferred lever does not fit the "passive write-path guard" shape of
Levers 1–2. A naive deferred-publish batch is **unsafe** because the parse phase
reads the store mid-parse (`desugar` reads CST/AST back via `query_eavt`,
`lang.zig:900-927,1311`), so writes cannot be hidden until a commit. The correct
mechanism is a **read-through transient load mode**, which is a larger,
CHAMP-core change with its own atomicity and arena interactions.

See **[RFC: Transient Load Mode](19b-transient-load-rfc.md)**. It depends on this
plan's §0 policy (a transient builder must honor the same per-attr tier) but is
otherwise independent and unscheduled — the shipped prune levers already bound
load-time *peak* without it.

---

## 4. What each lever buys (against the measured bands)

From [Load-Time Memory](19-load-memory.md): no-prune peak grows linearly
(3.2 GB → 13.8 GB across 400 → 1600 forms); the shipped prune levers flatten it
to ~800 MB. These two lower the constant factor and steady state underneath:

- **Lever 1** removes per-fact RRB history for ~all code facts — the bulk of
  retained code-fact bytes that survive a compaction.
- **Lever 2 (VAET-only)** drops code facts from 5 → 4 indexes — removes one of the
  two triple-nested HAMTs. Roughly a quarter of code-fact index memory; a future
  skip-AVET tier (§2.4) would reclaim the rest.

---

## 5. Sequencing & dependencies

```
§0 policy foundation (static codeAttrTier: no_history + skip_vaet)   ← prerequisite for both
        │
        ├──► Lever 1 (no-history)        ← independent, ship first, no read-side risk
        │
        └──► Lever 2 (skip VAET)         ← needs §2.3 frozenInsert/compactInsert guards + §2.5 compact test
        ┄┄► (follow-ups) Lever 2b skip-AVET (§2.4); §0.3 user opt-in; the transient-load RFC (19b)
```

Recommended order: **§0.1–0.2 → Lever 1 → Lever 2 (VAET)**, each a self-contained,
separately-tested commit per the phase-as-commit discipline.

---

## 6. ABI & append-only checklist

Levers 1–2 add **no new ABI surface** — they are internal to the fact store, on
the existing `assert`/`retract` paths. (The transient-load RFC does add ABI; that
checklist lives there.) Nothing in this plan crosses the dylib boundary.

---

## 7. Corrections from the adversarial reviews

Two review passes against the source corrected the earlier drafts. Preserved so
the reasoning is auditable:

**Round 1:**
- **Lever 3 mechanism (B1/B2):** batching `eval_top_level` was wrong — it runs
  `runMethod` (`id_api.zig:618`) and the AST bulk is asserted earlier in
  `parseExpr:`. → led to re-anchoring, then (round 2) to splitting it out.
- **skip_avet (M1/M2):** breaks `:many` reads (`materializeManyCurrent`,
  `fact_store.zig:1027`) and retract (`hasFact`, `:1145`). → skip-AVET deferred.
- **Query premise (M3):** the engine already resolves value-bound clauses via an
  AEVT walk (`query.zig:444`); the "AVET probe" Case D is dead code (`:470`). →
  removed the fictional fallback prerequisite.

**Round 2:**
- **"Parse is write-only" was FALSE (BLOCKER):** `desugar` reads CST/AST facts
  back mid-parse (`lang.zig:900-927`, and reads its own `body_ast` at `:1311`). →
  Lever 3 cannot use deferred publication; moved to the read-through transient
  RFC (19b).
- **VAET ≠ reverse-refs (MAJOR):** `cReverseRefs`/`entitiesMatching` read **AVET**
  (`id_api.zig:794` → `fact_store.zig:1202`); the only VAET reader `pointingAt`
  (`:1229`) is dead. → §2.1/§2.2 rewritten (skip-VAET loses nothing observable);
  the §2.5 test corrected to assert reverse-refs *still work*.
- **Replay invariant (MINOR):** index consistency under `replayEventsInto`
  depends on `attrTier` being static. → recorded as the §0.1 invariant.
- **Excise path + post-compact test (MINOR):** `exciseEntityFromRoots` added to
  the §2.3 write-path inventory; a post-`compact` VAET-empty regression added to
  §2.5.

**Verified and kept across both rounds:** the EID bands are contiguous and
code-only; `ATTR_VTABLE` must be excluded; the `:many` attrs 168/169/170 are safe
under skip-VAET but not skip-AVET; `CardinalitySchema` is a faithful template;
Lever 1 does not break compact/prune; image save/load uses MEAT/EAVT; the
append-only ABI discipline.
