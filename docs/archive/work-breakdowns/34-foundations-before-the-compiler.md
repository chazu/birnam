# Foundations to settle before the compiler (rung-3 prep)

> **Status:** Open — a working agenda (2026-06-27). Builds on
> [The Destination](../../design/30-the-destination.md), [Refound on libid](../../design/32-refound-on-libid.md),
> and [Rung 2 — cells](33-rung-2-cells.md). Rungs 1 (libid) and 2 (cell/relational
> store) are shipped and green. This note collects the **cross-cutting,
> representation-level decisions** that are cheap to make now and brutal to
> retrofit once peg + the compiler stamp them into every emitted instruction.
> Each item below is a question with analysis and a *leaning*, **not yet a
> decision** — we resolve them one by one and update each item's status as we go.

## Why this note exists

The compiler bakes representation. The moment it emits code for `3`, a failed
send, a block, an error, or an object's slots, it commits the whole stack above
to those choices. libid is ~390 lines and rung 2 is one file; the same decisions
cost a multi-day audit once there is a compiler, a parser, and a body of compiled
code resting on them. So we settle them at this seam.

## The remaining canonical layers

`docs/30` §9 fixes the layer order as **id → fact store → gc → peg → compiler**.
Rungs 1–2 (id, fact store) are done. Remaining: **gc, peg, compiler/lang**, then
the richer **relational engine** (full algebra / constraints / closure — Date
Third-Manifesto power, store-side and orthogonal to the rest). gc sits *under*
peg/compiler in the canonical order because they allocate heavily; pragmatically
it can be deferred past peg **except** for the parts F3 locks below (the header /
trace / root protocol), which cannot be retrofitted cheaply.

---

## F1 — Tagged immediates / value representation

**What.** libid is "pre-SmallInt": every value is a heap pointer, and integers
are passed pointer-encoded (`@ptrFromInt` — rung 2's `count` / `attrAt` returns
do exactly this). Do we introduce **tagged immediates** (small int, and likely
`nil` / `true` / `false`, maybe char/float) so common values aren't heap objects?

**Why harder later.** The compiler commits to a value representation in every
literal and arithmetic op; retrofitting immediates means auditing every
`vtableOf`, every deref, every value-touching method, and all compiled code.

**Shape if yes.** A tagged word (e.g. low bit set ⇒ small int, value in the high
bits). `vtableOf` branches on the tag and returns a well-known `smallint_vt`
instead of reading `obj[-1]`; `allocate` never produces a tagged word. This is a
hot-path change to `bind`/`vtableOf`. The `-1` trick survives for heap objects;
immediates simply have no `[-1]`. Rung 2's `count`/`attrAt` become the first real
SmallInt consumers (validating the scheme, removing the pointer-encoding hack).

**Leaning.** Do it now, before the compiler.

**DECIDED (2026-06-27).** Follow Piumarta/maru exactly: **SmallInt is the only
immediate** — bit0 = 1 ⇒ a 63-bit signed fixnum; pointers have bit0 = 0 (our
16-byte alignment guarantees it). `nil` / `true` / `false` are **boot-time
canonical singleton objects** (heap, compared by pointer), *not* immediates, and
kept distinct from Zig `null`, which stays *internal absence* (no-NULL rule:
"no current value" ≠ the value `nil`). `vtableOf` gains one branch
(`word & 1 → smallint_vt`, else read `[-1]`); `allocate` never yields a tagged
word. Rung 2's `count`/`attrAt` become the first real SmallInt consumers,
deleting the `@ptrFromInt` hack. Specials can be promoted to immediates later if
profiling demands; not now.

**Status:** DECIDED + **SHIPPED** — `smallint_vt` + `nil`/`true`/`false`
singletons + `mkSmallInt`/`smallIntValue`/`isSmallInt` in `id.zig`, exposed on
the `IdApi` (appended); `vtableOf` tag-branches; rung-2 `count`/`history`/
`attrCount`/`attrAt` now return real SmallInts. `test-id` 3/3, `test-store` 11/11.

