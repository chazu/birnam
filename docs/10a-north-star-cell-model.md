# North Star — The Cell Refounding

> **Status:** direction **accepted 2026-06-25**. This is the conceptual center the
> system is being steered toward. It is **not a rewrite** and not the rejected
> mutable-object inversion (ADR-image-1 addendum, "the trap"). It is realized
> *incrementally* by work items already on the table — D5, D5.1, Q1, Q2, Q3 of
> ADR-image-1 turn out to be five faces of this one idea. Read this to know which
> direction those refactors are pointing.
>
> Background: [Vision](10-vision.md), [The Fact Model](11-fact-model.md),
> [The Six Indexes](11a-the-six-indexes.md), [The Object Model](13-object-model.md),
> [Cells](2a-cells.md), [Dispatch](26-dispatch.md),
> [The Performance Model](18-performance-model.md), ROADMAP.md ADR-image-1 (D5 /
> D5.1 / Q1 / Q2 / Q3).

## The thesis

We have built up two models — a Piumarta-style **id object model** (everything is
an object; you send it messages; even dispatch is overridable) and a Datomic-style
**facts-all-the-way-down relational model** (everything is an `[e a v t]` fact;
immutable; historied; queryable). The instinct has been to treat these as two
things to *marry*. They are not two things. **They are one operation read from two
directions**, and the system has been circling that center from the top down
without naming it. This note names it.

## They are already the same operation

The object model's atom is a **message send**: given a receiver and a selector,
produce a result — `bind(receiver, selector)`.

The relational model's atom is a **fact**: an entity, an attribute, a value —
`[e a v]`.

These are the same shape. **Receiver = entity. Selector = attribute. Result =
value.** And in birnam this is not an analogy — it is literally how the kernel
works: dispatch resolves a method by reading the fact `[vtable selector method]`
(`send.zig:131`; `obj[-1]` vtable ≡ `[e :vtable vt]`). *Looking up a fact and
sending a message are one operation.* Call it `get(entity, attribute)`.

So "how do we marry the object model and the relational model?" carries a false
premise. There is nothing to marry. There is one store, read two ways.

## The atom: a cell

There is exactly one substrate:

> **A persistent, historied, name-keyed store of cells. One operation —
> `get` / `assert` a cell. A cell can hold data *or* code.**

A **cell** is a named location with a timeline. Its name is a pair
`(entity, attribute)`. That is the whole atom. birnam already ships it as
`CELL_VT` ([Cells](2a-cells.md), Phase 16 Tier E) — a thing with identity, a
current value, and free history. The reframing is to recognize the cell is not a
*feature on top of* the system; it is *what the system already is*. A fact
`[e a v]` is "the cell named `(e,a)` currently holds `v`." An object is "the cells
that share entity `e`." The fact store is "all the cells, indexed by name."

Everything else is a **reading convention** or a **cache** over the cells:

