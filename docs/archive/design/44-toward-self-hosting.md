# 44 — Toward Self-Hosting (shrink native semantics, keep the from-zero bootstrap)

> **Status:** Analysis (2026-07-09, rev. 2 after adversarial review; rev. 3
> 2026-07-10 records Phases 1–3 shipped, the measured ~1.5–2µs send constant
> that now gates migration choices, and the console-REPL target discovered
> during the subcommand collapse; rev. 4, 2026-07-22, makes the cold-bootstrap
> invariant toolchain-neutral for Design 61; rev. 5, 2026-07-23, records the
> Rust `libid` cutover and retains the Zig siphon, higher rungs, and cold
> compiler). A findings
> document, not a plan of record. Builds on [43 — Modules & the Self-Hosted
> Layer](43-modules-and-config.md) (the staging model this extends),
> [42 — Object Canonical](../../design/42-object-canonical-relational-derived.md),
> [40 — The Image](40-the-image.md), [32 — Refound on libid](32-refound-on-libid.md).
> Code: `src/id/*.zig`, `src/id/stdlib.birnam`, `modules/*.birnam`.
>
> **Rev. 2 changes:** re-costed every migration estimate against the source after
> an adversarial review found the rev. 1 numbers optimistic; added the standing
> requirement that a **full independent cold compiler is retained** for from-zero
> bootstrap; reclassified the current Zig compiler as durable-dual (not a ratio
> target);
> corrected the fixpoint gate; moved codegen's runtime shims into the floor.
>
> **Rev. 4 correction:** “from zero” means from source using the declared
> non-Birnam implementation toolchains, with no prebuilt Birnam binary or
> artifact. It does not mean “Zig only.” Design 61 therefore may replace libid
> with Rust while Zig remains the current cold compiler and higher-rung host.

## 0. The question and the three constraints

What does moving birnam *toward* self-hosting look like, under three standing
constraints:

1. **The from-zero source-bootstrap invariant.** We must *always* be able to
   bring the whole system up from a source checkout using its declared
   non-Birnam implementation toolchains, with no prebuilt Birnam binary, image,
   compiled module, or other Birnam artifact. The implementation languages are
   not themselves constitutional. Under Design 61 the declared toolchains are
   Zig and Rust.
2. **An independent cold compiler is retained.** The current
   `lang.zig`+`codegen.zig` compiler remains the cold compiler until an
   equivalent non-Birnam replacement lands together with the same clean
   source-bootstrap proof. Removing or replacing it may not create a dependency
   on an already-built Birnam system.
3. **Minimize duplicated native semantics, not a language-specific LOC count.**
   Moving library and language semantics into Birnam is valuable when the old
   native implementation can be deleted safely. Translating an irreducible floor
   from Zig to Rust may improve safety or maintainability, but is not
   self-hosting progress by itself.

Constraints #1 and #2 together **dissolve** the collision rev. 1 worried about.
Rev. 1 treated "the cold compiler must survive to compile the Birnam compiler" as
a problem to engineer around (a minimal `codegen0`). The durable answer is to
keep a complete independent cold compiler on purpose. Today that compiler is
Zig; the invariant permits replacement only if the replacing non-Birnam compiler
arrives with an equivalent cold-bootstrap proof. This removes the cold compiler
from the self-hosting ratio game (§5) without declaring Zig permanent.

## 1. Where we are — much further than the ratio suggests

Production Zig (non-test) and birnam at the 2026-07-09 snapshot (verified with
`wc -l`):

| Bucket | LOC |
|---|---:|
| Zig production total (`src/id/*.zig`, excl. `counter.zig` demo) | **13,039** |
| Zig tests (`*_test.zig`) | ~4,700 |
| birnam — all | **5,042** |
| birnam — library only (`stdlib`/`toml`/`config` + `modules/`) | 2,724 |

Headline ratio ~2.6:1 Zig-to-birnam (or ~1.7:1 library-only). **But the ratio
understates how far self-hosting has already come**, because the cheap, safe
migrations are largely *done*:

- The **collection higher-order protocol** (`collect:`/`select:`/`inject:`/
  `detect:`/equality) is birnam (`stdlib.birnam`) over a single native fold; the Zig
  `collection.zig` is now only the data structures.
