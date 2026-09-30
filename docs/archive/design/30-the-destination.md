# The Destination (extensional identity over an intensional floor)

> The long-horizon target for birnam's first few layers, set down so the work to
> reach it can be described against something concrete. This document **refines**
> the north star (`docs/10a`) on one load-bearing point — *primacy* — and absorbs
> the eid-superimposition and module-equivalence corrections from the 2026-06-26
> design conversation. Background: [Vision](../legacy-kernel/10-vision.md),
> [North-Star Cell Model](../legacy-kernel/10a-north-star-cell-model.md),
> [Object Model](../legacy-kernel/13-object-model.md), [Dispatch](../legacy-kernel/26-dispatch.md),
> [Cells](../legacy-kernel/2a-cells.md), [Re-Layering Audit](../legacy-kernel/20a-relayering-coupling-audit.md).

> **Status (2026-07-10):** the *primacy* argument here is superseded by the
> object-canonical and compiled-unit decisions: compatible native units are the
> primary executable layer, while the relational store is a derived durable/meta
> mirror. See [42 — Object Canonical](../../design/42-object-canonical-relational-derived.md)
> and [45 — Compiled Units](../../design/45-compiled-units.md). The five invariant goals and
> extensional-identity thesis still stand; the layer-primacy discussion below is
> historical design context rather than the runtime authority contract.

## 0. What this document is

`docs/10` is the vision; `docs/10a` is the north star (one historied cell store,
read two ways: *send is get*). This document is the **destination** — the shape
of the bottom of the system once it fully honours the five invariant goals
below. It does not re-argue the vision. It states, precisely, *where the floor
is, what is frozen, what is superimposed, and how object identity becomes
extensional* — and it names the one place where reaching that destination
**reverses** a decision the north star made provisionally (§6).

## 1. The five invariant goals (the constitution)

These have been constant across every id/idst/COLA-inspired iteration. Nothing
below may violate them; everything below is in service of them.

1. **Fully malleable, image-based, grow-it-as-you-go.** The system is *for
   growing software*. Live mutation is the default cycle, not a debug mode.
2. **Thin layers, id object model at the bottom.** Each module loads the way
   idst loads — registering objects via sends.
3. **Source-load and binary-load are semantically identical.** A module is *data
   + message sends*. Whether it enters as source (compiled, then loaded) or as a
   prebuilt dylib that registers via the Id API is an implementation detail of
   *delivery*, never a semantic distinction. (Corrects the
   refile-vs-dylib dichotomy floated earlier — see §7.)
4. **C-ABI absorption.** A thin API wraps any C-ABI library into id-object
   adapters, interactively for exploration, then committable as a code unit and
   compilable to native. (Shipped: FFI / `wrap:as:signature:`.)
5. **The first layer exists only to enable (re)loading of other layers** — from
   the id object model up through PEG → syntax → compiler → ….

The research thesis layered on top of these: birnam is an experiment in
**extensional object identity** — identity determined by *what is true of a
thing*, as opposed to **intensional** identity (a pointer address or an allocated
id being the sole arbiter of what an object "is").

## 2. Vocabulary: what a *cell* is

A **cell** is a *named, historied place*. Concretely:

- an **identity** (its eid — see §3 for why this is not the floor),
- a **currentValue** — the latest asserted value of its value-attribute,
- a **history** — the append-only sequence of those assertions over basis-T.

A cell reads two ways at once, and this duality is the whole game:

- as an **object** — you send it messages: `[c value]`, `[c set: v]`, `[c asOf: t]`;
- as a **relvar / row** — it is just facts in EAVT: `[c :cell-value v @ tx]`.

So a cell is simultaneously birnam's atom/ref **and** Codd–Date's relvar: a
variable that changes by assignment, except old values become history rather than
vanishing. "Everything is a cell" = everything is a place-with-history in the one
store — which is *why* dispatch (find the method), deref (find the value), and
query (find matching facts) are one `get`. **Identity is the question "which cell
do you mean"; `identityOf:` (§5) is the answer.**

## 3. The eid is a superimposition, not the floor

Piumarta's `id` has **no eid**. An object *is* its pointer; allocation is
`malloc`; identity is the address; a vtable's parent and method dictionary are
*slots in the vtable struct*. That is the irreducible floor — the self-describing
object/vtable closure, plus `send`/`bind`.

birnam's eid + `object_table` (eid → pointer) are a **birnam addition**,
introduced for exactly one reason: the fact store cannot key on a raw pointer (a
pointer is not stable across image save/load, and is not a value). The eid is the
**value-handle for a pointer** — the bridge that lets an object be spoken about
*relationally, serializably, and temporally*.

