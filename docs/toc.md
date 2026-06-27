# Birnam Documentation — Table of Contents

This documentation describes Birnam as it currently exists and as it is
*intended* to work — both the running implementation and the design intent
behind it. It is organized ground-up: first the **concepts** (the model and the
"why"), then the **implementation** (the "how", file by file), then **usage**
(how you actually drive it).

> Status snapshot: Phases 0–16 (+15.R) complete and merged; Phase 19 FFI
> absorption batch 1 merged; ADR-perf-1 (performance model) decided. See
> `ROADMAP.md` for the authoritative phase ledger and `VISION.md` for the
> locked design vision.

Each entry below names the topic and the file that will hold it (under
`docs/`). Files marked _(planned)_ do not exist yet; this TOC is the build
order.

---

## Part 0 — Orientation

- **What is Birnam** — `00-what-is-birnam.md` _(planned)_
  One-paragraph pitch, the elevator version of the substrate idea, who it's for,
  and how to read the rest of these docs.
- **Document map** — `00-document-map.md` _(planned)_
  How `VISION.md`, `DESIGN.md`, `DATALOG.md`, `ROADMAP.md`, `README.md`, and
  this `docs/` tree relate — which is authoritative for what, and when each is
  updated.

## Part 1 — Concepts (the model and the intent)

- **The vision: a relational fact-store substrate** — `10-vision.md`
  Why everything is a fact; the "fact store is the substrate, not a feature"
  thesis; the locked design vision and its constraints (HAMT + RRB only, the
  `-1` trick preserved at the materialization layer).
- **North Star — the cell refounding** — `10a-north-star-cell-model.md`
  The conceptual center the system is steered toward: object model and relational
  model are one operation (`get`/`assert` over a named, historied cell store) read
  two ways. Why the five layers are derived caches, not foundations; how
  D5/D5.1/Q1/Q2/Q3 are facets of it; native code as a cache of bytecode-cells.
- **The destination — extensional identity over an intensional floor** — `30-the-destination.md`
  The long-horizon target for the bottom layers. The eid is a *superimposition*,
  not the floor (the Piumarta id closure is); identity becomes overridable policy
  (`identityOf:` — candidate-key first, content-addressed later); per-layer
  primacy (struct-primary floor, fact-primary above) refines `10a` D5.2; the
  pointer-identity-first bootstrap audit (~80% mechanical, ~20% the dispatch seam).