**Float (the second number, post-MVP — `docs/39`).** SmallInt owns the *only*
immediate tag, so the floor's float is a **boxed `f64`**: a heap object whose
`[-1]` vtable is `float_vt`, payload one `f64` (`mkFloat`/`floatValue`/`isFloat`,
appended to the IdApi). The prelude adds its protocol with int↔float promotion
(`1 + 2.5` → `3.5`); the grammar a `float` rule before `number`; the FFI `d`/`f`
signature chars. The conservative GC scans the payload like any value (an `f64`
bit pattern that looks like a pointer only over-retains — never unsafe).

## F2 — `doesNotUnderstand:` as a message

**What.** Today `bind` returns null and `send` *panics* ("method not found").
Make message-not-understood an overridable **message** (`doesNotUnderstand:` or
equivalent) instead of a hard panic.

**Why harder later.** It is the dispatch-level hook for proxies, futures, and the
"dispatch is get" / relational-dispatch door we deliberately left ajar (Option C
in `docs/33`). Adding it after the compiler emits sends means changing the
failure path under all of them. ~20 lines now.

**Shape.** On a failed `bind`, instead of panicking, `send` re-dispatches a DNU
selector with the original selector + args reified. A default DNU on `object_vt`
raises the error (per F7); subclasses/prototypes can override.

**Leaning.** Do it now, before peg.

**DECIDED (2026-06-27).** The **light** form: on a failed `bind`, `send`
re-dispatches a `doesNotUnderstand:` selector to the receiver, passing the
original selector followed by the original args (a small stack buffer, no
`Message` object yet — that waits for F1/F5 to make object plumbing cheap). A
default `doesNotUnderstand:` is installed on `object_vt` at boot (so every object
inherits an overridable handler); a prototype can override it to intercept
(proxies, futures, relational dispatch). Bottom-out: if `doesNotUnderstand:`
*itself* is unbound, panic (a genuine bug) — mirroring how `bind` bottoms out
`lookup`. The default handler traps for now and is **swappable** so F7 can make
it raise an error value / signal a condition.

**Status:** DECIDED — implement first.

## F3 — Object header / trace discipline / root protocol (the gc gate)

**What.** A libid object today carries only the `[-1]` vtable pointer — no size,
no layout. `allocate:` takes a size the object then forgets. The collector
therefore can't know an object's extent, which payload words are pointers, or
where a module's roots live (the store keeps Oop-bearing facts in a `c_allocator`
`ArrayList` invisible to a naive heap walk; `Store`/`Cell` payloads are *opaque
Zig structs*, not Oop arrays).

**Why harder later.** The *collector* can come later, but the **header + trace +
root protocol cannot** — they are properties every object kind must carry from
birth. Adding a size/layout word or an opaque-payload flag after many object
kinds exist is a sweep through all of them.

**Decisions to lock (collector deferred).**
- (a) how an object reports **size/extent** (header word vs a descriptor on the
  vtable);
- (b) a **layout/trace** distinction — "this object's payload is N Oop slots" vs
  "opaque bytes, do not trace" (rung-2 objects are the opaque case);
- (c) a **root protocol** — how a module (the store especially) exposes its live
  Oop roots to a future collector.

**Leaning.** Reserve the header + trace flag + root hook now; implement the
collector when memory bites (before heavy compiler use).

**DECIDED (2026-06-27).** Clarification: `Store`/`Cell` *are* full id objects —
the concern is only that their **payloads are opaque** (a Zig struct + an
`ArrayList` of Oop-bearing facts in `c_allocator` memory). The destination
dissolves this: the backing becomes **id-native HAMT/RRB reached through the id
api**, making the facts ordinary traceable id objects. Piumarta used **Boehm
conservative GC** — which is *why* his model needs no headers/trace maps
(conservative scanning finds roots) — at the cost of false retention and weak
support for moving/compacting collection (which the malleability + distributed
ambitions eventually want). **Build no collector now** (we leak, as today). Lock
only one cheap, retrofit-expensive thing: a **per-vtable instance-layout
descriptor** — "payload is K Oop-slots" (precise trace) vs "opaque — call my
`roots`" (Store/Cell). That single field keeps **both** conservative and
precise/moving GC open and composes with F1 (a fixnum is never a pointer).
Implementation of the descriptor itself is deferred until just before the
collector; recorded here as the locked shape.