Therefore:

```
L1.5  identity/fact superimposition   eid alloc · object_table-as-index · EAVT     (SUPERIMPOSED — may redefine L1)
L1    the id closure                  object/vtable · pointer identity · send/bind  (FROZEN FLOOR)
L0    persistent structures           HAMT · RRB                                    (FROZEN PRIMITIVE)
```

The eid layer is allowed to **redefine** parts of the Piumarta-faithful L1 (e.g.
override `allocate` to *also* mint and bind an eid) — that is the orthodox
Piumarta "make the mechanism a first-class, overridable send" move, applied to
allocation. The base model identifies by pointer; a *loaded layer* teaches it to
also identify by eid. The early kernel objects (`object`, `vtable`, the handful
of well-known vtables) can hold **pointer identity** during boot and be
**retrofitted** with eids when the identity layer mounts.

**Consequence for "is the fact store a module?"** Yes — in goal-3's sense, of
course it is. It is not a *leaf* module like the JIT (which is a pure cache the
kernel calls down into and which can load arbitrarily late). It is the **second
rung of the bootstrap ladder**: the first superimposition over the id closure.
The only thing that must exist before *any* module is the id closure itself,
because that is the language modules are written in.

## 4. Date's bargain: the eid is legal as a relvar name, illegal as a pointer

Codd–Date's relational model (and *The Third Manifesto*) **prohibits** pointers
and object-ids as identity: identity is always *by value* (candidate keys). This
appears to be in direct opposition to Piumarta's pointer-is-identity floor. It is
reconciled by one of Date's own constructs — the **relvar**: a *named place whose
value is a relation*, updated by assignment. Date is not an immutability purist;
he models mutation as assignment to a relvar holding immutable relation values.

A birnam **cell is a relvar.** So the eid-as-place is *legal under Date* — on one
condition, which becomes the discipline that keeps the whole system honest:

> **The eid may be a place-handle (a relvar name). It must never be the
> navigational identity (a pointer you dereference to find "the object").
> Navigation and equality go by value / candidate key.**

This is precisely where birnam currently *sins*: dispatch dereferences
`eid → object_table slot → pointer`. That is pointer-chasing by OID — the thing
Date forbids and the thing that makes today's identity intensional-in-the-bad-way.
The redemption is the pattern birnam already trusts everywhere (native = cache of
bytecode; mcache = cache of fact-dispatch):

> **The `object_table` slot becomes a *cache of an extensional identity
> resolution*, not the source of truth.** "Which object is this?" is answered by
> `get`; the pointer slot is its memoization.

## 5. Identity as overridable policy: `identityOf:`

The smallest change that turns extensional identity from an aspiration into a
runnable experiment is to make **identity-resolution a send** — `identityOf:` /
`internIdentity:` — whose **default** is today's mechanical counter, but which a
loaded layer may override.

Because the eid layer is a superimposition (§3), this is a *normal layer edit*,
not floor surgery: generalize allocation from `counter` to `policy`.

### 5.1 Candidate-key identity (the first target)

Two entities asserting the same key values **are** the same entity. Mechanically:

1. declare key attributes for a type (`[T identityKey: #[ #attr1 #attr2 ]]`);
2. on reference/allocate, consult the **AVET** unique-index on those attrs;
3. **found ⇒ that eid is the identity** (you are talking about the same cell);
   **not-found ⇒ mint a surrogate.**

AVET already exists — candidate-key identity is "look before you mint." The
default `identityOf:` becomes: *resolve by declared key if the type has one, else
mint a surrogate counter.* The surrogate fallback is correct: it is exactly the
keyless/anonymous case Date concedes needs a surrogate.

**The deep cost it forces — key mutability.** If identity *is* the key and the
key can change, identity is unstable (edit the object, it becomes a different
object). Resolution, and the rule birnam adopts:

> Keep the surrogate eid as the **immutable worldline / place**. Treat the
> candidate key as the **public, navigational identity *plus* a uniqueness
> constraint** over that worldline — *not* as the literal identity. The eid does
> not disappear; it stops being *public* (never navigated) and becomes the
> internal thread of time, with the key as how you *find* and *refer to* it.

This keeps Date happy (reference by value; no pointer navigation) and Piumarta
happy (a stable mutable place to grow).

### 5.2 Content-addressed identity (later, never foundational)

