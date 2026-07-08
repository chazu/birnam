# 42 — The Object Model is Canonical; the Relational Reading is Derived

> **Status:** decision **accepted 2026-07-05**. **Supersedes the primacy claim of
> [10a — The Cell Refounding](../archive/legacy-kernel/10a-north-star-cell-model.md)** (and the "code is a
> cache of facts" thesis it carries). It does **not** delete 10a's *mirror
> mechanism* — it reverses its *polarity*. Settles all of audit §D
> ([41 — MVP Unity Audit](../archive/work-breakdowns/41-mvp-unity-audit.md)).
>
> Background: [13 — The Object Model](../archive/legacy-kernel/13-object-model.md),
> [26 — Dispatch](../archive/legacy-kernel/26-dispatch.md), [37 — The Relational Reading](37-the-relational-reading.md),
> [33 — Rung 2 / the store](33-*), [40 — The Image](40-the-image.md).

## The reversal

`10a` steered the system toward a single conclusion: **facts are canonical, and the
object model, dispatch, and code are readings/caches over the one cell store**
("send == get"; eventually "native machine code is a cache of bytecode-cells"). We
now reject that primacy claim at its root.

> **The canonical substrate is the live, malleable object environment** — objects,
> vtables, methods (as native code), all reassignable *live*. That is what is true,
> and that is what the image preserves.
>
> **The relational model** — EAV facts, cells, datalog, Date-style 1NF — **is a
> derived reading over that environment.** It is *one of several directions the
> language could take*, held to a high bar (strong adherence to Date's relational
> model) precisely because it is an experiment worth doing well — **not** because it
> is the foundation.

"Code is a cache of facts" is retired. What replaces it: **a malleable live object
model, with the fact store serving two first-class but derived roles** — (1) a
**meta-object protocol**: a live, always-consistent *mirror* of the object model,
and (2) a **relational data store**: a place to assert data and run datalog under
the Date experiment. Neither role is the system's ground; both are lenses over the
canonical object environment.

This maps cleanly onto datalog's own layering: user-asserted base data and rules are
canonical *program input* (EDB + rule definitions); query answers are *derived*
(IDB). We are not inventing a hierarchy — we are honoring the one datalog already
has, with the object environment underneath it all.

### What is kept from 10a, with polarity reversed

10a's insight that *the object model is reflectable through the fact store* stays —
the `[vtable selector method]` **mirror is the meta-object protocol**, and it is
valuable. What flips is which side is truth:

| | 10a (superseded) | 42 (this doc) |
|---|---|---|
| Canonical | the fact store | the object model (dicts, native methods) |
| Dispatch | *is* `get` over facts | reads the native vtable dict (unchanged, fast) |
| The `[vt sel m]` fact | the truth | a **derived, live mirror** (the MOP) |
| Relational engine | the substrate | a derived lens; "one of many directions" |
| Native code | a cache of bytecode-cells | canonical; malleability served by re-codegen, not by fact-primacy |

The JIT's "native is a cache of *bytecode*" (10a §native-cache) is a **different**
claim about the malleability dial (regenerate native on redefine) and is **kept** —
it actively serves the live-environment goal.

## The MOP mirror and the consistency law

The audit's §D1 desync bug is *not* "we have a mirror." The mirror is the MOP and we
want it. The bug is that the mirror is **partial and silently stale**: only the
opt-in `addMethod:on:with:` writes the `[vt sel m]` fact, while the native
stdlib/prelude install via plain `api.method` (no fact), so a later plain install
over a tracked `(vt,sel)` leaves the fact pointing at a dead method.

The fix is **completion, not removal**, governed by a single invariant:

> **The consistency law.** For every `(vt, sel)`, the mirror fact `[vt sel m]` either
> equals the live dict binding, or is **absent**. It is *never* a different, stale
> value.

The asymmetry is the whole design: **absent is honest; wrong is forbidden.** Today's
bug is a *wrong-value* failure. Absence — "the mirror doesn't reflect this binding" —
is truthful, and it is exactly what makes escape hatches safe: a hatch is allowed to
be *silent*, never to *lie*.

### The seam already exists — put the mirror on it

`api.method` (`methodC` in `id.zig`) is a **direct Zig call** to `vtableAddMethod`: it
pokes the dict and **bypasses `send`/`bind`**. The dispatched form
`[vt addMethod: sel with: m]` goes through `send`. So the send-vs-direct-call
distinction *already is* the mirrored-vs-raw distinction:

- **`[vt addMethod: sel with: m]` (dispatched) is the MOP install — the mirroring
  chokepoint.** Because `addMethod` is a real, overridable message, post-init it is
  the seam that performs *bind-in-dict + assert-in-mirror as one operation*. This is
  the "so tightly interwoven it can't desync" property: there is no bind-then-sync
  with a skippable second step; install *is* the paired operation. It is also
  Piumarta-faithful — "you may override `bind`" applied to install.
- **`api.method` (direct call) is the raw escape hatch, by construction.** It never
  reaches the seam, so it is dict-only. The boot seed uses it (the store does not yet
  exist); perf-critical bulk installs may use it deliberately.

No mass conversion of native call sites is required: seed-time installs stay on the
fast raw path and are reconciled by backfill (below); only installs that must be
reflected go through `addMethod`.

### Maintaining the law atomically (the concurrency answer)

Every mirrored install orders its two effects as **retract-old-fact → mutate-dict →
assert-new-fact**. There is therefore never an observable instant at which the mirror
holds a *wrong* value — at worst it is transiently *absent* while the dict flips.
This is what lets us **not concede the single-transactor / consistent-snapshot
invariant**: no observer ever sees the mirror contradict the dict. (Single-threaded
today, so moot in practice; the ordering costs nothing and keeps the door open.)

The raw escape hatch is *outside* the law by definition. To keep it from *lying*, the
**safe** raw variant retracts any present fact on overwrite (degrading to absent, not
wrong); the truly-raw `api.method` is "you are bypassing the MOP, on your own" — used
only where correctness of the mirror is not wanted (seed pre-backfill, deliberate
private installs).

### Bootstrap, backfill, and the image

- **Boot:** the kernel seed installs via raw `api.method` *before the store exists* —
  no floor→store upward call during boot, no chicken-and-egg.
- **Init flips the default on:** once the relational engine is initialized, the
  `addMethod` seam mirrors. From that point the law holds for every dispatched
  install.
- **Seed backfill (decided: YES).** At init we enumerate every seed vtable's dict and
  assert the corresponding `[vt sel m]` facts, so the mirror is **total** — reflection
  works over the kernel primitives too, and *no functionality is lost merely by
  loading an image*. This needs **one small floor primitive: enumerate a vtable's
  selectors** (also what any live reflection query wants).
- **The mirror is a rebuilt cache, not independently serialized.** Loading an image
  *replays installs* (bringUp re-seeds the kernel + backfills; user methods re-fileIn
  / relink from their canonical source/bytecode facts), and each replay flows through
  the mirroring seam — so the mirror **regenerates** on load. There is thus no
  stale-mirror-in-the-image failure mode. Canonical durable state stays the object
  environment (seed + persisted source/bytecode); the mirror is derived, like the
  native-code cache.

### Why performance is safe

The mirror is on the **install** path, never the **send** path. Dispatch reads the
native dict; the mirror facts are read only by *reflection* (datalog over `[vt sel m]`,
the `attrCount:`/`attrAt:`-over-a-vtable introspection). The hot loop never touches
it. Installs are rare by nature — boot (once), stdlib load (once), user `defineMethod`
(interactive), FFI wrap (rare), **JIT promote (once per method, amortized)**. The only
way to make installs hot is redefining methods in a tight loop — pathological,
deliberately paid for, and covered by the raw escape hatch. Cost is one store assert
per rare install; the send path is untouched.

## The relational store as the Date experiment (§D2)

Datalog **rules are canonical relational *data*** — user program input, not an
object-model mirror. They must live in the store (queryable, historied, serialized by
the image), not in a process-global Zig map (`g_rules`) that vanishes on reload:

- **Reify rule definitions as store facts/cells** (a name's relation = the union of
  its definitions, which fact multiplicity already models). `g_rules` demotes to a
  **transient cache rebuilt from those facts** at query time / on load.
- **The engine is derived; its scratch is per-query.** The fixpoint materialization
  state (`g_active`, `g_fixpoint_depth`, `g_fixpoint_done`) moves from process-global
  mutable into a **per-query context/arena** — which also retires a latent
  concurrency hazard of the same class as the peg `g_furthest` global.

Rule definitions canonical (EDB + rules); query answers derived (IDB). Standard, and
exactly the Date/datalog faithfulness the direction wants.

## Temporal fidelity of the relational store (§D3, §D4)

Mechanical, independent, and *more* relevant under strong Date adherence:

- **§D3 — honest history-off `asOf`.** A history-off `:one` attr overwrites in place
  and bumps `tx`, so `[cell asOf: oldBasis]` reports the attribute **absent** rather
  than its old value. This is the documented D6 tradeoff (no history ⇒ no
  time-travel), so the fix is *honesty*: distinguish "this attribute retains no
  history" from genuine absence, rather than silently returning nil.
- **§D4 — schema time-travels.** `enumerateTriples` reads cardinality/history CURRENT
  even inside an `asOf` query, reinterpreting old snapshots with today's policy. Thread
  the query basis through `cardinalityOf`/`historyOf` so an `asOf` snapshot reads the
  schema as it was.

## Decision summary (all of §D)

1. **Object model canonical; relational reading derived.** "Code is a cache of facts"
   retired. The fact store is a MOP mirror + a relational data lens, both derived.
2. **§D1** — complete the mirror under the consistency law (mirror = dict, or absent,
   never wrong). Mirroring lives on the dispatched `addMethod` seam; `api.method` is
   the raw escape hatch. Ordering retract→set→assert. Init flips mirroring on; **seed
   backfilled** for a total MOP. One new floor primitive: enumerate a vtable's
   selectors.
3. **§D2** — rules reified as canonical relational data; engine derived; fixpoint
   scratch per-query.
4. **§D3 / §D4** — honest history-off `asOf`; schema time-travel.

## Invariants preserved

- **Performance:** the send hot path is untouched; the mirror costs only at
  (rare) install time.
- **Concurrency:** no new shared-mutable write path; installs route through the
  single-transactor assert with an ordering that never exposes a wrong mirror value;
  the relational engine's global scratch becomes per-query. Net: the concurrency
  posture is *tightened*, not conceded.
- **Image durability:** the canonical object environment is preserved; the mirror and
  the rule-cache are rebuilt on load — no independently-serialized derived state to go
  stale.

## What this is NOT

- **Not a rewrite.** The object environment, the dispatch dict, and the store all
  already exist. This changes *which side we call truth* and *completes* the mirror.
- **Not a retreat from the relational model.** The Date experiment gets *stronger*
  (rules durable, temporal fidelity honest) — it is simply a lens over the object
  model, not its ground.
- **Not removal of the MOP.** The mirror stays; it is made total and law-bound.