**Status:** **SHIPPED (2026-06-29)** — the collector now exists, in `src/id/id.zig`.

A **self-contained conservative mark-sweep** (no Boehm dependency). Why
conservative and not the precise descriptor first: birnam compiles methods to
native code via libtcc that holds live Oops in C locals/registers with **no stack
maps**, so a precise/moving collector cannot find those roots. We therefore scan
conservatively, exactly as Piumarta did — but hand-written over our own heap:

- **Tracking.** Every allocation is recorded in a side table (`g_blocks`) keyed by
  object address, tagged `.collectable` (instances: `alloc`/`mkString`) or
  `.permanent` (vtables, symbols, the nil/true/false singletons). Only
  collectables are swept; permanents are never freed and never traced — their only
  outgoing Oops live in the env/interner, scanned as roots.
- **Roots.** The C stack + callee-saved registers (flushed via `_setjmp`, then the
  whole stack from the collector frame to a recorded bottom is scanned) + the
  export env + the interner + registered **root providers** (module Oops in non-id
  memory). Any word equal to a live object's address is a root (false positives
  only over-retain; SmallInts (bit0=1) are skipped; references always point at an
  object's payload start, so an exact-address table lookup suffices — no
  interior-pointer handling).
- **The (b) layout/trace distinction became a per-vtable TRACE HOOK** (the locked
  shape, realized): `Vtable.trace_hook` (set via `setTraceHook`). When the
  collector traces a live instance, it calls the hook so opaque objects can mark
  Oops the conservative payload scan can't see. The store installs one that marks
  every fact's e/a/v — fires per reachable store instance, so both the singleton
  and every `[Store new]` are covered. (The (c) root protocol = `RootProvider`,
  used for genuinely global caches like the logic-var interner.)
- **Triggering.** `[System gc]` (manual) and **automatic**: an allocation that
  crosses a 16 MiB threshold since the last collection triggers one — safe at an
  allocation point because every live object is reachable from the stack/roots
  there. Inactive until a stack bottom is set (off during module load); the script
  runner sets it. `[System autoGc: bool]` toggles; `[System liveObjects]` reports.
- **Finalizers (post-MVP — `docs/39`).** A `.collectable` block may carry a
  `finalize` hook (`Block.finalize`, set via `IdApi.setFinalizer`), run once just
  before reclamation so an object can release memory it owns *outside* the id heap
  (the hook must not allocate id objects — it runs mid-sweep). This closed a real
  gap: the sweep freed only the wrapper block, leaking whatever it pointed at. Now
  a reclaimed **String** frees its byte buffer, and a **TccUnit** marked
  `disposable` (its native code provably dead — codegen does this for an eval that
  installed no closures) is `tcc_delete`d. See `docs/39` §2.

Proof: `examples/gc.birnam` (manual, churns 540k objects, all live data survives)
and `examples/auto-gc.birnam` (2M allocations stay ~25k live / ~36 MB RSS). Tests
in `id.zig` (reclaim, no-op-until-anchored, root/trace-hook survival, permanents
never swept, auto-gc bounds the live set) + `relational_test` (store + fresh-store
facts survive).

**The precise descriptor — SHIPPED (2026-07-01, slice 2 of the images arc;
[docs/40](../../design/40-the-image.md) §3c).** The locked per-vtable layout shape is now
real: `setLayout(vt, kinds, n, rest)` declares which payload WORDS are Oops vs
raw (a per-word kind prefix + a kind for the variable tail — covers fixed
structs, all-Oop trie nodes/block captures, and header-then-slots HAMT nodes
alike; the kinds live in `idapi` as `slot_raw`/`slot_oop`/`slot_code`). Every
pointer-bearing type installed one (floor String/Float, Vector/nodes/Map/Range,
Error, Cell, LogicVar, TccUnit; emitted block vtables declare their all-Oop
captures via the `birnam_layoutAllOops` runtime shim). With `gcSetPrecise(1)`
(`[System preciseGc: true]`, or `BIRNAM_GC_PRECISE=1`), tracing honors the
layouts — a raw word aliasing a dead object's address no longer retains it
(proven in `collection_test`) — while types without a layout, and the C
stack/registers always (the tcc no-stack-maps constraint), stay conservative.
Conservative remains the default; precision is what the image save (docs/40
slice 3) swizzles through. Still open: a compacting/moving collector.