- **Integer/Float derived arithmetic** (`negated`/`abs`/`even`/`max:`/`factorial`)
  is birnam; `prelude.zig` keeps only the primitive `+ - * / mod <` and syscalls.
- The **module dependency graph** is store facts with a birnam-visible surface; the
  **TOML parser and config layer** (`toml.birnam`, `config.birnam`) are pure birnam;
  the entire `modules/` library (file, log, time, http, socket, json, lsp, doc,
  process, os — 2,304 LOC) is birnam over `Alien`.

So the remaining Zig in that snapshot was not a big undifferentiated pile of
migratable library code. It was mostly **floor, driver, perf-core, and the
bootstrap compiler**. Design 61 has since replaced the `libid` part of that floor
with Rust; the Zig driver, higher native rungs, and bootstrap compiler remain.
The honest near-term self-hosting prize is still smaller than a language LOC
ratio suggests.

## 2. The durable native substrate: floor + driver + cold compiler

Three categories of native code remain outside the self-hosted layer. The table
is a Zig-specific snapshot from 2026-07-09; Design 61 has replaced the `id.zig`
implementation with Rust without changing whether that behavior is floor.

### 2a. The native floor (~8,000 Zig LOC in this snapshot) — the mechanism sends run on

This behavior remains native because it *is* what a message send, allocation, or
syscall bottoms out in.