Two things with the same defining facts *are* the same (Unison-style; great for
code-cells and immutable values). This is **just another `identityOf:` policy**,
superimposed if and when wanted. Explicitly **not** part of the pre-bootstrap
foundation: no hashing / De Bruijn normalization is baked into the floor. Unison
is the proof the live/immutable tension resolves — content-addressed identity
*plus* a mutable name→hash layer for liveness — and in birnam that mutable layer
is the interner over cells.

### 5.3 The payoff

Making identity a *policy* means **both** identity styles become loadable and
**neither** is foundational. Candidate-key now (intellectual priority);
content-addressed later; the mechanical counter as the always-correct default for
the anonymous case.

## 6. Primacy: struct-primary floor, fact-primary above (refines `docs/10a` D5.2)

`docs/10a` (D5.2) resolved *send == get* by declaring **facts primary**:
struct/mcache are memoizations; a populated projection-as-layer was **rejected as
anti-Piumarta**. Reaching this destination requires the opposite claim **at the
floor**, and only at the floor:

- The **irreducible metaobject graph** — the well-known vtables, their delegation
  parents, and their method dictionaries — lives in **structs** (Piumarta), and
  is *sufficient on its own* to dispatch. This is what lets the bootstrap come up
  on pointer identity before any fact exists.
- The **fact store mounts over that graph and projects it.** Above the floor,
  user-level cells are **fact-primary** (Date).

So primacy is **per-layer**, and the cell's dual faces (place/struct vs
value/fact) are the seam:

| Layer | Primary face | Why |
|---|---|---|
| **Floor** (id closure: ~5 well-known vtables + their methods/delegation) | **struct / place** | must dispatch before facts exist; this is goal 5 |
| **Everything above** (user vtables, cells, schema, code, modules) | **fact / value** | history, reflection, query, time-travel — goals from `docs/10` |

This **refines** rather than contradicts D5.2: *send is get* still holds
universally; the change is only that `get`, at the very bottom, **bottoms out in
the struct projection of the metaobject graph** rather than in a stored fact. The
fact store *adopts* the pre-existing struct graph on mount (projects it into
EAVT/AVET), after which the two faces are kept coherent. "Projection-as-layer" is
re-admitted **only for the frozen floor's own metaobject graph** — not as a
general user-facing dispatch mechanism, so the anti-Piumarta objection (which was
about making *all* dispatch a separate populated cache) does not apply.

## 7. Module = data + sends, delivery-format-agnostic

Per goal 3, there is **no semantic distinction** between:

- **source-load** — file in source → PEG → compile → register objects via sends;
- **binary-load** — dlopen a dylib → it registers objects via sends through the
  Id API.

Same effect: facts asserted, objects bound. The only *real* axis that
distinguishes layers is **bootstrap depth** — can a layer load *late*, or is it
depended-upon *while other layers come up*? And per §3 that axis collapses to a
single line: **the id closure is the one must-be-first thing; eid, facts, and
dispatch-over-facts are all superimposable.** "Static vs dylib" is never the
question; "how early is it needed" is.

## 8. What the destination requires of the boot path (audit result)

Bootstrap writes **324 facts** before the module system exists (measured by the
W7.0 probe, `boot_probe.zig` + `bootstrap_test.zig`, 2026-06-26 — supersedes the
earlier ~250 estimate). They split by deferability:

1. **Reflection shadow — 215 (66%)** — `:name` (171) + `:vtable` (44). Pure
   duplication of structure that *already* exists: the vtable is in every object
   header's −1 slot; the string↔eid bijection is in the interner's maps. **100%
   deferrable** — publish lazily when the identity/fact layer mounts. Mechanical.
2. **Metaobject graph — ~50** — `:delegate` (42) + the kernel method-dict
   bindings (5 selectors + the Module/FFI method installs in the long tail). In
   pure Piumarta these are *struct slots* (`vtable->parent`, `vtable->methods`).
   **Relocatable into the vtable struct, projected on mount** — the §6 primacy
   cut, i.e. the Q2 dispatch-seam from `docs/20a`.
3. **Fact-layer feature/config — ~55** — built-in functions (`:function-name`/
   `:function-impl` ×7), binary readers (×2), `:cardinality`, `:promote-threshold`,
   FFI symbol/schema attrs, etc. These *are* fact-layer features; they move *with*
   the fact layer.