| Concept | Is just… |
|---|---|
| **Object** | the cells read **by entity** (`e`'s slots) |
| **Relation / datalog / reverse-refs** | the cells read **by attribute / value** |
| **Method** | a cell whose value is **executable** |
| **Dispatch** | `get(vtable, selector)`, then apply if it is code |
| **Inheritance / delegation** | a get-miss **rule**: miss ⇒ `get(get(e, :delegate), a)` |
| **Time-travel** | the cell never overwrites; `assert` **appends to its timeline** |
| **Malleability** | *every* cell is reassignable — including method-, schema-, and dispatch-defining cells |
| **Modules** | cells **tagged by provenance** (`m`) |
| **Schema** | cells describing attributes — read *after* they are asserted (see Q1) |

Malleability "all the way down" is not a special power here; it is the *default*,
because there is only one kind of thing (a cell) and all cells are reassignable by
asserting a new value. That is the Piumarta "you may override `bind` itself"
property, recovered without a special mechanism.

## The topology: not a layer cake — a core with satellites

The five-layer stack (L0 structures → L1 identity → L2 dispatch → L3 fact store →
L4 modules) is a useful *implementation* picture, but as a *conceptual* picture it
is the wrong topology — and it is precisely what manufactures the Q2 "violation."
It puts dispatch (L2) and facts (L3) on different layers and then asks why the
lower one reaches up. But they are **the same operation**. There is no "up."

The cell model's topology is a **center with derived satellites**:

```
              [ EAVT order ]   [ AVET order ]   [ VAET order ]
                       \           |            /
   [ object table ] —— ( the cell store: get / assert ) —— [ mcache ]
                       /           |            \
              [ MEAT order ]   [ AEVT order ]   [ history ]
```

The center is the one primitive. **The six indexes, the object table, and the
method cache are all caches** — alternate orderings or materializations of the
same cells — built when a query justifies them and dropped under memory pressure
([The Six Indexes §7](11a-the-six-indexes.md) is already groping toward this; the
cell model makes it the *organizing principle*, not an optimization). Nothing is
above or below anything: one operation in the middle, a ring of derived speed
around it.

## The five layers are the performance realization, not excess

The radically simple thing is the **semantics**, not the **runtime**. Naive
`get(e,a)` through a HAMT on every message send is ~4.6 ns and loses the method
cache; you genuinely need the materialized projections to be fast
([The Performance Model](18-performance-model.md)). So the layers are not
accidental complexity — **they are the performance realization of the one idea.**
The cell store is what the system *means*; the layers are what it *does* to be
fast.

The move, therefore, is **not to delete the layers** but to **demote them from
co-equal foundations to derived caches and swappable policy over the one
primitive.** Stated plainly: *facts are canonical; indexes, the object table, and
mcache are caches; which of them exist is policy.* That is the same demotion
[D5.1](../ROADMAP.md) performed for identity (eid demoted from anchor to handle;
the interned name promoted to the durable anchor).

## The open questions are facets of this one refounding

These are not five separate refactors. They are five faces of *refounding the
system on the cell*:

- **D5** (object identity precedes the fact store) = *the naming of cells is more
  fundamental than any index built over them.*
- **D5.1** (interned names are the durable anchor; the eid is a relocatable
  handle) = *a cell's durable name is symbolic; the integer is an encoding.*
- **Q3** (make index / history / tier injectable policy) = *which caches and
  orderings exist is policy over the core, not part of it.*
- **Q1** (raw-append primitive + a flip to policy-aware mode; schema-as-seed) =
  *even the schema is cells, asserted before the cell-rules turn on* — the
  bootstrap of the cell store itself.
- **Q2** (dispatch's cold-path fact reads) = *dispatch is the canonical instance
  of `get`* — see below.

If this framing is adopted, those items stop being a grab-bag and become a single
arc with one destination.

## What this does to Q2

In the layer-cake picture, Q2 is a dilemma: dispatch (L2) reading facts (L3) is a
"violation," resolved either by **(a)** projection-population (dispatch reads a
separate materialized representation the store fills on miss) or **(b)** a logical
boundary (license the read directly). In the cell picture, **Q2 is not a dilemma at
all** — dispatch reading a cell *is the one operation*, and mcache + the object
table are simply transparent memoizations of it.

**Resolved (ROADMAP.md D5.2, 2026-06-25) via Piumarta purity.** Piumarta's model is
one operation, metacircular, no privileged machinery — and *happily circular*
(`vtable.vtable == vtable`); clean down-only layering is not a Piumarta value. Since
**`send` *is* `get`**, the pure move is to let dispatch *be* that one operation, not
to give it a private path:

- **Dispatch is *semantically* `get`** (the (b) commitment). Cold resolution reads
  the store; the metacircular seam is a feature, not a violation. Lookup itself is a
  reassignable cell — `[vt :bind method]` — Piumarta's overridable `bind`.
- **mcache and the object table are *transparent memoizations* of that `get`** (the
  (a) speed), with **no independent semantic status** — droppable, invalidated on
  redefinition. This is (a)'s caching *mechanism* under (b)'s *semantics*.
- **The "resolution interface" *is* `get`** — the closed cold-path question set
  ([the coupling audit](20a-relayering-coupling-audit.md): selector→method,
  receiver→vtable, →name, →delegate, →bind) is just the shape of `get` dispatch
  uses.
- **Rejected:** pure-(a) — the projection elevated to a real layer with an explicit
  L3→L2 populate flow — as anti-Piumarta (privileged dispatch-only machinery that
  denies `send == get`).

Replaceability is recovered the cell way (Q3 policy injection over the *one* store),
not via a second representation. This is *less* work than pure-(a), not more: keep
dispatch reading the store, fronted by the caches that already exist.

## Code is cells, too — and "native code can't be data" is two things

If methods are cells whose value is executable, the natural objection is the old
residue: *native code can't be data.* That residue is really **two distinct
things**, and they behave very differently:

1. **The evaluator** — the VM loop, the bytecode walker, the HAMT/RRB operations,
   the GC, the dispatch mechanics. *Something must actually run.* This is
   **irreducibly** native: you cannot express the interpreter in the data it
   interprets without infinite regress. At the bottom the CPU is the one evaluator
   not expressed in the system's own terms. **Cannot be removed, only relocated.**
2. **The primitive methods** — `+` on a SmallInt, the kernel vtable methods, today
   shipped as native Zig function pointers. These are native **for speed, not for
   necessity.** Nothing requires `+` to be machine code; it is native because it
   is hot. **This part is contingent.**

So: removing "native code can't be data" dissolves the *contingent* part (the
primitives) and genuinely simplifies the **model** — but the *irreducible* part
(the evaluator) stays, so the floor never reaches zero.

### Native code as a cache of bytecode — the cell-consistent shape

The cell model already has a pattern for "a fast representation *derived* from
canonical data": **the object table is a cache of facts** — facts are truth, the
materialized struct is a derived, droppable, rebuilt-on-demand projection. Native
code should have the *same* shape:

> **Machine code = a cache of bytecode-data.** Bytecode is the canonical form (it
> is data; it lives in a cell; it round-trips). Native machine code is a derived
> satellite, regenerated/re-linked on load — exactly parallel to how *promotion*
> materializes an object from its facts.

birnam **already does this for user code**: a bytecode method *is* data (the
`:bytecode-stream` fact) and materializes into a `Closure` after first read —
canonical-data + derived-cache, the same pattern. The only code that is not data
today is (1) the primitives and (2) the evaluator.

### The cheap win now vs. the JIT someday

Making the primitives data has a cost: to keep them fast once they are bytecode you
need a **JIT**, which is a large, platform-specific machine — *more* complex than
the pointer-fixup pass it would replace. So full removal trades a small simple
thing for a big complex thing; the **model** simplifies but the **engineering**
floor moves from "fixup pass" to "JIT," and does not shrink.

There is a free move in between — take it now, defer the JIT:

> **Declare bytecode the canonical form for *all* methods, including primitives.
> Treat native machine code as an *optional materialization* of a code-cell — a
> fast-path cache, not a foundation.**

Then the model is unified immediately (every method is bytecode-canonical, native
is a cache), the image round-trips uniformly (canonical bytecode serializes; the
native cache is re-derived or re-linked on load, like the object table is rebuilt),
and "native code can't be data" turns from a *foundation* into an *optimization* —
the cell-correct stance — **without paying for the JIT yet.** The JIT becomes a
someday-optimization (regenerate the native form instead of re-linking it), not a
prerequisite. This mirrors exactly what D5/D5.1 did for identity: do not delete the
fast thing — *demote it from primary to derived.*

### The native-cache backend: native ALWAYS; the dial is codegen-residency — 2026-06-25

**The goal (locked direction):** make **mode 1 itself native** — fully malleable
*and* native machine code, the way idst/COLA was. The bytecode/native split
dissolves not by "interpret in mode 1, compile in mode 3" but by **native being
the execution form in every mode**; what the strip-dial removes is the *ability to
change the code*, not the native-ness.

**The enabling insight:** "malleable AND native" requires only that the native
form be **regenerated cheaply on every redefinition** (edit → re-codegen →
native, fast enough not to stall the loop). That is exactly why Piumarta built the
VPU — a *live* code generator, not a ship-time one. So the real axis is **codegen
latency**, and it is what separates a mode-1 backend from a ship backend.

**Backends, by codegen latency:**
- **`libtcc` (TinyCC as a library) — the mode-1 JIT.** Compiles a C string to
  *in-memory executable code in ~ms* and returns a function pointer
  (`tcc_compile_string` → `tcc_relocate` → `tcc_get_symbol`) — **no dylib, no
  dlopen.** Fast enough to re-JIT on every redefine → **malleable + native**, with
  **C as the IR** (the stated direction). Native quality is unoptimized but *far*
  above a bytecode interpreter. Tiny (~100 KB), built for exactly this.
- **`clang` / `zig cc` (+ `zig build-lib`) — the mode-3 ship backend.** Slow
  (100s ms) but fully optimized; emits a frozen dylib, relinked by name on load.
  *Same `birnam → C` codegen pass*, different compiler.
- **Piumarta VPU / template JIT — the no-C alternative.** Most idst-faithful, no C
  dependency, tunable native quality; cost = a hand-written per-arch code
  generator. The successor if libtcc's unoptimized output or C dependency ever
  bites.

**Direction (locked): birnam → C, with `libtcc` for mode 1 (live) and
`clang`/`zig cc` for mode 3 (ship).** One codegen pass, two compilers at the two
ends of the speed/quality dial.

**Mode-1 loop PROVEN end-to-end (2026-06-25/26).** The full chain runs and is
stress-stable: `bytecode → C (codegen.zig) → native (libtcc, jit.zig) → installed
as the method (jit_method.zig) → ordinary send dispatches native`, and **re-JIT
on redefine** demonstrates malleable-AND-native. The generated C uses the runtime
"as a library" via C-ABI entry points (`birnam_send0/1/2/N`, `birnam_is_falsey/
_error`). Native is a cache: closure.data parks a SmallInt index (GC-safe) into a
JIT'd-body registry; `:bytecode-stream` stays canonical. **Block lowering done**
(MAKE_BLOCK/INVOKE_BLOCK → `birnam_make_block`/`birnam_invoke_block`; block bodies
stay interpreted) so conditionals/loops/higher-order blocks JIT — only a block's
`^` non-local return into the frameless JIT'd method is unsupported. See
[[project_birnam_libtcc]]. **Remaining:** a live auto-JIT policy (when to JIT a hot
method); the mode-3 ship path (`zig cc`, extends `freeze.zig`); vendoring libtcc;
`^`-into-JIT'd-method non-local return.

**The strip-dial, recast (more idst-faithful):** native is always the execution
form; the dial controls whether the **codegen + AST/source are resident** (= can
you still change the code):
- **Mode 1** — native + `libtcc` + AST resident → redefine anything, re-JIT
  instantly. *Malleable and native.*
- **Mode 3** — native frozen by `clang`, codegen + AST stripped → fast, immutable.

**Cell-model fit:** canonical code stays the **code-cell** (AST/bytecode — data,
serializable, round-trips in the image); **C is a transient codegen target**;
**native is the cache**; the JIT keeps that cache warm under editing. Bytecode, if
kept, demotes to "the compact serialized form of a code-cell"; the *interpreter*
can eventually retire in favor of always-JIT (true idst) or stay as a fallback.

**Notes / residue.** A compiled method **relinks by name on load** — the *same*
mechanism as W1 closure rematerialization, W5's fact linker, and D5.1
name-anchoring; the ship path *extends `freeze.zig`* (which already reconstructs a
module and `zig build-lib`s a relink-on-load dylib — swap "embed source +
re-`fileInSource:`" for "emit compiled methods"). **arocc not needed** — birnam→C
is our own codegen pass; C→native is `libtcc` (live) or `zig cc` (ship). Zig's
`-ofmt=c` is Zig→C (wrong direction). **birnam→Zig** is lower-friction for the
*ship* path (runtime-ABI access, same `build-lib`); **birnam→C** is required for
the `libtcc` *live* path and is more portable — codegen is the same shape, so the
ship target is deferrable. The evaluator (VM loop, GC, HAMT) stays native by
necessity (residue below); generated code *calls into* it ("C using the birnam
runtime as a library," not standalone).

## The irreducible residue (Münchhausen)

You cannot reach zero non-cell ground; you can only minimize and relocate it. What
irreducibly remains:

1. **The evaluator** — one thing that actually runs (§ above).
2. **The raw-append primitive + the flip** — the operation that asserts a cell
   *without* consulting cell-policy, plus the moment policy turns on (Q1).
3. **A handful of eid constants** — the self-referential anchor (`NIL = 0`, the
   kernel vtables).

This residue is the true frozen kernel — far smaller and more data-shaped than
today's hand-written bootstrap. Everything above it is cells.

## What this is NOT

- **Not the rejected inversion.** This is *not* "make mutable slot-vector objects
  the substrate with a fact store bolted on" (ADR-image-1 addendum, "the trap").
  That kills structural sharing and the Datomic bargain. Here the persistent,
  immutable, structurally-shared store stays at the center; nothing becomes
  mutable; snapshots/history/time-travel stay free.
- **Not a rewrite.** The center already exists (the fact store *is* a persistent
  name-keyed map; `get`/`assert` already exist; cells already ship; user methods
  are already canonical-data-plus-cache). This is a change of *which thing we call
  primary*, realized through refactors already scoped.
- **Not a performance regression.** The caches stay. "Radically simple" is the
  semantics; the runtime keeps its satellites. Confusing the two would rebuild the
  trap.

## The direction, in one breath

> **One versioned, name-keyed cell store. One operation, `get` / `assert`. Keys
> are `(entity, attribute)`; values can be code. Read by entity it is an object
> system; read by attribute it is a relational, temporal database; and because
> every cell — including those defining methods, schema, and dispatch itself — is
> reassignable, it is maximally malleable. Objects and facts are not married; they
> are one store seen from two sides. The indexes, the object table, and the method
> cache are speed satellites around that center, built on demand and dropped under
> pressure. Code is cells; native machine code is a cache of bytecode-cells, the
> same way the object table is a cache of facts. A tiny native seed boots it.**

That is more radically simple than the five-layer stack, more faithful to *both*
Piumarta and Datomic than either alone — and it is the center the layers have been
circling. We are steering there deliberately, one demotion at a time.
