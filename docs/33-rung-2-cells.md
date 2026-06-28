# Rung 2 — the cell / relational layer (design)

> **Status:** Design accepted + **SHIPPED** (2026-06-27). Builds directly on
> [ADR — Refound on libid](32-refound-on-libid.md) and
> [The Destination](30-the-destination.md). Implements `docs/32` §"Next on the
> libid floor" item 1: the first superimposition over libid. Code lives in
> `src/id/store.zig` (+ `store_test.zig`); `zig build test-store` is green and
> the siphon (`src/id/main.zig`) mounts the store. The work breakdown below is
> all done — see the trailing notes per item.

## What rung 2 is

The first module mounted over the libid floor (`src/id/`). It introduces the
**cell / relational store**: an object's *state* becomes **cells** (get/assert
over a historied triple store), and — opt-in — the system's own *dispatch*
becomes queryable through that same store. Per `docs/32`'s layering, relational
semantics are woven in **low** (rung 2, below peg/compiler), as the substrate
for everything above.

libid (rung 1) stays pure pointer-dispatch and is **untouched** by this work:
`bind` / `send` / `allocate` / `lookup` / `addMethod` are not modified. Rung 2 is
a separate dylib the siphon loads through the id API (the `counter` module is the
template), holding only the `IdApi` and building everything via it.

## The decisions (and the reasoning that fixed them)

### D1 — Option B, sequenced as 2a then 2b (not a bolt-on)

We choose **Option B**: state-as-cells **plus** a *read-only* dispatch mirror.
But B is reached in two increments, because the reliable form of the mirror
depends on getting the foundation right first:

- **Rung 2a — state-as-cells + the self-modeling store.** The store object, the
  `rawAssert → seed → flip` bootstrap, `assert` / `currentValue`, history
  (opt-in), and object state as cells. `bind()` stays pristine. This is the
  uncontroversial core and proves the rung-2 thesis on its own.
- **Rung 2b — the dispatch mirror.** Method installs on *opted-in* types are
  projected into the store as facts, so behaviour becomes queryable/historied.
  Built only after 2a.

Option C (dispatch *resolved* through the store — literal "send is get" on the
hot path) is **rejected for rung 2**: it re-introduces the layering circularity
W7 removed and puts the relational store on the hottest path. It is a rung-3+
possibility, not the MVP.

### D2 — State has exactly one source; only dispatch is two-source

The clarifying insight that shrinks the whole problem:

- **libid has no named slots.** `allocate: n` returns `n` raw payload bytes;
  only Zig primitives read them. There is no message-addressed slot access for
  application objects at all. So when rung 2 introduces named, queryable
  attributes *as cells*, it **is** the slot mechanism — it competes with nothing.
  Application-object state therefore has a **single source** (the store) from
  birth. No sync, no advice, no reliability problem for state.
- **Dispatch is the only genuinely two-source case**, because the vtable dict
  (`keys`/`values`) is the authoritative fast path and the store mirror is
  derived from it. The reliability work in D4 applies *only* here.

### D3 — The dispatch mirror is one-write-two-representations, NOT methods-as-objects

2b does **not** reify methods into per-method objects. Methods stay bare
function pointers in the vtable dict. The mirror is achieved by making a method
*install* on a tracked type a single operation that writes **both** the dict
(authoritative, fast) **and** a derived fact — the same shape as the JIT story
("native is a cache of bytecode"). The fact is a *projection* of the dict, not a
second authority.

This is the direct answer to the memory/perf/concurrency constraints (see D6):
no per-method allocation, the hot path (`bind`/`send`) stays raw pointer scans
(lock-free reads), and nothing relational sits on dispatch.

Full methods-as-objects, and the more ambitious "dispatch is a message"
(`[vtableOf(rcv) invoke: sel on: rcv with: args]`, which has its own
bottom-out-`invoke:` regress and taxes the hot path), are deferred to **rung 3**
(compiler / JIT), where a method-object is the natural home for "native is a
cache of bytecode" — opt-in and bounded even there.

### D4 — Never patch the root metavtable; tracking is opt-in, default-out

The escape hatch ("some objects completely outside the store's reach") is the
**default**, achieved by leaving `vtable_vt` pristine:

