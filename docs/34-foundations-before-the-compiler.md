# Foundations to settle before the compiler (rung-3 prep)

> **Status:** Open — a working agenda (2026-06-27). Builds on
> [The Destination](30-the-destination.md), [Refound on libid](32-refound-on-libid.md),
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

**Leaning.** Do it now, before the compiler — at least SmallInt + the
nil/true/false specials. Pick the exact bit layout here.

**Status:** OPEN.

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

**Status:** OPEN.

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

**Status:** OPEN.

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

**Status:** OPEN.

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

**Leaning.** Emit AST as objects (cells), history off — but confirm the
representation against F1/F3 before building peg.

**Status:** OPEN.

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

**Status:** OPEN.

## F7 — Error model — **decide with the VM, not before**

**What.** The old kernel used errors-as-propagating-values (Result-style) over
Smalltalk exceptions. How do errors propagate through evaluation?

**Why listed.** The VM's eval path bakes this in, so it is settled *when* the
compiler/VM is built — not earlier. Recorded here only so it is not forgotten,
and so F2's default DNU behaviour lines up with it.

**Leaning.** Defer to the compiler/VM increment; align DNU (F2) with whatever
error model is chosen there.

**Status:** OPEN.

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
