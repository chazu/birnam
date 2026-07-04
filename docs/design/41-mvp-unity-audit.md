# 41 — MVP Conceptual-Unity Audit (2026-07-03)

A read-only sweep of `src/id/` looking for the same *class* of wart the
"uniform type handles" fix (`2a90702`) removed: shortcuts, stubs, and
special-cases that violate the system's intended conceptual unity. Catalogued
by five parallel investigators, then consolidated. **No code changed** in
producing this — it is a tracking list.

**The recurring pattern:** a feature gets wired into one pass / type / path and
its siblings don't get updated. Chains in the emitter but not the capture
analyser; `collect:` on Vector but not Map/String; `=` on scalars but not
collections; `abs` on Float but not Integer; the dispatch-fact mirror on one
install path but not the others; the errors-as-values model in `error.zig` but
not in PEG/Alien (which predate it).

**Confidence:** one finding (E1) was verified by hand and is a **confirmed**
correctness bug. The rest are investigator-reported at file:line and specific
enough to confirm on demand; treat as PLAUSIBLE until checked.

Severity: **HIGH** = breaks the conceptual model / user-visible inconsistency ·
**MED** = internal asymmetry, papered over · **LOW** = cosmetic / clearly-marked
deferral.

Status legend: `[ ]` open · `[~]` in progress · `[x]` resolved (add commit).

---

## A. The prototype refounding (✅ SHIPPED, 2026-07-03)