**The load-bearing measurement — the boot is essentially write-only.** Exactly
**ONE** fact is read back before the module-load boundary: `:promote-threshold`
(Stage 11 seeds it, then `promotion.init` reads it — config, not structure).
`:name`, `:vtable`, and `:delegate` are read-before-mount **zero** times: the
cold-path delegation/method reads the audit worried about happen at *module load
and runtime* (post-mount), not during boot. So **deferring the reflection shadow
(A1/A2) and relocating the metaobject graph (Track B) carry no boot-ordering
risk** — far stronger than the ~46 ordering-sensitive writes first feared. The
W7.0 probe test asserts `totalRbm() == 1` to guard this invariant.

So: **a pointer-identity-first bootstrap is ~80% mechanical (buckets 1 + 3) and
~20% the §6 primacy cut (bucket 2)** — and bucket 2 is decoupled from boot
ordering (it is about dispatch's steady-state primacy, not boot timing). Once
made, build-time = run-time becomes real, because the floor rebuilds itself from
source before the fact layer exists.

## 9. The siphon and the layer-order

The **siphon** is birnam's bare entry point — the loader, and *only* the loader.
It is the smallest possible in-binary Zig core:

- `main()`,
- the **module loader** (dlopen for binary modules; source-load once the compiler
  is up),
- the **IdApi** struct + `kernel_eids` mirror it hands each module — initially a
  near-empty skeleton that each layer fills in its own slice of as it mounts.

Crucially, the siphon **contains none of the layers** — not id, not the store, not
the structures. **id is the *first module it loads*, not part of it.** Everything
in the tower below — id, fact store, gc, peg, compiler — is a layer the siphon
mounts in order through one uniform interface (register-objects-via-sends, §7).
This is the operational form of §3/§6: it decomposes today's monolithic
`bootstrap.zig` (`→ siphon.zig`) into an ordered sequence of mounts. The W7.0
measurement (§8) is what licenses the decomposition: the boot is write-only with a
single read-before-mount, so the layers carry almost no cross-ordering
entanglement.

### The order (and why each edge holds)

```
siphon (bare entry point): main + module loader + IdApi skeleton + kernel_eids
   │   loads, in order (it contains none of these):
   ▼
1. id          object/vtable/send/bind — struct-primary floor (§6)         deps: allocator only
2. fact store  eid alloc + interner/strings + EAVT/AVET; projects the      deps: id, HAMT/RRB
               id struct graph; function-call assert/query (NO parser)
3. gc          tracing over object structs + object table + fact VALUES     deps: id, fact store
               (a closure is reachable through a fact value); lazy-mountable
4. peg         parser engine; minimal fact-assert grammar first            deps: id, fact store
               (hand-coded Zig-struct), then grammars-as-facts
5. compiler    birnam source → AST → bytecode cells → C → native (tcc)     deps: peg, fact store
```

The decisive edges:

- **id needs nothing below it** — struct-primary (§6) means its vtables carry
  parent + method-dict in slots, so it dispatches with no facts, no HAMT. This is
  why id is hand-coded Zig and mounts *first*.
- **fact store does not need peg** — it is a function-call API (`assert(e,a,v,m)`),
  not a parser. So the store mounts at step 2, long before any grammar.
- **gc mounts above the store** — a method's closure is kept alive through a *fact
  value*, so the tracer must scan the value column; gc therefore needs the store.
  It is lazy-mountable (run on arena until allocation pressure matters).
- **peg's engine is store-free, but birnam stores grammars as facts**, so full peg
  needs the store; only its *first* grammar is hand-coded structs.

### Two kinds of "layer": frozen primitives vs loadable modules

The tower has two distinct kinds of rung, and conflating them is a category error
(it is why "split HAMT/RRB into a module" sounds plausible but mostly isn't a
thing):

| | **Frozen primitives** | **Loadable layer-modules** |
|---|---|---|
| examples | HAMT, RRB, the id closure, raw-assert | fact-store policy, gc, peg, compiler, user modules |
| nature | compile-time generic code / hand-coded Zig | registered via sends; swappable |
| role | the floor — never runtime-pluggable | the superstructure the siphon mounts |
| travel | linked into whoever uses them | loaded as Zig / dylib / source |

**HAMT/RRB are frozen primitives, and this is correct.** They are already split as
*source* — pure, generic, stateless container libraries at L0 with clean downward
deps (the W6 audit confirms `fact_store` imports them and they import nothing up).
But they are **compile-time generic types** the fact store *monomorphizes and
inlines* (`Hamt(u64, Leaf)`), not an interface it calls. Making them a
runtime-loadable/swappable module would replace that with function-pointer dispatch
on **the hottest inner loop in the system** — every fact read/write walks a HAMT —
to buy something nobody does (hot-swapping a HAMT implementation). So they travel
*compiled-in* with the fact-store module (their main user) and are reachable by
other modules at runtime through the IdApi (`rrb_len`/`rrb_nth` already are). The
"loadable" face of persistent structures already exists, one level up: `RRB_VT`
vectors, `ASSOC_VT` maps, and the Phase 16/17 collections layer — birnam objects
built *over* the frozen primitive. Frozen primitive below, object layer above; only
"runtime-pluggable structural floor" is the thing that is neither possible nor
wanted.

The siphon loads the *second* kind and **contains neither** — it is thinner than
even the frozen primitives, which are linked into the modules that use them.

### The capability bootstrap

The siphon's own power grows with the tower. Until peg + compiler are mounted it
can load only **hand-coded / binary** modules (id, store, gc, peg, compiler as Zig
or dylibs registering via the IdApi). *After* the compiler mounts, it can load
**birnam-source** modules. The siphon — `main` + loader + IdApi skeleton — is the
one thing that is never itself loaded; everything else, id included, is a module it
brings in.

### The seed: `seed.birnam` is canonical

The schema + vtable graph + names that `bootstrap.zig` stages 4–17 assert today
(the 324 writes of §8) become a **`seed.birnam`** — *human-authored birnam source*,
a file of fact-assertions — parsed by **peg's first grammar**: a minimal,
hand-coded Zig-struct grammar of ~5–10 rules (literals + one assert form). This is
the self-hosting prize (principle 5): the system's own foundation, editable in its
own language rather than imperative Zig or opaque binary.

> **Decisions (2026-06-26):** (1) peg's first grammar is **hand-coded Zig-struct**,
> so peg needs nothing from the store to parse the seed; full grammars become facts
> once the store holds the seed. (2) **`seed.birnam` is the canonical authoring
> form.** It may be *compiled to* a binary `.bim` seed for fast cold boot
> (ADR-image-1's "boot = load the seed image"), but the text is the source of
> truth, not the binary.

Why this lands *after* the store, never before it: you cannot assert into a store
that is not yet mounted, and the store needs no parser to be populated (§ "fact
store does not need peg"). A fact-assertion grammar is meaningless before step 2.
So the minimal grammar is the *earliest meaningful* grammar — step 4's first act —
not a step-0 prerequisite. The interim until `seed.birnam` exists is the current
Zig-asserted stages (form (a)); the destination is the text seed (form (c)).

## 10. The destination, in one paragraph

The frozen floor is the Piumarta id closure over HAMT/RRB: pointer identity,
struct-resident vtables with struct-resident delegation and method dictionaries,
`send`/`bind` — sufficient to dispatch with no facts in existence. Over it, an
**identity/fact layer is superimposed** (a module in goal-3's full sense): it
mints eids as *value-handles*, projects the floor's struct graph into EAVT/AVET,
and makes every object a **cell** — a historied place that is at once an object
(you send it messages) and a relvar (it is rows in EAVT). **Identity itself is an
overridable policy** (`identityOf:`): the mechanical counter by default,
candidate-key for declared types (the eid demoted to an immutable worldline, the
key promoted to public identity + uniqueness constraint), content-addressing
available later as pure policy. Everything above the identity layer — schema,
code, the PEG/compiler tower, user modules — is fact-primary, loaded uniformly
whether delivered as source or as a binary, each a set of facts tagged by module.
The **siphon** mounts these as an ordered tower — id → fact store → gc → peg →
compiler — and the foundation itself (schema + vtable graph + names) is authored as
**`seed.birnam`**, parsed by peg's first hand-coded grammar. That is birnam: an
intensional floor thin enough to bootstrap itself, carrying an extensional system
rich enough to be the database, the image, and the language at once.

## 11. Deliberately deferred / open

- **Content-addressed identity** — wanted, but a later `identityOf:` policy;
  no De Bruijn / hashing in the foundation (§5.2).
- **The maximum policy form** — making the *whole* 5-index arrangement injectable
  above the frozen line (`docs/20a` Q3 max); speculative until a workload needs it.
- **Floor method-dict representation** — **LOCKED (B0, 2026-06-26):** `parent: Ref`
  + inline open-addressed `(sel→method)` array, no HAMT at L0 (dumb + fast; §6).
- **Retrofit timing** — eager (assign every floor object an eid at mount) vs lazy
  (on first relational reference); §3.
- **Layer-mount granularity** — whether gc/peg/compiler ship first as decomposed
  Zig layer-modules (siphon-loaded) or stay in-binary until a later pass; §9. The
  id/store split (W7) is the load-bearing one; the rest can follow.
