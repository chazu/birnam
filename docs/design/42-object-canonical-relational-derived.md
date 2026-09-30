# 42 — The Object Model is Canonical; the Relational Reading is Derived

> **Status:** decision **accepted 2026-07-05**. **Supersedes the primacy claim of
> [10a — The Cell Refounding](../archive/legacy-kernel/10a-north-star-cell-model.md)** (and the "code is a
> cache of facts" thesis it carries). It does **not** delete 10a's *mirror
> mechanism* — it reverses its *polarity*. Settles all of audit §D
> ([41 — MVP Unity Audit](../archive/work-breakdowns/41-mvp-unity-audit.md)).
>
> Background: [13 — The Object Model](../archive/legacy-kernel/13-object-model.md),
> [26 — Dispatch](../archive/legacy-kernel/26-dispatch.md), [37 — The Relational Reading](37-the-relational-reading.md),
> [33 — Rung 2 / the store](33-*), [40 — The Image](../archive/design/40-the-image.md).
>
> **Implementation update (2026-07-23):** the canonical-object/derived-mirror
> polarity is unchanged by the floor rewrite. The Rust floor is authoritative,
> the Zig floor is the retained oracle, and the same Store MOP consistency law
> and cross-floor tests apply to both.

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

> **The consistency law.** For every direct `(vt, sel)` binding, the mirror is
> exactly one of: the live method pointer for a tracked binding; `#primitive`, a
> presence-only marker for an unreified direct binding discovered by backfill; or
> **absent**. With no direct binding it must be absent. It never names a different
> method, and `#primitive` never survives after that direct binding is replaced or
> removed.

The asymmetry is the whole design: **absent is honest; wrong is forbidden.** The
`#primitive` marker is also honest but deliberately less informative: it says a
direct unreified binding exists, not which raw code pointer implements it. Today's
bug is a *wrong-value* failure. Absence — "the mirror doesn't reflect this binding"
— is truthful, and it is exactly what makes escape hatches safe: a hatch is allowed
to be *silent*, never to *lie*.

### The actual seam: Store operation over id mechanism

The physical primitive is `vtableAddMethod` in id. Both its internal message and
`api.method` reach the same dictionary mutation; neither asserts a Store fact. The
tracked semantic operation is instead **`[Store addMethod: sel on: vt with: m]`**:
it calls the id primitive, then asserts `[vt sel m]`. `Reflect defineMethod...`
reaches this Store operation through Codegen, so live definitions use the tracked
path today.

Any overwrite through `vtableAddMethod` after Store mounts fires Store's registered
mutation hook first. The hook retracts a fact the mutation would make stale. There
are not two raw variants: the same physical primitive is unhooked before Store,
hooked afterward, and fact-asserting only when a caller chooses the tracked Store
operation.

One current delivery exception is explicit: artifact-mode user-method lowering
still emits raw `birnam_method` installs. The hook keeps its mirror honest but may
leave it absent; artifact source is separately recorded under `__methods__`. The
CompiledUnit work in doc 47 closes this gap for user-method artifacts by routing
them through a tracked runtime shim. Artifact-private block-vtable plumbing remains
raw/private and does not claim a public MOP mirror.

### Who arbitrates the MOP (codification, 2026-07-12)

"The Store is the MOP" does **not** mean facts are authoritative over dispatch —
that polarity is the decision this document reverses. It means that, once Store is
mounted, every **supported, post-bootstrap meta-level mutation** enters through a
Store operation. Store owns the semantic operation, its history, and its mirror;
the id object model owns the physical vtable representation and remains canonical
for dispatch.

The supported live method-install path is the model:

```
Reflect defineMethod:on:as:
  → Codegen installMethod
  → Store addMethod:on:with:       semantic + historied MOP operation
  → id vtableAddMethod             physical dictionary mutation
  → Store asserts [vt sel method]  derived mirror
```

The raw id operation exists because boot must install methods before Store exists,
because artifact-private method dictionaries need no public mirror, and because a
thin floor must remain independently usable. It is mechanism, not the supported
post-bootstrap API for user-visible meta-mutation. A raw mutation may leave the
mirror absent; once Store is mounted, the registered hook first retracts any fact it
would make false. Thus the exact invariant is:

> **Supported post-bootstrap MOP mutation is Store-arbitrated. Raw floor mutation
> is a bootstrap/private escape hatch. In either case the object dictionary is
> canonical, and the Store mirror is an exact tracked Method, an honest
> `#primitive` presence marker, or absent—never stale or falsely present.**

This is the per-layer primacy decision in operational form: Store does not command
the vtable dictionary from facts, but it is the public chokepoint through which the
live environment becomes reflectable, historied, and queryable.

### Removal follows the same split (codification, 2026-07-12)

Method removal is not representable as a composition of lookup + add. Installing
the inherited pointer as a direct binding is observably wrong: the selector remains
direct, shadows later parent changes, and lies about protocol structure. Exact
removal therefore has the same two-layer shape as addition:

- **id mechanism — `vtableRemoveMethod`.** The vtable metaobject owns the dense
  key/value arrays, so only id removes a direct entry: find it, invoke the existing
  mutation hook, shift the arrays, decrement `tally`, and answer the removed method
  (or raw absence). It knows nothing about facts, history, CompiledUnits, or
  rollback. It is an internal message reached through the existing `send` API; it
  does not enlarge IdApi.
- **Store MOP — `removeMethod:on:`.** This is the supported post-bootstrap
  operation. It requires a direct binding, drives the raw removal, preserves the
  method history, and exposes absence/error in Birnam terms. `Reflect` may provide
  the stratified public spelling, but the operation still routes through Store.

The existing mutation hook generalises from "overwrite" to "change/remove": before
any existing direct binding changes, retract its mirror value **including a
`#primitive` marker**. A tracked replacement immediately asserts its new mirror; a
tracked removal leaves the direct mirror absent; a raw replacement/removal leaves it
honestly absent until backfill. The law remains:

```
tracked replace: retract mirror → replace dict entry → assert new mirror
tracked remove:  retract mirror → remove dict entry
raw change:      retract mirror → mutate dict       (absence is honest)
```

Removing a direct override naturally reveals the inherited parent binding. No
tombstone, disabled trampoline, or copied parent pointer is an acceptable
substitute.

### Maintaining the law atomically (the concurrency answer)

Every tracked install orders its effects as **retract-old-fact → mutate-dict →
assert-new-fact**; tracked removal is **retract-old-fact → mutate-dict**. There is
therefore never an observable instant at which the mirror holds a *wrong* value — at
worst it is transiently absent while the dict flips. This preserves the
single-transactor/consistent-snapshot posture. The runtime is single-threaded today,
but the ordering is still the contract.

Before Store exists the raw primitive has no hook and no mirror to protect. After
Store mounts, even a raw overwrite/removal fires the hook and degrades any affected
mirror to honest absence. Raw addition has no old fact to invalidate and remains
absent until a tracked install or backfill supplies one.

### Bootstrap, backfill, and the image

- **Boot:** the kernel seed installs via raw `api.method` *before the store exists* —
  no floor→store upward call during boot, no chicken-and-egg.
- **Init registers the hook and tracked MOP:** once Store is initialized, supported
  meta-mutation routes through its operations, and any raw overwrite/removal still
  retracts facts it would make false.
- **Backfill (decided: YES).** At init and image boot we enumerate every currently
  registered real type vtable's dict, including the seed vtables, and assert a
  corresponding `[vt sel #primitive]` presence fact where no exact tracked fact
  exists. Internal dictionaries are excluded. The seed mirror is therefore
  **total** — reflection works over the kernel primitives too, and *no functionality
  is lost merely by loading an image*. This needs **one small floor primitive:
  enumerate a vtable's selectors** (also what any live reflection query wants).
- **The mirror is a rebuilt cache, not independently serialized.** Loading an image
  re-seeds/backfills the kernel and reinstalls user behavior from compiled/source
  operations. Tracked installs regenerate exact mirrors; raw artifact installs may
  be honestly absent until backfill. There is no stale-mirror-in-the-image failure
  mode. Canonical durable state stays the object environment plus its compiled/source
  recovery material; the mirror is derived.

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
2. **§D1** — complete the mirror under the consistency law (exact tracked method,
   honest `#primitive` presence marker, or absence; never stale or falsely present).
   Supported post-bootstrap addition/removal is Store-arbitrated;
   raw id mutation is the bootstrap/private escape hatch. Ordering is
   retract→set→assert for replacement and retract→remove for deletion. Init flips
   mirroring on; **seed backfilled** for a total MOP. Floor mechanisms are selector
   enumeration plus exact direct-entry removal; policy and history stay in Store.
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
