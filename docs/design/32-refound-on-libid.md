# ADR — Reach the inversion by refounding on libid, not by inverting in place

> **Status:** Accepted (2026-06-27). Supersedes the *in-place* completion of the
> W7 inversion (`docs/31` §W7). The destination is unchanged (`docs/30`); only the
> route to it changes. Background: [The Destination](30-the-destination.md),
> [Work Breakdown](../archive/work-breakdowns/31-destination-work-breakdown.md), [Re-Layering Audit](../archive/legacy-kernel/20a-relayering-coupling-audit.md),
> `~/dev/c/birnam` (the C reference @ `d35caee`).
>
> **Implementation update (2026-07-23):** the refounding and rung construction
> are complete. The `libid` identity and frozen `IdApi` remain the boundary; its
> production implementation is now `rust/birnam-id`, with `src/id/id.zig`
> retained as a separately named conformance and developer-rollback oracle.

## Context

The goal (the "inversion") is a self-sufficient **id floor** that dispatches with
no facts in existence — so the eid + fact store become a *superimposition* on top,
not the foundation (`docs/30` §3, §6; "the eid is a superimposition, not the
floor").

We pursued this **in place**, by de-coupling the existing fact-store-first kernel:

- **W7 shipped, and it PROVED the thesis.** The floor's metaobject graph now
  carries **zero facts**: kernel methods (B4-methods) and delegation (B4-delegation)
  are struct-resident; `siphon_test` shows `[OBJECT_VT init]` dispatching with both
  its method and delegate facts absent. The fact store is, for the floor, already a
  superimposition. (Commits `6a10c03` · `f41c193` · `a9293bb`.)
- **But finishing it is "turning the implementation inside out."** The residue —
  the `-1` slot still holds an **eid** (a fact-layer concept) rather than a vtable
  *pointer*; `floor.resolve` is eid-keyed; the 7 non-materialized vtables; the query
  projection (B3); the `:vtable` drop (A2) — each is a careful cut into the hottest,
  most load-bearing path, and the end state is still a kernel whose *origin* was
  fact-first. We would be paying to unwind history rather than to build the floor.

Meanwhile we **laid a clean floor** alongside it (`src/id/`, commits `2610f58` ·
`beaf876` · `e5d8f9f`): a hand-written Piumarta object system (`libid`) — vtable
**pointers** at `object[-1]`, symbols as selectors, method-dict + parent in the
vtable, **no eids, no fact store** — loaded by a bare **siphon** through an
`IdApi` (`id_init`), which can re-import even the id layer and persist module
state across hot reloads. This *is* the floor the inversion was trying to expose,
built directly instead of excavated.

## Decision

**Continue the inversion by building UP from libid, not by completing the in-place
inversion of the fact-store kernel.**

- The in-place inversion is **frozen at "factless floor"** — kept as a working
  proof-of-concept and a validation of the destination thesis. We will *not* push
  the remaining cuts (eid→pointer at `-1`, materialize-the-7, B3, A2, C2) into the
  old kernel.
- The eid/fact layer is rebuilt as a **module loaded on top of libid** (the
  destination's "fact store is the second rung, not the foundation"), rather than
  de-coupled out of a kernel that was fact-first from birth.
- The existing kernel (`siphon.zig` et al. → the `birnam` exe) **stays functional**
  through the transition; the `src/id/` tree is the forward path.

## Why

- **By construction beats by excavation.** libid has no eid at `-1` because it
  never had one — the property we were fighting to *recover* is the default. No
  hot-path surgery, no history to unwind.
- **The thesis is already validated.** W7 proved a floor dispatches without facts;
  we don't need to also prove it *in the old kernel*. The proof transfers.
- **Reflective from rung one.** libid is itself a module the siphon loads and can
  re-import; module loading is an id-api call. That reflectivity is hard to retrofit
  onto the old monolith and free on the new floor.

## What carries over (the W7 work was not wasted)

The in-place inversion produced the **design** the refounding now builds to:

- **Struct-primary dispatch works** — parent + inline method-dict in the vtable,
  fact-first/struct-fallback resolution (B0/B1/B2). libid is the same shape, minus
  the eid indirection.
- **Per-layer primacy** (`docs/30` §6) — frozen struct floor below, fact-primary
  above — is the layering libid + a fact-store module realizes cleanly.
- **The boot-is-write-only measurement** (W7.0) and the seam discipline
  (`send.resolve` as the single cold-path) inform how the fact-store module will
  superimpose over libid.
- **The siphon / layer-order** (`docs/30` §9) is now literal: `src/id/main.zig`.

## Consequences / open

- **Migration is unscoped here.** How the existing capabilities (FFI, datalog,
  image, JIT, the whole fact store) come back as modules on libid — and whether the
  old kernel is ported, wrapped, or retired — is the subject of follow-on work, not
  this ADR. The decision is only the *route*: build up from libid.
- **Two trees coexist for now.** `src/kernel/*` (fact-store kernel, the live
  `birnam`) and `src/id/*` (the libid floor). Expect them to diverge before they
  reconverge; the destination (`docs/30`) is the shared target.
- **`docs/31` §W7 remaining items are retired** as in-place tasks (they become
  "build it right on libid" instead). The factless-floor proof stands.

## Layering: relational is the FIRST superimposition, not a high module

A correction to an earlier draft of this ADR, which mistakenly put the fact store
*after* peg/compiler. The relational/cell layer is **rung 2** — the first thing
mounted over libid, below everything else — per `docs/30` §9
(`id → fact store → gc → peg → compiler`). Relational semantics are woven in **low**.

This is per-layer primacy (`docs/30` §6), not a demotion:

- **Rung 1 — libid (pure pointer).** Pre-relational *by necessity*: to dispatch you
  need the object model, so the object model's own floor cannot itself depend on the
  relational store (the circular dependency W7 resolved). The floor is the irreducible
  self-sufficient kernel — vtable pointers at `-1`, no cells.
- **Rung 2 — the cell / relational layer.** The first superimposition. It is the
  substrate for *everything above*: an object's **state** (slots/attributes) is
  **cells** (get/assert) — historied, queryable, time-travelable. Method **dispatch**
  stays libid-pointer (fast), but is **projected** into the cell store so it too is
  queryable/reflectable. "Send *is* get" holds as a *reading*; the hot path stays
  pointer-fast. So the whole *usable* system is relational from rung 2 up — only the
  bootstrap kernel (rung 1) is not.

**Open (decide before building rung 2):** how far the projection goes — state-as-cells
is clear; whether dispatch is merely *projected* into the store (read-only reflection)
or also *resolvable* through it (the full "dispatch is get") is the W7 §6 primacy call,
to be made when rung 2 is designed.

## Bootstrapping rung 2: the relational model models itself

Rung 2 has its own Münchhausen problem, and it bottoms out the *same way libid
does* — this is the **raw-append → seed → flip** pattern from `ROADMAP.md`
§ADR-image-1 Q1 (the "cardinality fixpoint").

**The regress.** To `assert [e a v]` with `:one` semantics the store must know the
cardinality of `a` — which is itself the fact `[a :cardinality :one]` — so the
cardinality *of `:cardinality`* regresses forever.

**The break.** Bottom it out with one hardcoded step, exactly like `Class class`:

1. **Raw mode.** The store has one irreducible primitive: `rawAssert(e, a, v)` —
   append a triple, **consult nothing** (no schema, no cardinality, no policy).
2. **Seed the schema as data**, while the rule-engine is off — raw-assert the whole
   schema as ordinary facts, *including the self-referential knot*:
   ```
   [cardinality  cardinality  one]      ← the fixpoint that unties itself
   [cardinality  name         "cardinality"]
   …attributes, tiers, index policy, all as facts…
   ```
3. **Flip the switch** → schema-aware.
4. From here every `assert` consults `currentValue(a, cardinality)` — a fact the
   store already holds. **The relational model now models itself.**

The slogan: *assert the rules while the rule-engine is off, then turn it on.* You
**cannot eliminate the ground** (Münchhausen) — you minimize and relocate it from
imperative Zig into self-describing data. The only hardcoded pieces are **(a) the
raw-append primitive** and **(b) the single flip** — not a tower of
schema-about-schema.

### It is libid's trick, one layer up

This is structurally identical to libid's own bootstrap — and now that *cells are
libid objects*, it is literally the same move at two rungs:

| | libid (rung 1) | relational store (rung 2) |
|---|---|---|
| metacircular op | `bind` resolves a method via `send(vt, lookup, …)` | `assert` consults cardinality via `currentValue(a, …)` |
| the regress | binding `lookup` needs `lookup`… | cardinality-of-`cardinality`… |
| the one hardcoded bottom | `vt == vtable_vt && msg == lookup` ⇒ call `vtable_lookup` **directly** | **raw mode**: append without consulting ⇒ seed schema ⇒ **flip** |
| after the bottom | the object model models itself | the relational model models itself |

So rung 2's bootstrap is not a new kind of thing — it is the rung-1 fixpoint again,
expressed in rung-1 objects: the `Store`, the schema cells, and `flip` are all libid
objects and sends. Concretely:

1. the cell module mints a `Store` (a libid object) in **raw** mode;
2. `[store rawAssert: …]` the schema cells (incl. `[cardinality cardinality one]`);
3. `[store flip]` ⇒ schema-aware;
4. `[store assert: e attr: a value: v]` now reads its own schema cells to place the
   value — self-describing, queryable, historied; every fact a libid object.

Only `rawAssert` and `flip` are Zig-hardcoded; the schema is data. (`seed.birnam`
is a *separate, later* concern — authoring the higher initial fact-set as birnam
source once peg is up. The fixpoint here is the store bootstrapping its *own*
self-description first, by hand, the way libid hand-boots.)

## Next on the libid floor

1. **Rung 2 — the cell/relational layer** as a module over libid (object state as
   cells; the get/assert/query/history substrate; bootstrapped by raw-append →
   seed → flip above). This is the inversion's destination, reached by
   construction — relational woven in *low*.
2. **peg** over the relational layer (grammars as cells/objects).
3. syntax + compiler (methods as cells), then up.

Each rung is a module the siphon loads; state persists across reloads; the floor
stays reflective.
