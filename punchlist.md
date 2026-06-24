# Birnam Punchlist — closing out the correctness / completeness / docs arc

Opinionated, prioritized list of what stands between the current state and "the
system we already have is correct, complete, and documented" (the locked
direction as of ROADMAP 2026-06-17). This is **not** the full TASKS.md backlog —
it's the subset that actually gates "done", plus the explicit non-gating items
so they don't masquerade as blockers.

Ordering principle: fix the one real bug, then let real programs generate the
rest. Everything tagged *demand-driven* should be done when a program you're
writing actually reaches for it — that's also the cleanest signal you're
genuinely complete.

---

## 0. Forcing function — write real programs (do this early, in parallel with §1)

The fastest way to discover what "complete" means is to write non-trivial
end-to-end programs and let them surface the gaps, instead of guessing at a
backlog. Most of TASKS.md was discovered building *infrastructure*, not
*programs*.

- [ ] **Minesweeper** — finish the `wip` example (commit `bbe73ce`). Exercises
  state + dispatch + collections.
- [ ] **A networked program** — `Net` exists now; a tiny HTTP server or chat.
  Exercises the event-loop + I/O surface (and will immediately expose §1).
- [ ] **A fact-store-centric program** — *highest value*. e.g. a task tracker
  that queries history via `asOf:` / `history`. This exercises the thing that
  makes Birnam *Birnam*, which no amount of stdlib polish does. Write this one
  even if the other two slip.

> Treat the gaps these surface as the real next list — they outrank anything
> below that a program doesn't actually hit.

---

## 1. GATING CORRECTNESS BUG — unbounded store growth in long-running loops

The only thing in the current state that's an actual *bug* rather than polish,
and the code says so itself.

- [ ] **Generalize the quiescent-boundary prune/gc sweep to ALL event loops,
  not just the REPL.**
  - The REPL sweeps via `processReplLine` → `prune.maybePrune` at each line
    boundary (`src/shell.zig:403`, also the linenoise loop `shell.zig:477`).
  - The **LSP does not** — every parse asserts CST/AST facts into the live store
    and the editing loop never prunes. Self-documented at `src/lsp.zig:22–24`:
    *"over a long editing session this grows unbounded. A prune hook belongs on
    the didChange path … this loop does [not]."*
  - The **`Net` server `accept` loop** has no sweep point either
    (`src/modules/net/net.zig`).
  - Root cause is ADR-perf-1 finding #1 ("writes grow memory without bound —
    never-freed history + five-index path-copy", `docs/18-performance-model.md`)
    showing up on every long-lived surface.
  - **Fix:** lift the REPL's quiescent-boundary maintenance
    (`prune.maybePrune` + `gc.consider`) into a shared hook any event loop can
    call at its safe point — wire it into `didChange` (LSP) and the accept loop
    (Net). A language whose LSP/servers OOM in a long session is not "correct."

---

## 2. COMPLETENESS FLOOR — uniform value protocols

`=`, `hash`, `printString` / `displayString`, `<=>` must be coherent across
*every* value type, enforced as one protocol — not added VT-by-VT.

- [ ] **Audit + complete the four core protocol selectors across all VTs.**
  - Symbols got the full set 2026-06-18 (`SYMBOL_VT`, `kernel_eids.zig:720`);
    numbers/strings have pieces; coverage is otherwise scattered.
  - Maps and datalog **depend** on equality+hashing being uniform, so gaps here
    are latent correctness bugs hiding in collections.
  - This is the Phase 17 "printString/equality" bucket — do it as a deliberate
    sweep ("does every VT answer the four selectors sanely?"), not ad-hoc.

---

## 3. UNLOCKS A CLUSTER — 15.A-AST / source-provenance facts

- [ ] **Source-location provenance facts on install (15.A-AST).**
  - Gates LSP `definition` / `references` (P3) and method-source hover (the
    "richer hover" deferred in TASKS.md LSP P2).
  - Worth doing because one feature unlocks several parked LSP items at once.
    A feature, not a correctness gap — sequence it after §1–§2.

---

## 4. DOCS — currency, not absence

The `docs/*.md` set is in good shape: 25 files, all real (66–174 lines, no
stubs). The risk is **drift**, not missing pages.

- [ ] **One docs-vs-code audit pass.** The June stdlib / LSP / Net / config
  blitz landed a lot that the docs predate — catch where the prose now lies.
  Don't write new docs first; reconcile existing ones with shipped reality.

---

## Non-gating — real, but do NOT let these masquerade as "done" blockers

These are genuine open items; none block calling the arc complete. Most are
*demand-driven*.

- [ ] **RRB `concat` / `splitAt` are naive O(N)** (`src/kernel/rrb.zig:421,433`).
  Verified **correct**, just slow (rebuild via `push`). Only bites if
  source-ropes / image-log splicing get hot. Completeness sliver — keep the
  `TODO Phase 8/10+` note (relaxed-radix concat/split, Bagwell & Rompf 2011).
- [ ] **Datalog: pull over reverse / `:many` attrs; subquery / saved-query name
  resolution from the surface.** *Demand-driven.*
- [ ] **FFI: callback arity > 2; struct-as-entity nested-field accessors; struct
  arrays in `{...}`; by-value struct ARG auto-marshal; x86_64 libffi headers.**
  *Demand-driven.*
- [ ] **`peekInt32:` (ffi_module.zig:911)** — no longer load-bearing: `Net`
  sidestepped it via `std.net`. Only needed if a future pure-birnam-over-libc
  path wants 4-byte C ints. Downgraded from BLOCKER.
- [ ] **Net: non-blocking/async I/O, TLS, UDP, `recv:` read-until.**
  *Demand-driven.*
- [ ] **LSP P0/P1/P2 slivers** — UTF-16 positions (currently bytes), static
  unbound-var + arity checks, method-install symbols, expose `:peg-expected`.
  (The store-growth one is promoted to §1.)
- [ ] **Stdlib gaps** — `slugify` (non-alnum-run collapse), `Yaml stringify:` /
  nested block structure, `System spawn:args:` + stdout capture.
- [ ] **15.R / 15.P slivers** — retention/excision/snapshot tail items.

---

## Recommended order

1. **§1** — store-growth sweep generalized to all event loops (the one real bug).
2. **§0** — build the 3 real programs, especially the fact-store-centric one;
   let them generate the next list.
3. **§2** — uniform value-protocol sweep (`=`, `hash`, `printString`, `<=>`).
4. **§3** — 15.A-AST (unlocks the parked LSP cluster).
5. **§4** — docs currency audit.
6. Everything in the non-gating section: **demand-driven**, as real programs
   reach for it.

> Deliberately out of scope for this arc: multi-threading and Phase 18 native
> codegen — both parked. Note for whenever they return: the concurrency/memory
> model must be settled *before* Phase 18, since codegen bakes the memory model
> into emitted machine code. Don't freeze a single-threaded model into native
> code and then re-do it.
