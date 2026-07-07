# 41 — MVP Conceptual-Unity Audit (2026-07-03)

A read-only sweep of `src/id/` looking for the same *class* of wart the
"uniform type handles" fix (`2a90702`) removed: shortcuts, stubs, and
special-cases that violate the system's intended conceptual unity. Catalogued
by five parallel investigators, then consolidated. **No code changed** in
producing this — it is a tracking list.

> **SCOREBOARD (2026-07-06).** ✅ **DONE:** §A, §B (incl. B8/B9), **§C (C1–C5, C4 stale)**,
> §D (D1–D4), **§E (E1/E2/E3)**, **§F F1 (pointer `85e4a85`, long `8cf5136`) + F2 + F3
> (unsigned `u` sigchar `aba41fd`) + the boxed `Word` type (`4c0da27`)**. §D's resolution
> **inverted the North Star** — object model canonical, relational reading derived — recorded
> in **[docs/42](42-object-canonical-relational-derived.md)** (READ IT for current direction).
> All 13 suites + 24 examples green. ⬜
> **OPEN:** §F **F4 only** (peek/poke/alloc/free — foundation laid, hangs off `word_vt`).
> The large-int question is CLOSED: box has been full i64 since §A (a fossil ±2^62 clamp,
> now lifted), `u` gives Integer-if-fits/else-`Word`; **no bignum, no arithmetic-to-Word**
> (bignum deferred as a named feature). · §G **the
> harder half** (G2/G3 image consistency, G4 left-recursion crash, G6 Binding, G8 layering,
> G9 operator charset, G10 FFI leaks — each real design/feature work). **§G cheap end
> swept 2026-07-05: G1/G5/G7 done.** Nothing is parked.

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

## B. Sequence-protocol non-uniformity — unified over `inject:into:`; equality + gaps closed (✅ SHIPPED, 2026-07-03)

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

### B3/B4/B5/B7 — SHIPPED 2026-07-03
- [x] **B3 · HIGH — structural equality via double dispatch.** New floor primitive
  `==` (pointer identity, on `object_vt`). `Object>>=` is identity over `==`, so
  Boolean/nil/Symbol (singletons) get value equality for free; Integer/Float/String
  keep their native value `=`. Collections override with **double dispatch**: `a = b`
  → `b equalsVector:/equalsRange:/equalsMap: a`; the back-selectors default to `false`
  on Object, so `#[1] = aMap` or `#[1] = 5` is false with no type tag and no dispatch
  special case. Element comparison recurses through ordinary `=` (nested + mixed types
  work); Maps compare by key→value content, layout-independent. Also **enforced the
  `1 = 1.0` = false decision**: `iEq`/`fEq` are now per-type (were cross-kind) — arith
  still coerces, equality does not. Folds in the B8 leftover: Booleans now answer `=`.
  Files: `id.zig` (`==` + total `vtableOfC`), `prelude.zig` (`iEq`/`fEq`), `stdlib.birnam`.
- [x] **B4 · MED** — Integer `floor`/`ceiling`/`rounded`/`truncated`/`asInteger`
  (identity on a whole number) + `sqrt` (via `asFloat`), in `stdlib.birnam`.
- [x] **B5 · MED** — Range `at:` (0-based, matching Vector) / `first` / `last` /
  `reversed`, native in `collection.zig`.
- [x] **B7 · MED** — nil overrides `printString` → `"nil"` (native, `collection.zig`).
- Hardening surfaced en route: `vtableOfC` force-unwrapped the vtable slot and
  **crashed** when `store.eq` compared a method-trampoline fact value (a leaked
  non-object with no vtable slot). Made it total (null slot → null), so the
  value-equality helpers see "not a string" instead of trapping. Latent — the extra
  method facts from B3's defines tipped it over; it would eventually have bitten the
  live system as methods accumulate.

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
  test in `codegen_test.zig`. (The follow-on "Booleans have no `=`" gap was closed by
  B3 — `Object>>=` gives the boolean singletons value equality.)