## F4 — Concurrency stance

**What.** A *stated invariant set*, even without a concurrent implementation, so
nothing above bakes in single-threaded assumptions. Candidates that silently
assume single-thread today: the global mutable intern table (`symbol_list`), the
export `env`, non-atomic vtable-dict growth, the store's `ArrayList`.

**Why harder later.** Concurrency is pervasive; a compiler and engine written
against implicit single-thread assumptions are very hard to parallelize after.

**Shape to decide.** e.g. "dispatch is lock-free over immutable vtable dicts; a
`Store` has a single writer; the fact log is append-mostly; interning is
synchronized." Enforcement deferred; the rules guide everything above.

**Leaning.** Write the invariants now; implement to them lazily.

**DECIDED (2026-06-27).** Adopt the Clojure/Datomic invariants the substrate is
already shaped for — no new threads or STM yet, but everything above is written
to respect them:
1. **Values are immutable; an identity changes by appending a new value at a new
   tx.** (The fact log already does this — the keystone for lock-free reads.)
2. **A snapshot is an immutable basis (a tx); reads run against a snapshot and
   never block.** (Free once the backing is HAMT/RRB.)
3. **Dispatch is lock-free** — a method change *publishes* a new binding (and a
   new mirror fact), never mutates in place.
4. **One writer per store** (a transactor, Datomic-style) or CAS on the
   current-basis pointer; many stores run concurrently. The old kernel's
   single-transactor `transact:` is the seed.
5. **Interning is synchronized** (the one shared mutable map).

**STM** (coordinated multi-identity transactions, Clojure refs) is a later
feature on top of 1+2+4 — not built now, but kept open. Bonus: immutable values
+ (future) content-addressed identity (F6) is also what **distribution** needs —
so F4, F6, and the distributed door are the same door.

**Status:** DECIDED (invariants) — threads/STM deferred.

## F5 — AST / code representation (peg's output) — **due before peg**

**What.** Does peg emit AST as **libid objects / store cells**, or as a throwaway
**Zig tree** the compiler consumes? `docs/32` wants "grammars as cells/objects"
and "methods as cells."

**Why harder later.** This is the one decision specifically due *now* because it
shapes peg's output type. If code-as-cells is the destination, peg should emit
into the object model / store from the start; retrofitting "the AST was actually
a Zig struct" into "AST is cells" after the compiler reads it is a rewrite.

**Tension.** Code-as-cells unifies (homoiconic, queryable, historied code) but
must respect F1/F3 and the D6 memory lesson (the old 4 GB balloon was
everything-as-historied-facts — so code cells must default to **no history**,
exactly as rung 2 already supports). A Zig tree is faster to stand up but is a
throwaway we'd later replace.

**Leaning.** ~~Emit AST as objects (cells)~~ — revised below.

**DECIDED (2026-06-27).** Don't turn ephemeral pipeline scaffolding into facts
(D6 generalized):
- **Grammars** — id objects/cells (long-lived, queryable, extensible).
- **CST/AST and every lowering pass** — **transient, NOT facts**: plain Zig
  values (or throwaway id objects never asserted). Built and discarded per
  compile; the store never sees them.
- **The durable unit is the *method*** — the installed binding is what lands in
  the object model, and via 2b's mirror is *optionally* projected as `[vt sel m]`
  (history off). "Code as cells" = the *binding* is reflectable, not every node.
  The body (bytecode/native) is a cache held in the method.