| File | LOC | Why floor |
|---|---:|---|
| `id.zig` | 1,327 | Piumarta object system: vtables, `send`/`bind`, `alloc`, interning. Design 61 has replaced this production implementation with Rust and retained it as the oracle. |
| `idapi.zig` | 260 | The C-ABI seam every module binds against. |
| `store.zig` | 1,001 | Historied cell/fact substrate; dispatch/deref/query bottom out here (perf floor). |
| `alien.zig` | 960 | The C-ABI/`dlopen` gateway — *how* birnam reaches libc at all (goal #4). |
| `tcc.zig` | 478 | libtcc — the native-code backend that makes machine code. |
| `image.zig` | 976 | mmap, pointer relocation, precise heap. Raw memory. |
| `peg.zig` | 838 | The PEG **matcher** (hot backtracking loop). Only the matcher — the grammar-*construction* API is birnam-visible. |
| `collection.zig` | 1,405 | Vector(RRB)/Map(HAMT)/Range **structures** (perf). Their protocol is already birnam. |
| `prelude.zig` | 776 | Primitive arithmetic + `System` syscalls. Derived methods already migrated. |

### 2b. Irreducible driver (~1,170 LOC, inside `main.zig`)

`main.zig` is 1,371 LOC; **only ~200 is migratable** (§3), and even those keep their
current Zig scaffolding. The rest is the native driver that currently boots
Birnam:

- **Every current entry point supplies a native stack anchor.** Each Zig `runX` calls
  `gcSetStackBottom(@intFromPtr(&anchor))` (`main.zig:789`); the conservative GC
  scans from that local. Any future non-Zig or stackless driver must replace that
  root-discovery contract explicitly before this frame can move.
- Bake machinery (Mach-O/ELF trailer, `selfExeBytes`, ~170 LOC), `runDemo` floor
  self-test (~180), stack/stdlib load orchestration + path resolution (~250),
  interactive stdin REPL byte-I/O (`readRawLine`/`isBalanced`/`readForm`, ~230),
  argv dispatch (~135), `wrap` scaffold (~90).

Count this with the floor: **floor + driver ≈ 9,200 LOC** is the true asymptote.

### 2c. The current cold compiler (~1,900 Zig LOC) — durable under constraint #2

`lang.zig` (293) + `codegen.zig` (1,609) are the current from-zero compiler. They
stay while they are the declared cold compiler so a source checkout can compile
Birnam without a Birnam artifact. They may be replaced only by an independently
bootstrappable non-Birnam compiler with an equivalent proof. Two consequences
the rev. 1 draft missed:

- **~400–500 LOC of `codegen.zig` is itself floor**, not emitter: the runtime shims
  the emitted code links against — `birnam_global`/`birnam_send`/`birnam_global_set`
  are `@export`-ed C symbols (`codegen.zig:312–362`), plus the `dlopen` artifact
  loader (`mLoadUnit`, 1340–1356) and baked-handle patching. A birnam codegen
  literally cannot `@export` the C symbols its own output calls. This half never
  moves while this generated-C ABI remains.
- Because an independent cold compiler is retained anyway, **migrating the compiler to Birnam is
  additive, not subtractive** — you end with two compilers on purpose (§5). It is
  therefore *not* a ratio lever.

## 3. The genuinely reducible surface (re-costed honestly)

Migrate to birnam, **delete the Zig**, and the retained Zig compiler (2c) still
recompiles the birnam source from zero. No duplication, real Zig reduction. The
rev. 1 "Tier A ~2,300 LOC" was inflated (mostly by over-counting `main.zig`); the
honest near-term figure is **~700–750 LOC**, plus a perf-gated relational slice.

| Target | Zig removable | Notes |
|---|---:|---|
| `error.zig` protocol → birnam | ~110 ✅ SHIPPED | The control-flow protocol (`on:do:`/`ensure:`/`ifError:`/`printString`/String raising) moved; the representation, the **DNU swap** (`objDnu`, installed on `object_vt`), and the factory stay in core. [44a](44a-pilot-error-birnam.md). Actual: `error.zig` 286→179. |
| `main.zig` subcommand boot-strings | ~200 ✅ SHIPPED | Collapsed `runLsp`/`runDoc`/`runDocgen`/`runListen` to one generic `runCli` → `modules/cli.birnam` ("load the dispatcher, pass argv"); the driver frame stays (2b). Actual: `main.zig` 1371→1276. |
| more `stdlib` library protocol | ~100–200 ✅ SHIPPED (measured-partial) | The string protocol moved (isEmpty/notEmpty/startsWith:/endsWith:/includesSubstring:/words/do:/inject:into:); the per-byte **scanners** split:/trimmed/indexOf: are **perf-retained natives** (see §7 Phase 2 — a send measures ~1.5–2µs, so derived bulk scans are ~10×), and asLowercase/asUppercase/replace:with: wait on the Phase-5 StringBuilder. Actual: `collection.zig` 1405→1358. |
| `main.zig` console REPL (rev. 3 addition) | ~190 ✅ SHIPPED | The console moved to the compiled `#repl` module (consoleInit/consoleStep/consoleDone + the form reader, `:commands`, `:globals`, config-main); `runRepl` is a runCli-style host stub. The installed unit carries its source metadata and loads through `Module`, with a process-local compiled host-capability path for recovered images that do not contain Repl. **The send gate applied twice**: the per-byte balance scan became the native `bracketDepth` beside split:/trimmed (5KB paste 1.4s derived → 0.17s native), and the session *loop* stayed a Zig `while consoleStep` frame — birnam recursion segfaulted a ~1700-line piped session. (Superseded once native `whileTrue:` shipped: the loop moved into `[Repl console]`, stack-flat at 10k piped lines; the binary sends one selector and reports a faulted session's error value.) Behavior locked by `scripts/repl-golden.sh` (21 e2e transcripts); one deliberate change: `BIRNAM_NO_STDLIB` has no REPL now (exit 1). Actual: `main.zig` 1276→1032, `collection.zig` +36. |
| **Near-term total (clean, low-risk)** | **~400–500 ✅ ALL SHIPPED** (≈448 net) | Invariant untouched, no duplication. |
| `relational.zig` **surface** (Tier B) | ~300–400 | Perf-gated. Lift query planning/negation/aggregation wiring; **keep the native nested-loop join** (`relational.zig:383`) — part of the read-10–50× budget. Partial, behind a benchmark. The measured send constant (§7 Phase 2) predicts per-ROW derived loops miss the gate the same way per-byte scans did — expect only the surface wiring to pass. |
| collection **tier-B boundary** (includes:/reversed/copyFrom:to:/asVector) | ~30 ✅ MEASURED, split verdict (2026-07-12) | Gated by the four bench/micro.birnam cases added for exactly this (`vector.includes-miss`/`vector.reversed`/`vector.copy-1k`/`range.asVector`). **Lifted**: Vector+Range `asVector` (identity / Collection's derived build — 1.02×, allocation-dominated) and Vector `includes:` as a *delegation* to the retained native `indexOf:` scan (the full-derived Collection form measured **116×** — the send gate at its purest). **Stayed native by measurement**: `reversed` (2.58×) and `copyFrom:to:` (1.60×) — per-row loops, exactly as §7 predicted. Macro cases moved 0% throughout: the gate is protecting large-vector tails, not the measured workloads. Bootstrap note: modules.birnam's topo-sort membership became `indexOf: … notNil` (includes: is stdlib now, above the manager's rung). |

**`modules.zig` ✅ SHIPPED as Phase 5b (2026-07-10) — deleted whole (−373, plus its
−94 Zig test).** Rev. 2 called it boot-critical-so-maybe-never; the resolution was
the predicted **bootstrap restructure**, and it came out cleaner than the rev.-2
sketch: `loadStdlib` loads `modules.birnam` FIRST, hardcoded, through the same
`loadSelfHosted` dylib-else-embedded-source floor as the rest — so the from-zero
guarantee is the *same one stdlib already has* (the Zig compiler evals the source; no
prior birnam artifact needed), and `--compile-module` runs the bare floor (no
self-hosted layer at all), so the system that builds the modules dylib never runs the
artifact class it produces. The rev.-2 "loadName is genuine FFI floor with no clean
non-boot subset" claim was WRONG on inspection: boot never used `loadName` (it uses
`loadSelfHosted`/`[Codegen load:]`); the genuinely-native missing piece was reading
dependency metadata without installing. That dependency-only Codegen bridge has
since been replaced by Design 54's `[Artifact inspect:]`, which reads a complete
pointer-free descriptor without `dlopen`. Everything else — declaration facts,
the topo-sort, the deps-first
runtime `load:` orchestration, `libDir` — is birnam (`src/id/modules.birnam`, 184
lines, native floor only since it precedes stdlib). `[Reflect nameOf:]` landed as
predicted. Self-registration sits at the module's END (the method must exist to
run); the first-statement convention is load-bearing only for the driver's
`scanRequires` first-substring scan, which this module never enters (loaded by
name, first; its body contains the literal selector "named:requires:" the scan
would misread). `Module` is now a Globals binding, not a C export; the driver
re-sends `kernel:`/`libDir:` each boot (images included, pre-checkpoint) so a
booted image loads dylibs from ITS tree. Trade-offs accepted: `BIRNAM_NO_STDLIB`
now has no `[Module load:]` (the manager IS self-hosted — consistent with "skips
the whole self-hosted layer"), and the stale-dylib authoring gotcha now covers
`modules.birnam` too.

Two corrections to rev. 1 folded in here:

- **`lang.zig` is *not* a near-term win under constraint #2.** Rev. 1 (and the
  reviewer) called it the easiest transcription — mechanically true (it is pure
  `Peg`-facade sends, `lang.zig:44–223`). But the retained Zig compiler *needs a Zig
  parser* to cold-compile anything, so `lang.zig` can't be deleted. A `lang.birnam`
  would be **additive/duplicative**, and it is part of the long-term compiler track,
  not a ratio play. Defer it to §5.
- **`repl.birnam` is not a counterpart to `runRepl`.** `modules/repl.birnam` is the
  *out-of-process socket* REPL; `runRepl` (`main.zig:1257`) is the *interactive
  stdin* REPL and owns terminal I/O + the GC anchor. It stays.

## 4. Is the compiler even migratable? (yes, with real caveats)

Confirmed against source, because it's the surprising part. `codegen.zig`'s
*emitter* half (~800–900 LOC) is a string-templating pass over a message-driven CST
walk: it walks the tree only through sends (`ruleOf`, `childAt:`, `text`) and emits
C as plain concatenation of `birnam_send(...)`/`birnam_mkString(...)` text. So a
`codegen.birnam` needs **no genuinely new object protocol**. Caveats rev. 1 glossed:

- **String is immutable/persistent.** Building a multi-KB C translation unit by `,`
  concatenation is O(n²); `codegen.zig` uses `std.ArrayList(u8)` throughout. birnam
  needs a **mutable string buffer** (a small `StringBuilder` over the `Word`/byte
  primitives, or a native buffer type) before a birnam codegen is practical. "No new
  primitive" was too strong — no new *semantics*, but yes a new *utility type*.
- **Artifact mode must be reproduced.** `__amblk` salting (`Wyhash` per source),
  `__birnam_unit_init`, and the embedded artifact descriptor — the fiddly
  loadable-dylib path (docs 43 and 54). The birnam codegen must re-implement all
  of it.
- **The runtime-shim half never moves** (2c).

## 5. The compiler: durable-dual, long-term, not a ratio target

Given constraint #2, the compiler story is simpler than rev. 1's "Option 3."

**Reject the minimal-`codegen0` idea.** Rev. 1 proposed carving a ~500-LOC Zig
stage-0 that only compiles the birnam compiler. Two reasons to drop it: (a) the
review showed it's really ~1,200–1,350 LOC, because a compiler uses nearly the whole
language, so the saving is illusory; and (b) the current independent cold compiler
is already the full Zig compiler — a minimal subset-tracking stage-0 is strictly
more complex and more fragile than keeping what works. **Keeping the whole
`lang.zig`+`codegen.zig` is the simplest current way to satisfy constraint #2.**

So self-hosting the compiler, when it eventually happens, is:

- **Additive by design.** Add `codegen.birnam` (+`lang.birnam`) that the *warm*
  system can use; retain `lang.zig`+`codegen.zig` as the *cold* bootstrap tool
  unless an equivalent non-Birnam cold compiler later replaces it. Two compilers
  on purpose. Net Zig removed: **zero** — this is bought for
  malleability (extend the grammar/backend in birnam), **not** for the ratio.
- **Gated on the mutable-buffer utility (§4) and artifact-mode parity.**
- **A long-term terminal goal**, deliberately deferred so the near-term work (§3)
  stays small and low-risk.

**The fixpoint gate, corrected.** The milestone is a *self-application* fixpoint,
not a single recompile. The Zig compiler (C0) and the birnam compiler emit different
C *text*, so `C0(codegen.birnam) = C1 ≠ C2 = C1(codegen.birnam)`; identity only
appears at `C_N == C_{N+1}` — a ≥2-stage bootstrap (as GCC does). And compare the
**emitted C text** (deterministic; the `__amblk` salt is a pure hash of the source),
**not the dylib bytes** (libtcc/Mach-O determinism is unverified and fragile).

## 6. What "reduce the ratio" actually buys (metric honesty)

The review's sharpest structural point: at the compiler, "Zig lines deleted,"
"birnam lines added," and "system simpler" **diverge**. Migrating the compiler adds
~1,600 verbose birnam lines and deletes ~0 Zig (constraint #2), so the *ratio*
improves while total code and maintenance burden *rise*. A ratio target that rewards
that is being gamed.

So scope the metric to where it's coherent: **count native semantic lines
*deleted without duplication***. The historical measurements in this document
remain Zig-specific because that was the implementation at measurement time. Do
**not** count either a compiler duplication or a Zig-to-Rust floor translation as
self-hosting progress; book the former under malleability and the latter under
safety/maintainability. In the warm
image the active-code ratio also shifts toward birnam as modules load compiled
dylibs, but that's a runtime property, not a source-tree deletion — don't conflate
them.

## 7. Roadmap (each phase ends green, invariant intact)

- **Phase 0 — make the ratio visible.** A `zig build loc` that prints
  Zig-deleted-without-duplication vs birnam-added, so progress is measured on the
  coherent metric (§6), not the headline ratio.
- **Phase 1 ✅ SHIPPED — `error` object protocol → birnam.** Kept the swap +
  representation in `error.zig`. Exercised the "delete the Zig, recompile from source"
  loop on a low-stakes target. [44a](44a-pilot-error-birnam.md). (~110; also added the
  `Reflect vtableOf:` mirror + `isSymbol:`/`isString:`, and refounded `Symbol` as a
  proper prototype.)
- **Phase 2 — a low-risk, non-boot-critical target** ✅ SHIPPED, both halves.
  First the subcommand collapse (also completing Phase 3), then the **string
  library protocol → stdlib.birnam**: isEmpty/notEmpty (via Collection),
  startsWith:/endsWith:, includesSubstring:, words, and the do:/inject:into:
  fold (derived over Range's native fold + at:/byteAt:/copyFrom:to:), with the
  old native arg-guard semantics pinned by modules/stdlib-test.birnam. Two
  deliberate retentions discovered by MEASURING (a send costs ~1.5–2µs, so a
  derived per-byte scan over bulk text is ~10× end-to-end — docgen went 4s→39s
  in the naive all-birnam version): the per-byte SCANNERS split:/trimmed/
  indexOf: stay native ("lift the surface, keep the native join", as with
  relational). StringBuilder has since shipped; asLowercase/asUppercase moved
  to birnam over it, while hot `replace:with:` remains native by measurement.
  Result: docgen 4s→9.4s (accepted, batch-only), everything
  interactive unchanged. **NOT `modules.zig`** — it is boot-critical (§3
  correction); defer it to a deliberate bootstrap-restructure step, not an
  early one.
- **Phase 3 ✅ SHIPPED — `main.zig` subcommand collapse.** doc/docgen/--lsp/
  --listen route through ONE generic driver (`runCli`: boot + GC anchor + argv
  handover) to the compiled `#cli` module, whose
  `[Cli run:source:rustWorkspace:guides:]`
  dispatches by driving the modules' own public API (`[Doc cli:]`,
  `[Doc buildSite:zig:rustWorkspace:guides:]`,
  `[Lsp serveStdio]`, `[Repl serveOn:]`) — the CLI is one more client of the
  libraries, and every module stays fully usable from code without it
  (locked in by cli-test.birnam's library-use tests). main.zig 1371→1276; the
  four per-command Zig handlers (~200) became ~160 lines of birnam. Bonus:
  `--listen` now resolves modules/ exe-relative, so it works from any cwd, and
  argv reaches birnam through an escaper instead of raw interpolation.
- **Phase 4 (perf-gated) — lift the `relational` surface**, keep the native join.
  Behind a benchmark. (~300–400)
- **Phase 5 ✅ SHIPPED — the mutable-buffer utility** (`StringBuilder`) — now used
  across JSON, HTML, docs, benchmarks, and self-hosted case transforms; still a
  prerequisite for any birnam codegen.
- **Phase 5b ✅ SHIPPED — `modules.birnam` via compile-first.** The module manager
  as a self-hosted module loaded before the rest of the self-hosted layer, dylib
  preferred / embedded source the floor; `[Reflect nameOf:]` added for
  path-building. The original dependency-only Codegen probe is now superseded
  by the ID-floor artifact inspector. `modules.zig` deleted whole. Details in
  the §3 entry above.
- **Phase 6 (long-term, terminal) — `codegen.birnam`/`lang.birnam`** as an additive
  warm-path compiler; retain the independent Zig cold compiler unless an
  equivalent non-Birnam replacement is proved in the same change; verify the
  ≥2-stage emitted-C fixpoint. Pursue only when self-hosting-the-compiler is worth
  its own weight — it is **not** a ratio move.

Projected source-tree end state before Phase 6: Zig **13,039 → ~11,900–12,000**
(delete ~730 near-term + ~350 relational). **Actual with the near-term table
complete (2026-07-10, console REPL shipped): 12,711** — on the projected
trajectory with the string scanners (and now `bracketDepth`) deliberately
retained. What remains to the projection is the perf-gated relational surface.
Modest — because §1: the system is already self-hosted wherever it was cheap to
be. The floor+driver+bootstrap-compiler (~11,100) is the durable core, and most
of its behavior is *supposed* to remain native or independently bootstrappable;
its implementation need not remain Zig.

## 8. Risks & non-goals

- **Don't chase the ratio into the perf cores** (`store`, `peg`-matcher, `collection`
  structures, the relational join). Migrating them trades native speed for a metric.
  Lift surfaces, keep kernels native, always behind a benchmark.
- **Don't build `codegen0`.** It's more complex than keeping the full Zig compiler
  and saves nothing real (§5).
- **Non-goal: minimizing any one implementation language.** The target is the
  right native substrate plus deleting library semantics that are duplicated in
  native code for historical reasons. A Birnam that needs a prebuilt Birnam
  binary or artifact to build from zero has failed constraint #1; a checkout
  that needs both declared Zig and Rust toolchains after Design 61 has not.

## 9. One-line summary

Most of the cheap self-hosting is already done; the honest near-term win is
**~640 LOC** (`error` protocol, `modules`, `main` subcommands) deleted-and-recompiled
with zero duplication, plus a perf-gated relational slice. A complete
**independent cold compiler** remains mandatory; the current one is Zig, but Zig
is not itself the invariant. A warm Birnam compiler is therefore a long-term
malleability goal, **not** a ratio play, and should be deferred behind a
mutable-string-buffer prerequisite and a corrected ≥2-stage, emitted-C fixpoint
gate.