- [x] **B9 · LOW — FIXED 2026-07-05 (`82ce2c0`).** A successful `runProgram` now
  clears the compile-error flag at its end, so a program that internally probes a bad
  `[Codegen compile: …]` no longer leaves a false `lastError` for the CLI's post-eval
  check. aot.birnam exits 0 → all 24 examples green. (A failed top-level compile still
  returns before the clear, note intact.) Original finding:
- [ ] ~~**B9 · LOW · `main.zig` runScript**~~ — a script that deliberately compiles bad
  source (e.g. `examples/aot.birnam` probing `[Codegen compile: "let :="]` → nil)
  leaves a stale `[Codegen lastError]`; `runScript`'s post-eval error check then
  reports a false "compile error" and exits 1 even though the script ran correctly.
  Clear `lastError` before the user eval, or scope the check to it.

## C. Errors-as-values not realised in PEG and Alien (they predate the error model)

> **ALL OF §C DONE 2026-07-05.** C1–C3 (`623aa10` alien, `9f47014` peg): PEG + Alien
> failures now return kinded F7 error VALUES (imported at call time, nil fallback if
> the error module isn't loaded), so failures are distinguishable and propagate. C4
> was **stale** (the error module already swaps the DNU terminus — see below). C5 done:
> `ensure:` + multi-kind `on:do:` in `error.zig`.

- [x] **C1 · HIGH — DONE (`623aa10`).** FFI failures return kinded errors —
  `#badSignature` / `#moduleNotLoaded` / `#symbolNotFound` (carries the name) /
  `#compileFailed` / `#relocateFailed` — on `wrap:`/`callback:`, instead of an
  indistinguishable silent nil. (The shim CALL-path nil-on-bad-shape is unchanged.)
- [x] **C2 · HIGH — DONE (`9f47014`).** `g_furthest` module global RETIRED → moved to
  per-grammar state (`Gram.furthest`); `[g parse:from:]` returns a `#syntaxError`
  value (with line:col) carrying the diagnostic, so nothing rides a clobberable global.
- [x] **C3 · MED — DONE (`9f47014`).** An undefined rule reference is recorded
  (`Gram.undefined_rule`) and surfaces as a distinct `#undefinedRule` error (carrying
  the name), no longer the same silent nil as a legitimate non-match.
- [x] **C4 · MED — STALE (resolved by design).** The finding read the floor
  `@panic` as the DNU terminus, but the error module (`libbirnam_id_error`, always
  in the boot stack) **already swaps** `object_vt`'s `doesNotUnderstand:` at load to
  mint a `#doesNotUnderstand` error (`error.zig` `objDnu`). Verified end-to-end:
  `[nil frobnicate]` in a running image yields `isError` → true, `kind` →
  `#doesNotUnderstand` — not a crash. The remaining floor `@panic` is the deliberate
  minimal fallback for the bare floor with no error module (docs/34 "left it
  swappable"). No code change; the finding was written before the swap landed.
- [x] **C5 · MED — DONE 2026-07-05 (`error.zig`).** `ensure:` added on `object_vt`
  (`[protected ensure: cleanup]` runs cleanup on value OR error, answers the protected
  result — unwind-protect with no handler stack, which the value model doesn't need).
  `on:do:` now also matches a **set** of kinds: `on: #[#a #b] do:` catches either —
  pure dispatch (`includes:` to the kind-spec), so error.zig needn't know the collection
  module exists; a bare non-wildcard symbol short-circuits to exact match (no junk DNU).
  Regression tests in `error_test.zig` (+ collection bring-up for the multi-kind one);
  demoed end-to-end in `examples/errors.birnam`.

## D. Object/relational unity is aspirational, and two things can desync

> **✅ ALL OF §D DONE 2026-07-05 → [42 — Object canonical; relational derived](42-object-canonical-relational-derived.md).**
> The "unity / send == get" framing is **retired**: the object model is canonical and
> the relational reading is a *derived* lens. D1's mirror is **kept and completed** (the
> meta-object protocol, under a consistency law — mirror = dict, or absent, never wrong),
> not removed. D2 reifies rules as durable relational data. D3/D4 done mechanically. See
> 42 for the decision, and the per-item commits below.

- [x] **D1 · HIGH — DONE 2026-07-05 (`d8d162e` + `29e23fb`).** The dispatch mirror
  is now the **MOP, completed under the consistency law** (mirror = live dict binding,
  or absent, never a different stale value). A floor method-overwrite hook retracts a
  now-stale object-valued fact before any rebind (retract→set ordering); `api.method`
  stays the raw escape hatch. Backfill (`[Store backfillMirror]`, over a new floor
  vtable registry + `vtableSelectorAt`/`vtableParent`) mirrors native selectors as
  GC-safe `#primitive` markers → a TOTAL MOP, rebuilt on every boot AND image load
  (markers excluded from save). See docs/42.
- [x] **D2 · HIGH — DONE 2026-07-05 (`1da013e`).** Rules are canonical relational data:
  `defineRule:` stores each definition as a `[pair __rule-name__ name]` fact (queryable,
  historied, **serialized**); `g_rules` demotes to a rebuilt cache (`[db rebuildRules]`
  on image load). Verified end-to-end: a save→load roundtrip preserves a recursive rule.
- [x] **D3 · MED — DONE 2026-07-05 (`bcedd2a`).** Honest history-off `asOf`: new
  `[store valueOf: e attr: a asOf: basis]` returns the value when reliably visible, else
  a `#historyNotRetained` error (not a silently-wrong nil) for a history-off attribute
  it can't answer; genuine absence still reads nil. (Datalog `asOf` keeps its best-effort
  watermark semantics — no per-fact error channel in a triple stream.)
- [x] **D4 · MED — DONE 2026-07-05 (`bcedd2a`).** Schema time-travels: `enumerateTriples`
  reads cardinality AS OF the query watermark (`cardinalityOfAsOf`), so an old snapshot
  is interpreted with the schema as it was then.

## E. Compiler features half-wired

- [x] **E1 · HIGH · CONFIRMED — FIXED 2026-07-04.** Chained-send capture. `emit`
  routes an `r_message` chain (≥2 messages) to `emitSendChain`, but `collectRefs`
  only handled the single-message shapes (`binaryMsg`/`keywordMsg`) — when
  `mr == r_message` it fell through and scanned NONE of the chain's args, so a free
  variable in a chain arg position (`[:x | [x + base + 1]]` → `base`) went
  uncaptured; the generated C then referenced an undeclared name → compile failure
  (nil block). Fix (`codegen.zig`): factored the per-message value-position scan into
  `collectRefsMsg` and applied it to every message in an `r_message` chain, mirroring
  `emitSendChain`. Purely additive (scans more positions; params + shadowing `let`s
  are still subtracted downstream, so no false captures). Regression test in
  `codegen_test.zig` — verified to fail without the fix. (Was introduced by `3264027`,
  chained same-category messages: emitter updated, capture pass not.)
- [x] **E2 · MED — DONE 2026-07-05 (`codegen.zig`).** The `defineMethod:on:as:` →
  `…:source:` lowering (and the artifact-mode install) is now **gated on the receiver
  being the `Reflect` global** (`isReflectRecv`: `skip(recv)` is an `r_ident` named
  `Reflect`). A user object with its own `defineMethod:on:as:` compiles as an ordinary
  overridable 3-arg send again — no silent 4th `source:` arg, no method-install
  hijack. All 52 real call sites use the literal `[Reflect defineMethod: …]` shape, so
  the transform still fires everywhere it should (an aliased `Reflect` degrades to a
  plain send — correct, just not source-recorded). Regression in `codegen_test.zig`
  (a fake receiver with its own `defineMethod:on:as:` returns its own value; a Reflect
  control still installs + runs).
- [x] **E3 · MED — DONE 2026-07-05 (`prelude.zig`).** Added block-taking `and:`/`or:`
  on the boolean vtable — they send `value` only on the deciding branch (mirroring
  `ifTrue:`), so `[false and: [| [10/0]]]` never evaluates the divide. `&`/`|` stay
  eager (the correct Smalltalk split — for already-evaluated booleans). This gives the
  documented `[10/0]`-is-eager seam a short-circuit complement. Regression in
  `prelude_test.zig` (a side-effect block proves the skipped branch isn't run);
  demoed in `examples/stdlib.birnam`. (The audit's old "Clean" note wrongly listed
  `and:` as an existing send — it did not exist; docs/13 §99 specified it.)

**Clean (recorded so we don't re-check):** `+ - * /` and
`ifTrue:ifFalse:`/`whileTrue:` are **not** special-cased — real, overridable sends.
No arity caps in codegen.

## F. FFI is a reduced re-implementation vs the documented (attic libffi) surface

> **F1 + F2 + F3 DONE — only F4 remains in §F.** F2 = platform bug (`1f15245`). **F1 is now fully resolved.** POINTER
> half (`85e4a85`): `p` marshals through the boxed `Word` at all four sites, full-width
> round-trip. LONG half (`8cf5136`): the "truncation" premise was **stale** — an Integer has
> been a heap box holding a full **i64** since the §A refounding removed immediates (no tag),
> and the image already serializes it as a full u64. The only limit was a fossil ±2^62
> *arithmetic* clamp; lifting it to the real i64 range makes every C `long` a first-class
> arithmetic Integer. **The large-int "decision" was a false binary:** we did NOT
> promote-arithmetic-to-Word (Word is opaque, poisons composition) and did NOT build a bignum
> (a deferred *feature*, not an audit cleanup — see below). **Resolved policy:** the i64 box
> IS the signed number; `Word` carries only the two non-signed-number FFI values — a pointer
> (F4) and an unsigned ≥ 2^63 (F3). F4 (peek/poke/alloc/free) hangs off `word_vt`.
>
> **Deferred, named:** true `SmallInteger → LargeInteger` bignum promotion (the Smalltalk-
> lineage "integers are mathematical" destination) — its own design (arbitrary precision,
> normalization back to SmallInt to keep §B typed-`=`/hash/store-identity), a feature not a wart.

- [x] **F1 · HIGH — DONE (`85e4a85` pointer + `8cf5136` long) · `alien.zig`/`prelude.zig`.**
  `p` round-trips a full-width pointer through `Word`; `l` is a full-i64 arithmetic Integer
  (the box was always i64 — the ±2^62 clamp was a maru-tag fossil, now lifted; `#overflow`
  fires only at genuine i64 limits). No truncation, no bogus overflow. Corrected the stale
  "63-bit tag / immediate" comments in id.zig + prelude.zig in the same pass.
- [x] **F2 · MED — DONE 2026-07-05 (`alien.zig`).** `RTLD_DEFAULT` is now
  platform-conditional (`@import("builtin").os.tag`): the Darwin dyld sentinel
  `(void*)-2` on Apple targets, `null` (== glibc/musl `(void*)0`) elsewhere. The old
  unconditional `-2` made every `wrap:` return `#symbolNotFound` on Linux. Darwin path
  unchanged — `test-alien` + `examples/ffi.birnam` (div/inet_ntoa/callback) still green.
- [x] **F3 · MED — DONE (`aba41fd`) · `alien.zig`/`tcc.zig`.** New `u` scalar
  sigchar for `size_t`/`uint64_t`. Policy at one emit site (`putScalarBox`): a
  `>> 63` test → top bit clear (every real length/count) boxes a full-arithmetic
  Integer, top bit set (≥ 2^63) a lossless `Word`. The reverse (a `u` arg / callback
  return) reads either representation via the new `birnam_uintValue` shim. Works at
  every site the other scalars do (arg/return/callback/struct-field/array). No new
  number type, no bignum. Verified end-to-end: `strcspn` wrapped `u s s` returns a
  computable size_t (`n * 2`). (Pre-existing, unrelated: `putPreamble` hardcodes
  `extern long strlen`, so wrapping the literal `strlen` with a non-`long` return
  clashes — an artifact of that one name.)
- [ ] **F4 · MED · `alien.zig`** — no `peek`/`poke`, `alloc:`/`free`, pointer
  deref, or Cell out-params. A C API that fills a caller buffer can't be wrapped
  without writing C. Half of "wrap-a-C-lib without Zig." **Foundation laid
  (`4c0da27`/`85e4a85`):** pointers are now `Word`s, so peek/poke/alloc/free
  install as `word_vt` protocol.

## G. Marked deferrals (LOW — honest, catalogued for completeness)

> **Cheap-end sweep DONE 2026-07-05 (`48b110b`):** G7 (raw `==`→`eq()`), G5
> (fixpoint non-convergence now reported, not silent), G1 (comment honesty). The rest
> stand — G2/G3 (image consistency, latent), G4 (left-recursion crash), G6 (Binding
> object), G8 (layering), G9 (operator charset — a small feature, not a cleanup),
> G10 (FFI leaks) — each needs real design/feature work, not a one-liner.

- [x] **G1 · MED — DONE 2026-07-05 (doc honesty, `image.zig`).** The "counted, never
  dropped" claim overstated: deferral relies on slice-4 replay re-deriving the value
  from recorded source — true for source-backed method/block globals, but a block-valued
  fact with NO source backing is genuinely absent from the persisted image (reported by
  `reportDeferred`, not silent). Comment corrected to say so; behaviour unchanged.
- [ ] **G2 · `image.zig:440`** — method-redefining append warns instead of
  re-emitting code; image internally inconsistent until compacting save /
  `--rebuild`.
- [ ] **G3 · `image.zig:223`** — layout guard inspects only slot 0; a
  partially-laid-out object can serialise a live Oop as a raw word → dangling
  pointer on load. Latent (types call `setLayout` uniformly today).
- [ ] **G4 · `peg.zig:225`** — left-recursive grammar recurses unboundedly to
  stack overflow (memo written after match returns, so it doesn't guard
  left recursion). Uncatchable crash, not a kinded error.
- [x] **G5 · DONE 2026-07-05 (`relational.zig`).** The fixpoint cap (`max_iter =
  1000`) now tracks convergence and, if the cap is ever HIT, prints
  `relational rule '<name>' did not reach a fixpoint … — result is partial` to stderr
  instead of returning a truncated relation silently. (A finite store still converges
  well before the cap; behaviour on convergence unchanged.)
- [ ] **G6 · `store.zig:620`** — `Globals includes:` conflates absent and
  explicit-nil (v1, pending a real Binding object).
- [x] **G7 · DONE 2026-07-05 (`store.zig`).** `historyLen`/`replaceInPlace` now compare
  (e, a) through the `eq()` identity chokepoint like every sibling, not raw `==`. `eq()`
  fast-paths pointer identity, so no cost — and it fixes a latent boxing bug: a boxed
  SmallInt entity fails raw `==` against an equal-valued box, which would have
  double-counted its timeline / skipped in-place overwrite. The "single seam" holds again.
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

- **A** — creation reintroduced a class/instance seam. **(DONE.)**
- **D** — object dispatch and the relational store. **(DONE — but the resolution
  INVERTED the North Star: the object model is canonical and the relational reading is
  a derived MOP mirror + data lens, not "send == get." See docs/42.)**

Everything in **B**, **C.1–C.3**, **E**, **F** is the "sibling pass not
updated" pattern and is mechanical to close once decided. **G** is honest,
marked deferral.