- The "IR on a VM" idea is the compiler/VM rung; bytecode lives in the method
  object. Whether bytecode itself is cells = the methods-as-objects question,
  deferred (rung 3, opt-in, bounded).

So peg emits a transient AST; the compiler lowers to bytecode in a method object;
nothing per-node is a fact.

**Status:** DECIDED.

## F6 — Identity policy seam (`identityOf:`)

**What.** `docs/30`'s thesis: extensional identity (`identityOf:`,
candidate-key-first, content-addressed later) over the pointer-identity floor.
Rung 2's store currently assumes pointer `==` for e/a/v dedup and `currentValue`.

**Why harder later.** Lower urgency than F1–F5 — but if pointer-equality
assumptions spread through the store and engine unexamined, an overridable
identity policy becomes hard to introduce.

**Shape to decide.** *Where the cut goes*: at what layer `identityOf:` would
intercept, so the store's equality isn't assumed pointer-deep everywhere. A
"know where the seam is" item, not a "build now."

**Leaning.** Flag the seam now; keep pointer identity as the floor default;
revisit when the relational engine grows.

**DECIDED (2026-06-27).** Flagged, plus one cheap concrete: **route the store's
entity/attr/value comparison through a single `eq(a, b)` chokepoint** (and
eventually `keyOf(x)`) instead of the scattered inline `==`. Still just `a == b`
for now, but `identityOf:` / candidate-key policy then has exactly one place to
slot in — at key-match and at entity minting ("the customer with email X"
resolving to an existing entity). We do not decide the policy; we guarantee it a
seam. Connects to F4 (content-addressed identity ⇒ distribution).

**Status:** DECIDED + seam **SHIPPED** — `store.zig` routes all entity/attr/value
matching through `eq(a, b)` (pointer identity today; absence checks stay literal).
Policy deferred. `test-store` 11/11.

## F7 — Error model — **decide with the VM, not before**

**What.** The old kernel used errors-as-propagating-values (Result-style) over
Smalltalk exceptions. How do errors propagate through evaluation?

**Why listed.** The VM's eval path bakes this in, so it is settled *when* the
compiler/VM is built — not earlier. Recorded here only so it is not forgotten,
and so F2's default DNU behaviour lines up with it.

**Leaning.** Defer to the compiler/VM increment; align DNU (F2) with whatever
error model is chosen there.

**DECIDED (2026-06-27).** Deferred to the VM, with a direction noted: aim higher
than try/catch or Go sentinels. **Common Lisp-style conditions & restarts** is
the aspirational target — malleable (handlers high, restarts low, *resumable*
without unwinding), in birnam's Lisp/Smalltalk lineage, composes with the live
fix-and-resume image, and subsumes try/catch. **Errors-as-propagating-values**
(the old kernel's `ERROR_VT`) is the pragmatic floor. Decide at the VM; until
then F2's default DNU stays swappable so it can become "raise an error" / "signal
a condition" without reworking the dispatch-failure path.

**Status:** DECIDED (direction) — model chosen at the VM.

---

## Recommended sequence

1. **Now, before peg** (cheap, representation/dispatch-level, painful later):
   **F2** (`doesNotUnderstand:` message) and **F1** (tagging + SmallInt
   immediates, converting rung-2 `count`/`attrAt` to real SmallInts). Plus the
   *written* decisions for **F3** (header / trace / root) and **F4** (concurrency
   invariants), implementation deferred. Settle **F5** (AST-as-cells) on paper.
2. **peg** — emitting AST per F5.
3. **compiler / VM** (lang) — settling **F7** (error model) there.
4. **gc** — implemented against the F3 protocol.
5. **richer relational engine** — last, on top. Revisit **F6** as it grows.

The throughline: F1–F5 are all "decide the representation before the compiler
stamps it into a thousand instructions." None is large to build now; each is a
multi-day audit later.

## Working order through this note

We resolve the items one by one and flip each **Status** to a decision with a
short rationale (and a commit ref where code lands). Order of attack:
**F2 → F1 → F3 → F4 → F5**, then F6/F7 at their natural later points.