- Patching `vtable_vt.addMethod` is maximally global — every vtable tracked, no
  opt-out. Rejected.
- `object_vt` is the wrong layer (instance-level generic protocol, reached as
  `vtable_vt`'s parent) and still global. Rejected.
- **Chosen (first cut):** route tracked installs *through the mirror* —
  `[store addMethod: sel on: vt with: m]` writes dict + fact in one op. Plain
  `[vt addMethod: sel with: m]` stays the untracked path, and *is* the escape
  hatch. Most stratified (the meta-op lives on the mirror, see D5), zero
  monkeypatching.
- **Available later (stronger):** a *delegated tracked metavtable*
  `tracked_vt = [vtable_vt delegated]` with `addMethod` overridden there; opt a
  type in by minting it with `tracked_vt` as its meta, so even raw installs on
  tracked types are caught. Pure prototype delegation — no classes (see D7).

Either way the root metavtable is never patched, untracked is the default, and
opting out is free.

### D5 — Mirror discipline (Bracha & Ungar): meta-protocol off base objects

Adopted as a lightweight **placement rule**, not a built hierarchy:
reflective / meta operations live on the **store / mirror**, never on base
objects. You ask `[store currentValue: e attr: a]` and `[store history: …]`; a
`Point` never grows `value` / `history` / `query` / `slotsDo:` methods. This is
Bracha–Ungar *stratification*, and it is exactly what keeps the meta level
distinct from the base object system. The relational store is itself naturally a
**mirror**: it reflects object state (and, in 2b, dispatch) as queryable facts
without hanging query methods off the objects. We hold this line from day one;
we do **not** build a full mirror class hierarchy now.

### D6 — History is opt-in, per attribute, default OFF (the 4 GB lesson)

The previous Zig birnam ballooned a tiny program past 4 GB because *everything*
was a historied fact. Hard constraint this time: **history is opt-in per
attribute, default off** (last-value-only — one overwritten slot). And it is
expressed *as a schema cell* in the self-modeling store: `[attr :history false]`
by default, flipped to `true` only where time-travel is wanted. The fix for the
old disaster is one schema attribute. No relational machinery sits on the
dispatch path, so no perf ceiling is imposed and lock-free concurrent dispatch
is preserved.

### D7 — Prototype-based throughout; no classes; minimal violence to id

Everything here is prototype delegation — libid already *is* prototype-based
(vtables are prototypes, delegation is the `parent` link, there is no class or
metaclass tower). The tracked metavtable of D4 is `[vtable_vt delegated]` — a
child prototype overriding a method and super-sending the parent — not a class.
Violence to `id.zig` for rung 2a + 2b is **near zero** (the floor's hot path is
untouched; 2b's stricter variant only adds a delegated metavtable).

### D8 — The store is a first-class mintable object; the siphon mints one seeded well-known instance

`[Store new]` mints independent stores (the vision's "named historied cell
store" wants many). The siphon mints **one canonical** instance and seeds it, so
we get both: multiple possible, one well-known.

## The bootstrap: the relational model models itself

Rung 2's Münchhausen problem and its break, per `docs/32` §"Bootstrapping rung
2" — structurally identical to libid's own `bind` fixpoint, one rung up:

1. **Raw mode.** The store's one irreducible primitive: `rawAssert(e, a, v)` —
   append a triple, consult nothing (no schema, no cardinality, no policy).
2. **Seed the schema as data** while the rule-engine is off — raw-assert the
   whole schema, including the self-referential knot:
   ```
   [cardinality  cardinality  one]     ← the fixpoint that unties itself
   [cardinality  history      false]
   [name         cardinality  one]
   [history      cardinality  one]
   …attributes / policies, all as ordinary facts…
   ```
3. **Flip** → schema-aware.
4. From here every `assert` consults `currentValue(a, cardinality)` — a fact the
   store already holds. The relational model now models itself.

Only `rawAssert` and `flip` are Zig-hardcoded; the schema is data. (`seed.birnam`
— authoring a higher initial fact-set as birnam source — is a later, separate
concern once peg is up.)

## Surface (rung 2a)

Store, a libid object (payload → a Zig `StoreState`):

- `rawAssert: e attr: a value: v` — append, consult nothing (bootstrap only).
- `flip` — leave raw mode; become schema-aware.
- `assert: e attr: a value: v` — schema-aware: consult
  `currentValue(a, cardinality)`; place per `:one` (latest supersedes) /
  `:many` (accumulate); append with `tx++`; record history only if
  `[a :history true]`.
- `currentValue: e attr: a` — newest-first read; `:one` → latest, `:many` → set.
- `history: e attr: a` — the value timeline (only meaningful where history is on).
- `cellFor: e attr: a` — a **Cell** handle object (`value` / `set:` / `history`):
  the "object state as cells" surface. A cell *is* an (entity, attr) address
  into the store (cf. the old Tier-E `2a-cells.md`).

`StoreState` (Zig, backing the payload): `facts: ArrayList(Fact)`,
`raw: bool`, `tx: u64`; `Fact = { e, a, v: Oop, tx: u64 }`. Fact backing is
Zig-side, consistent with libid keeping its method-dict backing in Zig arrays;
reflectivity lives in the API (any fact, including schema, is queryable), not in
making each triple a separately-dispatched object. Entities / attributes /
values are `Oop`s compared by pointer identity (attributes and keywords are
interned symbols — libid's existing model).

Persistence across hot reload uses the siphon's opaque state channel
(`ModuleInitFn`, the counter pattern): the `StoreState` pointer is threaded so
the seeded store survives a module code reload.

## Surface (rung 2b)

- `addMethod: sel on: vt with: m` — install on a tracked type: write the vtable
  dict (via `api.method`) **and** project the fact `[vt sel m]` (D3). One write,
  two representations. Untracked types keep plain `[vt addMethod: …]` (D4).
- `query …` over dispatch facts — list a type's selectors, a method's install
  history, etc. (the reflective payoff: "what does this understand?", "when did
  this method change?").

## Work breakdown — all shipped (2026-06-27)

1. **2a.1** ✅ (`210d745`) — Store object + `StoreState`; `rawAssert`; `flip`;
   the schema seed (with the cardinality knot); module + siphon wiring (mint +
   seed the well-known store). Test: the fixpoint — asserting a normal fact
   post-flip consults `currentValue(a, cardinality)`, resolving through the
   store's own seeded schema. *(The `:one`/`:many` read semantics nominally in
   2a.2 landed here, since they make the consult observable.)*
2. **2a.2** ✅ (`84c02f5`) — **opt-in history** per attribute, default OFF (D6):
   a `:one` attr with history off replaces in place (one slot, no growth);
   opted-in via `[a :history true]`, the timeline is retained. `history:attr:`
   read proves it (a default attr stays at 1 across updates; an opted-in one
   grows). `:many` stays set-semantics (its history diverges only with retract).
3. **2a.3** ✅ (`9e22754`) — `Cell` handle (`cellFor:attr:` → `value` / `set:` /
   `history`); object state as cells end-to-end. Mirror discipline (D5) proven
   directly: the entity does not understand `value` — the stateful protocol is
   on the cell, not the base object.
4. **2b.1** ✅ (`86ee20f`) — `addMethod:on:with:` tracked-install: writes the
   vtable dict **and** projects `[vt sel m]` (one write, two representations).
   Test: the fast path dispatches; the store mirrors it; an untracked direct
   install dispatches but is invisible (escape hatch). Methods stay fn pointers;
   the root metavtable is never patched.

All green: `zig build test-store` (6 tests, MaxRSS ~1M — no balloon), plus
`test-id` and the `birnam-id` siphon.

## Open / deferred

- **Stronger tracking (D4 variant 2)** — the delegated tracked metavtable that
  catches even raw installs — only if discipline (route-through-mirror) proves
  insufficient.
- **Richer relational engine** — full relational algebra / constraints / closure
  (the source of Date-Third-Manifesto power, and *option-independent*: it lives
  in the store, not in dispatch) is its own later arc. Rung 2 lays the
  triple/cell substrate; the query surface grows on top.
- **Methods-as-objects / dispatch-as-message** — rung 3 (compiler / JIT),
  opt-in, bounded.
- **`seed.birnam`** — the higher initial fact-set as birnam source, once peg is
  up.
