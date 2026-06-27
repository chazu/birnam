# ADR — Reach the inversion by refounding on libid, not by inverting in place

> **Status:** Accepted (2026-06-27). Supersedes the *in-place* completion of the
> W7 inversion (`docs/31` §W7). The destination is unchanged (`docs/30`); only the
> route to it changes. Background: [The Destination](30-the-destination.md),
> [Work Breakdown](31-destination-work-breakdown.md), [Re-Layering Audit](20a-relayering-coupling-audit.md),
> `~/dev/c/birnam` (the C reference @ `d35caee`).

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

## Next on the libid floor

1. **Rung 2 — the cell/relational layer** as a module over libid (object state as
   cells; the get/assert/query/history substrate). This is the inversion's
   destination, reached by construction — relational woven in *low*.
2. **peg** over the relational layer (grammars as cells/objects).
3. syntax + compiler (methods as cells), then up.

Each rung is a module the siphon loads; state persists across reloads; the floor
stays reflective.