**Status: implemented and green** — all 13 test suites + the `--demo` full-stack
self-test + 22 examples pass; FFI works in all directions (struct-by-value,
struct-arg, callbacks). Boxing surfaced several latent identity-vs-value bugs that
were fixed as part of this (all were "compared integers by pointer, which was free
when they were immediates"): Map `keyEq` (collection.zig), the store's `eq`
chokepoint (store.zig), and the relational dedup/`groupBy` (relational.zig, new
`valEq`). Install paths were unified through an idempotent `ensurePrivateVtable`
(handles prototype, plain instance, or raw vtable) so `defineMethod`/`install:as:on:`/
FFI `wrap:` all resolve the family the same way. Known cost, as predicted: the
`auto-gc` example (2M allocating iterations) now runs ~100s (boxed ints allocate);
correct, just slow — reversible when immediates return as an optimization.



What started as three creation-seam warts (A1/A2/A3) resolved, through a design
argument recorded below, into a single coherent change. **The dispatch-split
first sketched here was itself classes in disguise** and was rejected; the
accepted design is a full Self-style prototype refounding that subsumes A1, A2,
and A3.

### idst provenance (checked against github.com/chazu/idst)

birnam's floor is named *libid* after idst's `libid.c`, and the two agree on the
canonical structure: `vtableOf` is idst's `_object___vtable` verbatim
(`NULL → nil_vtable`, `(long)self & 1 → tag_vtable`, else `[-1]`), one
self-describing meta shared by all families, delegation via `parent`/`delegate`.
**idst's floor has NO `new`/`clone` primitive** — its `__libid` C API is only
`proto`(=delegated), `alloc`(instance-of-family), `method`; creation lives in the
object language as ordinary dispatched methods, never a type-tag fork or a C
constructor pointer. And the 2007 idst base model **had no immediates at all** —
tagged fixnums are a later optimization. Both facts drive the accepted design.

### The design argument (why the obvious fixes are classes)

- A vtable-used-as-a-type-handle is a proto-class stratum. Making `[Vector new]`
  (family receiver) yield an *instance* is the one primitive classes add that
  prototypes don't have — "instantiate the type," a category jump from
  vtable→instance. Any `new`/`clone` behaviour that **differs by receiver-kind
  (family vs instance) IS the class/instance distinction.**
- The only class-free rule: **`clone` shallow-copies the receiver *as itself*** —
  copy an object → a sibling object; copy a vtable → another vtable. Uniform; the
  result's category just follows the receiver's.
- To make that rule *also* give ergonomic empty collections, a well-known name
  must denote a **prototype instance** (an empty vector), not the family. Then
  `[Vector clone]` copies the empty-vector prototype → an empty vector, by the
  *same* rule as `[anyObject clone]`. "A well-known name is the prototype."
- Removing immediates is what makes this total: with `SmallInt` boxed (like the
  already-boxed `Float`), `Integer` can be a real prototype too, so **every** name
  is a prototype, `vtableOf` loses its tag branch, and there is no residual
  value-type/heap-type split. Immediates return later as a transparent
  optimization behind the `Integer` prototype (the vision's "−1 trick at the
  materialization layer").

### The accepted model

- **No immediates.** Integers are boxed heap values (`mkInteger`/`integerValue`,
  copied from `Float`). `vtableOf` = null-check + `[-1]`, no tag.
- **Every well-known name → a canonical prototype instance** (`Object`, `Integer`,
  `Float`, `String`, `Vector`, `Map`, `nil`, `true`/`false`, …), each **owning**
  its family vtable (`vtable.owner = its prototype`).
- **One creation verb, `clone`** = shallow-copy the receiver as itself; **`new` is
  an alias** for `clone`. No "instantiate" verb.
- **`defineMethod on: <name>`** → `ensurePrivateVtable(on)`, unchanged: a prototype
  owns its family so it patches in place (extends all); a non-prototype COW-splits
  (extends one). The `[Object clone]`+`defineMethod` singleton pattern (Toml) still
  works — the clone isn't the owner, so it splits.
- **Floor keeps** `alloc`/`proto`/`method`/`ensurePrivateVtable`; **deletes**
  `newInstanceC`, `ctor`/`setConstructor`/`CtorFn`, `isSmallInt`/`mkSmallInt`/
  `smallIntValue`, the `vtableOf` tag branch, and **both** `== vtable_vt` forks.
- **One wrinkle:** shallow `clone` + a finalizer-owned buffer (`String`) would
  double-free — so `string_vt` **overrides `clone`** to dupe its bytes. An ordinary
  method override (idst-style), not a C hook; Vector/Map are fine shallow.

This resolves all three findings and is entangled with the immediate removal, so
it lands as one pass.

- [x] **A1 · `id.zig`** — deleted `newInstanceC`'s fork; `clone` (shallow-copy-self)
  on `object_vt`, `new` aliased to it. `string_vt` overrides `clone` to deep-copy
  its buffer (finalizer double-free otherwise).
- [x] **A2 · `id.zig`/`idapi.zig`** — deleted `ctor`/`setConstructor`/`CtorFn`. The
  prototype's payload IS the template; `Vector`/`Map`/`Range` prototypes minted once
  by `vecNew`/`mapNew`/`mkRange`. No `instance_size` needed.
- [x] **A3 · `codegen.zig`** — `targetVtable` is now `ensurePrivateVtable(on)`,
  unconditional; the artifact path uses a new `birnam_ensurePrivateVtable` shim.
- [x] **A4 · immediate removal** — `mkSmallInt`/`smallIntValue`/`isSmallInt`/
  `smallint_vt` KEPT AS NAMES but reimplemented boxed (modeled on `Float`), so the
  ~180 call sites were untouched. `vtableOf` lost its tag branch; `MKV` calls the
  `birnam_mkSmallInt` shim. Well-known names (`Object`/`Integer`/`Float`/`String`/
  `Boolean`/`Vector`/`Map`/`Range`) exported bound to prototypes that own their
  families (`setOwner`). New IdApi `setOwner`; removed `newInstance`/`setConstructor`.
- [x] **A5 (found during impl) · identity→value equality** — boxing broke every
  pointer-identity integer compare: Map `keyEq`, store `eq`, relational dedup/
  `groupBy`. All now value-aware for integers (+ strings). Also: `collection_test`
  resets the GC stack bottom per test (boxing's allocation pressure tripped auto-GC
  with a stale bottom); test C snippets that hand-tagged ints now use the shims.

## B. Sequence-protocol non-uniformity — the higher-order protocol unified over `inject:into:` (✅ CORE SHIPPED, 2026-07-03)

**Approach (b) shipped.** The higher-order protocol is now defined ONCE, in birnam,
on a shared `Collection` layer that all four families delegate to. Each family
provides exactly one native fold primitive — `inject:into:` — and inherits
`collect:`/`select:`/`reject:`/`detect:`/`detect:ifNone:`/`count:`/`includes:`/
`asVector`/`isEmpty`/`notEmpty`/`anySatisfy:`/`allSatisfy:` through ordinary
`vtable.parent` lookup. Adding a fifth collection type now costs one method
(`inject:into:`), not a dozen.

### Decisions settled (and why)
- **`1 = 1.0` is FALSE.** The cost of `true` is permanent: `=`⟹`hash` forces
  cross-type `=` coupling and a coordinated hash (integral floats must hash equal to
  their integer — a forever-maintained trap Smalltalk pays), AND it collapses
  `[e age 1]` / `[e age 1.0]` into one fact. `false` falls out of our principles:
  per-type `=`, per-type hash, the invariant trivially holds, and the relational
  substrate gets typed domains (Date 1NF: integer and real are different domains).
  Arithmetic still coerces (`1 + 2.5` = 3.5) — producing a value must pick a widening
  result type; interrogating value identity need not. `1 asFloat = 1.0` is explicit.
- **`collect:`/`select:`/`asVector` return a Vector** (the universal sequence),
  whatever the source family. Map folds over its VALUES.
- **birnam-authored, over `inject:into:`** (not `do:`): the language has **no mutable
  locals** (`x := …` is a syntax error; only `let` binds), so accumulation must be
  functional. `inject:into:` threads the accumulator; persistent `add:` builds the
  result. `do:` stays a native convenience but the protocol does not depend on it.

### Shipped
- [x] **B1 · HIGH** — Map answers the full higher-order protocol (folds over values).
- [x] **B2 · HIGH** — String answers it too, incl. `asVector` (lifts chars into the
  vector protocol).
- [x] **B6 (part)** — `detect:ifNone:` added; `detect:` returns nil when none match.
- [x] Uniformity cleanup — deleted the duplicated native `collect:`/`select:`/
  `detect:` on Vector/Range; one definition on `Collection` now serves all four.
- New floor primitive `IdApi.setParent(vt, parent)` (re-point a vtable's delegation)
  so `collection_vt` can be slotted between the families and `object_vt`. Files:
  `idapi.zig`, `id.zig`, `collection.zig` (native `mInject`/`strInject`, `Collection`
  export + re-parenting), `stdlib.birnam` (the protocol), `codegen_test.zig` (a
  stdlib integration test), `collection_test.zig`.

### Still open (mechanical, deferred — not blockers)
- [ ] **B3 · HIGH** — Vector/Map/Range still have no `=` (DNU). Expose element-wise
  structural `=` over the §A5 value logic + the new `Collection` iteration. (`1 = 1.0`
  now settled = false, so element compare is unambiguous.)
- [ ] **B4 · MED** — Integer missing `sqrt`/`floor`/`ceiling`/`rounded` + identity
  coercion `anInt asInteger`. (`abs`/`negated` already work via stdlib.)
- [ ] **B5 · MED** — Range half-indexable: no `at:`/`first`/`last`/`reversed`
  (`isEmpty` now works via `Collection`).
- [ ] **B7 · MED · `prelude.zig`** — nil doesn't override `printString`
  (`[nil printString]` → `"anObject"`).

### New bugs surfaced while doing B (out of scope, catalogued)
- [x] **B8 · HIGH (memory-unsafe) — FIXED 2026-07-03.** Literal `true`/`false` were
  not real Booleans: they were plain identifiers → unbound globals, and an unbound
  global reads `nil` (`birnam_global` returns the nil singleton for anything not in
  the Globals store), so `true`/`false` BOTH silently became `nil` — `true printString`
  → `"anObject"`, `true & false` → DNU `&`, `true not` **crashed**. Fix (`codegen.zig`):
  `nil`/`true`/`false` are now emitted as the runtime singletons `birnam_nil/true/
  false()` — literals, not identifiers, so they can't resolve to nil, be shadowed, or
  be captured as a block free variable (fixed both the `emit` leaf AND `collectRefs`
  capture analysis; added the missing `birnam_true`/`birnam_false` externs to the C
  preamble — the omission truncated the returned pointer, hence the crash). Regression
  test in `codegen_test.zig`. Note left open: Booleans still have no `=` method
  (`[1<0] = false` DNUs) — a separate small gap, folds into B3.
- [ ] **B9 · LOW · `main.zig` runScript** — a script that deliberately compiles bad
  source (e.g. `examples/aot.birnam` probing `[Codegen compile: "let :="]` → nil)
  leaves a stale `[Codegen lastError]`; `runScript`'s post-eval error check then
  reports a false "compile error" and exits 1 even though the script ran correctly.
  Clear `lastError` before the user eval, or scope the check to it.

## C. Errors-as-values not realised in PEG and Alien (they predate the error model)

- [ ] **C1 · HIGH · `alien.zig` (throughout)** — every FFI failure collapses to
  silent `nil`: bad signature, symbol-not-found, compile failure, relocate
  failure, indistinguishable. Docstrings enumerate the reasons; none surfaced as
  kinded values.
- [ ] **C2 · HIGH · `peg.zig:328`** — parse failure returns `nil`; the diagnostic
  (furthest-fail offset) rides a mutable module global `g_furthest`, "only
  meaningful right after a parse that returned nil." Two interleaved parses
  clobber it.
- [ ] **C3 · MED · `peg.zig:300`** — undefined rule reference indistinguishable
  from a legitimate non-match (both `nil`); a typo'd rule name silently never
  matches. (Root cause of the old triple-stale peg-csv bug.)
- [ ] **C4 · MED · `id.zig:262`** — default `doesNotUnderstand:` is a hard
  `@panic`; the DNU machinery is a proper overridable message but the default
  terminus crashes the process rather than yielding an error value. Marked
  deferral ("swappable once F7 gives errors a value form").
- [ ] **C5 · MED · `error.zig`** — `ensure:` absent (no unwind-protect) despite
  being in the documented model; `on:do:` matches only a single kind or wildcard
  (no multi-kind, no handler stack).

## D. Object/relational unity is aspirational, and two things can desync

- [ ] **D1 · HIGH · `store.zig:585`** — the "dispatch mirror" is opt-in and
  desyncable: only `addMethod:on:with:` writes the `[vt sel m]` fact; the entire
  native stdlib/prelude (plain `api.method`) produces no fact. A later plain
  install over a tracked `(vt,sel)` leaves the fact **stale** — `currentValue`
  returns a method no longer bound.
- [ ] **D2 · HIGH · `relational.zig:317`** — datalog rules live in a
  process-global Zig map, not as facts: not queryable, not historied, and
  `saveImage` doesn't serialise them, so every `defineRule:` vanishes on image
  reload. Breaks "the image IS the system."
- [ ] **D3 · MED · `store.zig:185`** — in-place slot patch makes `asOf:` silently
  wrong: the history-off default advances the fact's `tx`, so
  `[cell asOf: oldBasis]` *skips* the fact and reports the attribute **absent**
  rather than its old value. Confidently wrong, no error.
- [ ] **D4 · MED · `store.zig:405`** — schema doesn't time-travel: an `asOf:`
  query reinterprets old facts with today's cardinality/history policy.

## E. Compiler features half-wired

- [ ] **E1 · HIGH · `codegen.zig:833` — CONFIRMED correctness bug** — chained-send
  capture. `emitSendChain` (line 700) handles a message chain (`r_message`, ≥2
  children); `collectRefs` (line 835) only inspects `child(node,1)` as a single
  `binaryMsg`/`keywordMsg`. For a chain of length ≥2, `skip` doesn't collapse
  the `r_message` wrapper, so it matches neither branch and every argument in the
  chain is skipped by capture analysis while the emitter still references it.
  `[:x | [x + base + 1]]` fails to capture `base` → generated C won't compile →
  internal error. Introduced by `3264027` (chained same-category messages) —
  emitter updated, capture pass not.
- [ ] **E2 · MED · `codegen.zig:735`** — compiler intercepts the literal selector
  `defineMethod:on:as:`, receiver-blind; a user object with its own
  `defineMethod:on:as:` gets a 4th `source:` arg silently appended (or in
  artifact mode is compiled as a method install). Load-bearing for image/source
  recording, but this one selector is not a real overridable send.
- [ ] **E3 · MED · `stdlib.birnam:25`** — `&`/`|` are eager, no short-circuit;
  the documented `[10/0]`-is-eager seam at the boolean level. stdlib visibly only
  uses `&` where both sides are pure comparisons.

**Clean (recorded so we don't re-check):** `+ - * /` and
`ifTrue:ifFalse:`/`and:`/`whileTrue:` are **not** special-cased — real,
overridable sends. No arity caps in codegen.

## F. FFI is a reduced re-implementation vs the documented (attic libffi) surface

- [ ] **F1 · HIGH (latent) · `alien.zig:259`** — 63-bit SmallInt box truncates C
  `long`/pointers unchecked (same class as the known `peekInt` overflow); a
  64-bit mask/handle loses its top bit, no `#overflow`.
- [ ] **F2 · MED · `alien.zig:76`** — `RTLD_DEFAULT = -2` is macOS-only; glibc/musl
  use `(void*)0`. Every `wrap:` would silently return nil on Linux. Real
  platform bug (comment is factually wrong).
- [ ] **F3 · MED · `alien.zig` (grammar)** — no unsigned scalar types;
  `size_t`/`uint64_t` masquerade as signed, unmarshalled as signed → large
  values return negative.
- [ ] **F4 · MED · `alien.zig`** — no `peek`/`poke`, `alloc:`/`free`, pointer
  deref, or Cell out-params. A C API that fills a caller buffer can't be wrapped
  without writing C. Half of "wrap-a-C-lib without Zig."

## G. Marked deferrals (LOW — honest, catalogued for completeness)

- [ ] **G1 · `image.zig:217`** — block-valued facts dropped on save (comment says
  "never dropped"; they are dropped from the persisted image, just reported).
- [ ] **G2 · `image.zig:440`** — method-redefining append warns instead of
  re-emitting code; image internally inconsistent until compacting save /
  `--rebuild`.
- [ ] **G3 · `image.zig:223`** — layout guard inspects only slot 0; a
  partially-laid-out object can serialise a live Oop as a raw word → dangling
  pointer on load. Latent (types call `setLayout` uniformly today).
- [ ] **G4 · `peg.zig:225`** — left-recursive grammar recurses unboundedly to
  stack overflow (memo written after match returns, so it doesn't guard
  left recursion). Uncatchable crash, not a kinded error.
- [ ] **G5 · `relational.zig:403`** — fixpoint iteration cap (`while iter < 1000`)
  returns partial results silently on non-convergence.
- [ ] **G6 · `store.zig:620`** — `Globals includes:` conflates absent and
  explicit-nil (v1, pending a real Binding object).
- [ ] **G7 · `store.zig:175`** — `historyLen`/`replaceInPlace` compare with raw
  `==` instead of the `eq()` identity chokepoint; the "single seam" claim is
  already false (harmless today, will ignore any future identity policy).
- [ ] **G8 · `relational.zig:994`** — the relational module installs `printString`
  onto `symbol_vt` globally as a load side-effect (layering smell; all symbols
  render with `#` once datalog loads).
- [ ] **G9 · `lang.zig:120`** — fixed binary-operator charset (`= < > & | , + - *
  / ~`); operators outside the set (`%`, `@`, `!`) can't be binary selectors
  though dispatch would handle any interned selector.
- [ ] **G10 · `alien.zig`** — callbacks never released, `dlopen` handle leaked
  (v1, no-release); `@panic` on OOM throughout PEG/Alien/error.

---

## Two structural themes worth calling out

The two that are *structural* (not just a missing method):

- **A** — creation reintroduced a class/instance seam.
- **D** — object dispatch and the relational store are still parallel
  mechanisms, not one op (the North Star's "send == get" is aspirational).

Everything in **B**, **C.1–C.3**, **E**, **F** is the "sibling pass not
updated" pattern and is mechanical to close once decided. **G** is honest,
marked deferral.