- **ADR — Refound on libid (build up, don't invert in place)** — `32-refound-on-libid.md`
  The route change (2026-06-27): W7 proved a floor dispatches with zero facts, so
  the in-place inversion is frozen as a proof; we reach the destination by building
  UP from the clean libid floor (`src/id/`), with the fact store as a later module.
- **The fact model** — `11-fact-model.md`
  Entities, attributes, values, transactions (E/A/V/T). Datomic-style five
  indexes (eavt / aevt / avet / vaet / meat). Cardinality (`:one` / `:many`).
  What an "entity id" (eid) is and the kernel-eid vs user-eid split.
- **The six indexes** — `11a-the-six-indexes.md`
  From first principles: why indexing means keeping one covering structure per
  question shape, deriving the five fact indexes (EAVT/AEVT/AVET/VAET/MEAT) one
  question at a time, and the sixth — the object table (eid → live object) that
  completes the fact/object pairing. ValueCell vs PresenceCell, why all six are
  HAMTs, and why no fact pays for an index it will never be asked.
- **Everything is facts: reification** — `12-reification.md`
  Vtables, methods, AST nodes, and bytecode are all stored as facts. What it
  means for the language to be self-describing, and why dispatch reads facts.
- **The object model: message passing** — `13-object-model.md`
  Smalltalk-lineage objects and messages; selectors; `doesNotUnderstand:`; the
  number/boolean/nil protocols; vtables as entities.
- **Tagged pointers and the Ref encoding** — `14-refs.md`
  How a `Ref` packs immediates (small ints, eids, special values) into a word;
  the no-NULL philosophy (nil is a value; absence rides `ifAbsent:`); the `-1`
  trick and where it lives.
- **Time, history, and temporal queries** — `15-time-travel.md`
  How transactions give every fact a `tx`; `asOf:` / `history`; basis_t; the
  15.R retention / snapshot / excision model; why time-travel is now *opt-in*
  (concept-level; the cost story is in Part 1's performance page).
- **Persistent data structures** — `16-persistent-structures.md`
  HAMT (the index/map workhorse) and RRB vectors — what they are, why
  structural sharing matters for an immutable, historied store, and why these
  two only.
- **Collections and the sequence protocol** — `17-collections.md`
  Bracket-literal family `#( )` / `#[ ]` / `{ }` / `#{ }`; the unified sequence
  protocol (`collect:` / `select:` / `count` / `detect:` / `inject:into:` over
  vectors, ranges, maps, strings); ranges; reduced; transducers.
- **The performance model** — `18-performance-model.md`
  The central tension: the fact store is *both* the durable/queryable/
  time-traveled projection *and* the runtime object representation. The cost
  bands (read ~10×, write ~100–1000× native), the three-tier representation
  model, and what is inherent vs fixable. Summarizes ADR-perf-1.
- **Load-time memory: scaling code-as-facts** — `19-load-memory.md`
  Why loading a large program (the `logue` roguelike: ~4.4 GB) is expensive when
  every AST node and bytecode method is a write-through fact, what is separable
  from "everything is facts," and the two shipped levers (prune between file-in
  forms; one forced compaction at the load boundary) plus the deferred ones.
- **Code-fact tiers: no-history + index selectivity** — `19a-code-fact-tiers-plan.md`
  The build plan for two source-level levers `19-load-memory.md` defers: a
  per-attribute policy foundation (static code-attr set), the `:no-history`
  tier, and index selectivity (skip the never-read VAET index for code attrs).
  Sequencing, the deferred skip-AVET follow-up, correctness obligations, and the
  trail of two adversarial-review corrections.
- **RFC: transient load mode** — `19b-transient-load-rfc.md`
  The third deferred lever, split out because it needs a new write *mode*, not a
  passive guard: why deferred-publish batching fails (parse reads the store
  mid-parse), the read-through transient-index design, its CHAMP-core
  requirement, atomicity via copy-on-first-touch, and the safety gate.
- **W1: full-image save/load (scoping plan)** — `19c-w1-full-image-plan.md`
  The plan for ADR-image-1 W1: why it's not "delete one line" (the native
  fn-ptr residue), what `--boot-image` does today + its json/net gaps, the
  full-fact-image + native-relink-by-name design, and the staged
  (measure → write → relink → default → cleanup) implementation.

## Part 2 — Implementation (file by file)

- **Kernel overview and boot** — `20-kernel-overview.md`
  The `src/kernel` layout, what `bootstrap.boot` wires up (fact store + object
  table + interner), and the kernel/module split.
- **Re-layering coupling audit (W6 / ADR-image-1)** — `20a-relayering-coupling-audit.md`
  The kernel import-graph audit for the identity/fact-store re-layering: the
  reach-up surface (4 files), the ordered cut-list, and the Q1–Q3 mapping.
- **Ref encoding internals** — `21-ref-impl.md`
  `ref.zig` — bit layout, `mkSmallInt` / `mkEid` / specials, decoding.
- **The fact store** — `22-fact-store.md`
  `fact_store.zig` — `assert` / `retract` / `currentValue`, the five indexes,
  history append, `compact`, `enableIndexArena`, basis_t.
- **HAMT and RRB** — `23-hamt-rrb.md`
  `hamt.zig`, `rrb.zig` — node layout, path-copy, structural sharing.
- **Interner** — `24-interner.md`
  `interner.zig` — symbol/selector interning and eid assignment.
- **Object table, promotion, and demotion** — `25-promotion.md`
  `object_table.zig`, `promotion.zig` — hotness, materialized `[vt][slots]`
  structs as a read-through cache, in-place slot patch (perf-1.A), keep-hotness
  on demote (perf-1.B), the demote triggers.
- **Dispatch** — `26-dispatch.md`
  `send.zig`, `dispatch.zig`, `mcache.zig`, `kernel_eids.zig` — `sendN` → `bind`
  → `vtableEidOf`; the 1024-entry direct-mapped method cache; kernel eid ABI
  (append-only).
- **Compiler, bytecode, and VM** — `27-compiler-vm.md`
  `compiler.zig`, `bytecode.zig`, `vm.zig`, `closure.zig` — source → AST →
  bytecode → execution; closures and frames.
- **The PEG parser** — `28-peg.md`
  `peg.zig` and the `repl/parser.zig` front end — grammar, how source becomes
  AST facts.
- **Datalog: queries, rules, pull, predicates, aggregates** — `29-datalog.md`
  `query.zig`, `rule.zig`, `pull.zig`, `predicate.zig`, `aggregate.zig`,
  `saved_query.zig` — the query engine over the indexes; see `DATALOG.md` for
  the surface language.
- **Reference cells (temporal instances)** — `2a-cells.md`
  The Phase-16 Tier-E lead feature — a cell *is* an entity; `cell:` / `ref:` /
  `atom:`; `value` / `set:` / `asOf:` / `history`; `transact:`; metadata
  (`withMeta:` / `meta`); how cells realize opt-in time-travel.
- **Transactions** — `2b-transactions.md`
  `tx_engine.zig` — atomic-by-single-transactor semantics, `db transact:`.
- **Modules** — `2c-modules.md`
  `module.zig`, `module_abi.zig`, and the `src/modules/*` units (lang, query,
  peg, repl, hello) — the dylib ABI discipline (append-only `IdApi`),
  `id_api.zig`.
- **FFI: shimless C interop** — `2d-ffi.md`
  `ffi.zig`, `ffi_module.zig` — vendored dyncall; sigchars; deferred-dlopen
  `Module load:` / `wrap:as:signature:`; string/nil marshaling; out-params;
  pointer deref / peek-poke; struct-by-value (and the arm64-apple aggregate
  limitation). The Phase 19 absorption roadmap.
- **Images, freeze, and AOT** — `2e-images-aot.md`
  `image.zig` (`.bim` snapshots), `freeze.zig`, `loader.zig`, `macho.zig`,
  `elf.zig`, `aot_main.zig` — saving/loading a live image and ahead-of-time
  binaries.
- **Memory management** — `2f-memory.md`
  `arena.zig`, `arenas.zig`, `gc.zig`, `prune.zig` — arenas, the index epoch
  arena, prune/compact, and why sustained mutation needs prune-in-loop.

## Part 3 — Usage (driving Birnam)

- **Build and run** — `30-build-run.md` _(planned)_
  `zig build`, the shell/REPL, AOT binaries, build steps (including
  `kernel-bench` / `write-bench`). Prerequisites and ReleaseFast.
- **Language tour** — `31-language-tour.md` _(planned)_
  Syntax by example: literals, the bracket-literal family, blocks and block
  params, keyword messages, the `|>` pipe, `let` and destructuring, `:=`.
- **Working with the stdlib** — `32-stdlib.md` _(planned)_
  The sequence/collection protocol in practice; maps; ranges; transducers;
  `validate:` / `zip:` / `whenAll:`.
- **Querying facts** — `33-querying.md` _(planned)_
  Datalog from the REPL; pull; rules; saved queries; aggregates. Cross-ref
  `DATALOG.md`.
- **Reference cells and time-travel** — `34-cells-usage.md` _(planned)_
  Minting cells, reading/setting, `asOf:` / `history`, atomic `transact:`,
  opting objects into temporal behavior.
- **Wrapping a C library (FFI walkthrough)** — `35-ffi-walkthrough.md` _(planned)_
  End-to-end libtecla example from `examples/libtecla-module.birnam`: loading
  dependencies for flat-namespace symbols, `wrap:as:signature:`, calling, and
  the gotchas found while driving it live.
- **Images and persistence** — `36-images.md` _(planned)_
  Saving and loading `.bim` images; what persists; AOT packaging.
- **Benchmarking safely** — `37-benchmarking.md` _(planned)_
  The two harnesses (`kernel-bench`, `write-bench`), why benchmarking is
  memory-dangerous (the crash story), the in-process caps, and how to read the
  numbers. Mirrors the README Benchmarks section.

## Part 4 — Reference and appendices

- **Roadmap and phases** — `40-roadmap.md` _(planned)_
  Pointer to `ROADMAP.md`; how phases map to features; the phase-as-commit
  discipline.
- **ADRs** — `41-adrs.md` _(planned)_
  Architecture decision records, starting with ADR-perf-1 (projection
  boundary, opt-in time-travel, three-tier representation).
- **Glossary** — `42-glossary.md` _(planned)_
  eid, attr, tx, basis_t, vtable, selector, materialization, promotion/demotion,
  reification, projection, HAMT, RRB, Ref, sigchar, cell, transient.
