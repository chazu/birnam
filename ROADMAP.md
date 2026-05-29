# Birnam Roadmap

Detailed work sequence from current zig/birnam (linear-scan vtables, no fact store) to the locked vision (relational substrate, tagged pointers, HAMT+RRB everywhere). Reasoning per phase.

This roadmap follows VISION.md. Open questions in VISION.md must be resolved before starting Phase 1.

## Status (as of 2026-05-29)

**Completed:**
- Phase 0: demolition + scaffolding ✓
- Phase 1: HAMT + RRB persistent collections ✓
- Phase 2: tagged pointers + object table + kernel EIDs ✓
- Phase 3: fact store with 5 indexes + tx engine ✓
- Phase 4: id kernel bootstrap atop fact store ✓
- Phase 4.5: strings, interning, named selectors ✓
- Phase 5: module system rewrite (modules as fact subsets) ✓
- Phase 6: REPL module (REPL as fact-shaped object) ✓
- Phase 7: Datalog relational core (negation, disjunction, history, set semantics) ✓
- Phase 7.1: rules + recursion + stratification ✓
- Phase 7.2: aggregation (Date-aligned, distinct-by-default) ✓
- Phase 7.3: subqueries, saved queries, pull, reification, rule walk-back ✓
- Phase 7.4: predicates + functions (builtin arith/min2/max2 + user native preds) ✓
- Phase 8: promotion/demotion (hotness counter, threshold auto-promote, FFI pin, shape-change demote) ✓
- Phase 7.5: metacircular bind (`:bind` override along delegate chain, recursion guard limit=4, full mcache flush on mutate) ✓
- Phase 9 (substantial): boxed Float + FFI pointer; dyncall 1.4 vendored; sig-string parser; ffi.invoke engine; ffi.registerWrap + per-arity shim closures; send-dispatched sqrt/strlen end-to-end; Mach-O 64-bit reader with symbol iterator ✓
   - Deferred to 9.5: Module entity wrapper (`Module load:`/`wrap:as:signature:` as send-dispatchable selectors) and lazy `Module>>symbols` enumeration via the Mach-O reader

**Test count: 208/208 across 57 build steps.**

**Next: Phase 9.5** — Module dylib (entity-level wrap of registerWrap), lazy Module>>symbols using shipped Mach-O reader, ELF reader for linux gate flip, Module unload + stale-pointer flags, schema/multi-cardinality.

---

---

## Phase 0 — Demolition + Scaffolding

**Goal:** Clean repo down to what survives. Keep `shell.zig`, `build.zig` shell, test scaffolding. Delete the rest.

### Work items

0.1 Branch current `main` to `pre-relational-archive` so the v2 code is recoverable.

0.2 Delete `src/id/`, `src/peg/`, `src/lang/` from working tree on main.

0.3 Reduce `build.zig` to: shell exe target + test scaffolding. Drop id/peg/lang module wiring.

0.4 Keep `src/shell.zig` but **stub out** module loading for now — shell boots, prints a banner, exits. Re-wire as id-kernel module lands.

0.5 Confirm `zig build` and `zig build test` are green on the reduced tree.

### Reasoning
Object-representation changes (pointer → tagged word) ripple through every file in `src/id/` and `src/peg/`. Bootstrap reorders (HAMT first). `src/lang/` is wholly misaligned (tree-walker over arena-allocated Zig AST). Mutating these in place would generate more churn than rewriting. Archive-branch preserves the bootstrap-fixed-point and PEG nodes work as reference; we won't lose anything.

---

## Phase 1 — Persistent Collections Kernel

**Goal:** HAMT + RRB-Tree, both production-quality, both with thorough test suites. Pure Zig. No object model yet.

### Work items

1.1 HAMT design notes: branching factor 32, popcount-bitmap-compressed nodes, CHAMP shape (split bitmap into "value-slot" + "subnode-slot"). Decide leaf-inlining strategy.

1.2 HAMT impl: `Hamt(K, V)` generic over key/value. Operations: `get`, `assoc`, `dissoc`, `iter`. ~500 LOC.

1.3 HAMT tests: insert/lookup/delete invariants, structural-sharing property check (path-copy doesn't touch unrelated subtrees), collision-bucket behavior, large-N (100k) sanity.

1.4 RRB-Tree design notes: 32-way branching, relaxed nodes carry width metadata only when not full. Decide focus-pointer / transient-builder optimizations (later phase).

1.5 RRB impl: `Rrb(V)` over value type. Operations: `nth`, `set`, `push`, `pop`, `concat`, `split`, `iter`. ~500 LOC.

1.6 RRB tests: index/update invariants, concat/split correctness, mixed-arity tree handling.

1.7 Microbenchmark both vs `std.AutoHashMap` and `std.ArrayList` to confirm we're in the right ballpark (within 2-3x for ops we expect to use).

### Reasoning
These are the two persistent structures the entire kernel rests on. Get them right once, in isolation, with strong tests, before anything depends on them. Battle-tested designs (Bagwell 2000, Bagwell & Rompf 2011, Steindorfer's CHAMP). No invention required.

---

## Phase 2 — Tagged Pointers + Object Table

**Goal:** Reference type, tag/untag machinery, object table.

### Work items

2.1 Decide tag layout (already locked in VISION.md): 3-bit tag, codes `000`/`001`/`010`/`011`.

2.2 Define `Ref` as a `u64`. Helpers: `tagOf`, `untag`, `mkPointer`, `mkSmallInt`, `mkEid`, `mkKeyword`.

2.3 Object table: `Hamt(u64, Ref)` mapping EID → current materialized reference (or sentinel for "cold, fact-only"). Allocator for fresh EIDs (monotonic counter).

2.4 Tests: tag round-trip, small-int overflow detection, eid alloc monotonicity, object-table get/assoc/dissoc.

2.5 Document: in code, what the four codes mean and where each may appear.

### Reasoning
Tagged pointers must land before the fact store because facts contain `Ref` values. Object table must land before the fact store because EID resolution needs it. Both must land before `bind` is rewritten because dispatch reads tags.

---

## Phase 3 — Fact Store

**Goal:** Four indexes, tx engine, assert/retract, basic point queries.

### Work items

3.1 Define `Fact` struct: `{ e: u64, a: u64, v: Ref, tx: u64, op: AssertOrRetract }`. 32 bytes packed.

3.2 Decide internal representation per index: HAMT keyed by composite (e.g. EAVT key = e then a then v). Likely HAMT-of-HAMT-of-HAMT-of-RRB-of-tx for one index.

3.3 Implement EAVT first. `assert(e, a, v) → tx`. `query_eavt(e, a) → Ref | nil` (current value). `query_eavt(e) → iter<(a, v)>`. `as_of(tx)` variant.

3.4 Add AEVT, AVET, VAET. Each index lives in the same tx-counter regime but with its own HAMT-of-HAMT spine.

3.5 Tx engine: monotonic counter, all four indexes updated in one logical step per tx. No partial-tx visibility.

3.6 Retraction: `retract(e, a, v) → tx` — appends retraction fact; queries skip retracted values.

3.7 Tests: assert+lookup round-trip, retract+lookup, as-of-tx time travel, all four indexes agree on the same fact, large-N (100k facts) sanity.

### Reasoning
This is the bedrock of everything above. Get it solid before any vtable/dispatch/method work depends on it. Tests must demonstrate the four-index invariant (the same fact is visible from any axis), because the metacircular tower will rely on querying from any direction.

---

## Phase 4 — id Kernel Rewrite (Bootstrap)

**Goal:** Vtable, selectors, send/bind, mcache — all rebuilt atop fact store.

### Work items

4.1 Design well-known EID assignments for kernel entities: 0 = nil, 1 = vtable_vt, 2 = object_vt, 3 = selector_vt, 4 = closure_vt, 5 = vector_vt, 6..N = kernel selectors.

4.2 Selector table: HAMT `str → EID`. Selectors are entities. `intern(str) → eid` asserts schema facts on first encounter.

4.3 Materialized struct layout: keep current `Vtable`, `Selector`, `Closure`, `Assoc` extern struct shapes — they become **materialized representations** of facts. `-1 vtable` trick preserved.

4.4 Pre-materialize kernel vtables: allocate libid structs for vtable_vt, object_vt, etc.; register raw-pointer refs in object table.

4.5 `bind` rewrite: tag-check ref; if raw pointer, load `vt = obj[-1]`; mcache lookup on `(vt_eid, sel_eid)`; on miss, query EAVT for `[vt_eid, "lookup:"_eid, ?method]`; cache + dispatch.

4.6 `assert_method(vt_eid, sel_eid, fn_ptr)` helper — asserts `[vt_eid sel_eid method_ref]`, invalidates mcache for `(vt_eid, sel_eid)`.

4.7 Wire up `send.zig` to new `bind`. Keep `Send` struct + `send0-3` sigs unchanged. Recursion guard still required for `lookup:` self-reference.

4.8 Smoke test: `send(metavtable_eid, "lookup:", sel) → method_ref` via the new path. Then call the method.

4.9 Tests: re-port the id module's existing tests (delegation chains, method override, intern stability) against the new substrate. All must pass.

### Reasoning
Bootstrap is the single trickiest piece of code in the system. Doing it right means: HAMT and fact store are already trustworthy (Phases 1-3 done), well-known EIDs are pre-assigned (no chicken-and-egg), and the recursion guard pattern from current `send.zig` carries over verbatim. Pre-materializing the kernel vtables avoids the "you need bind to install methods, but bind needs methods installed" tangle.

---

## Phase 4.5 — Strings, Interning, Named Selectors

**Goal:** Add the string Ref type and selector-name intern table so user code can declare new selectors. Prerequisite for Phase 5 (modules want to introduce new selector names at load time).

### Work items

4.5.1 String storage. Option A: strings as entities with a `:bytes` byte-array attribute. Option B: dedicated tagged-pointer Ref encoding pointing at heap-allocated `[u8]`. Decide before coding — likely B (smaller, no per-string fact bloat).

4.5.2 Bytes / string Ref encoding. Pick a tag code from the reserved 100..111 range, or extend an existing one with a sub-tag scheme.

4.5.3 String intern table: `Hamt([]const u8, u64)` mapping byte content → selector EID. Reverse map (EID → bytes) so selectors can be printed.

4.5.4 Assert `:name` facts for kernel selectors (the hardcoded 16..21 block) so reflection queries can name them.

4.5.5 `intern(str) → u64` API: lookup-or-mint. New EIDs allocate from user range. Persistent: same string always returns the same EID across a session.

4.5.6 Extend `IdApi` with `intern` and `selector_name` accessors.

4.5.7 Tests: round-trip intern, kernel selectors carry correct `:name`, idempotence across calls, AVET-backed reverse lookup `[?sel :name "lookup:"]`.

### Reasoning
Phase 4 deliberately ships with kernel-only selectors (hardcoded EID constants) so bootstrap stays minimal. Phase 4.5 is the smallest additive layer that makes the kernel usable from new modules — exactly what Phase 5 needs. Separating it keeps Phase 4's bootstrap debugging surface as small as possible.

---

## Phase 5 — Module System Rewrite

**Goal:** Modules are fact subsets. Hot reload = retract+assert.

### Work items

5.1 Decide module unit: tx-range vs module-attribute tag. (Open question — resolve before starting.)

5.2 Extend IdApi to expose: `assert`, `retract`, `query_eavt`, `query_aevt`, `query_avet`, `query_vaet`, `intern`, in addition to existing `send/bind/alloc`.

5.3 Update PEG module to use new IdApi shape. PEG node creation: instead of `obj.alloc(node_vt, sz)`, do `assert([new_eid, vt_attr, peg_node_vt_eid])`.

5.4 Shell module load = open dylib, call `birnam_module_init(ctx)`, module asserts its facts. Tagged with module identity for selective retract.

5.5 Hot reload via kqueue: on file change, retract all facts tagged with that module, dlclose, dlopen, init, assert new facts. One tx.

5.6 Tests: load + unload + reload cycle preserves system integrity. Asserted facts visible from query; retracted facts invisible; mcache invalidated correctly.

### Reasoning
Modules are how birnam stays composable. They must be fact-shaped to gain diff/history/query for free. The decision on tx-range-vs-tag affects fact storage overhead — needs resolution before code.

---

## Phase 5.5 — Module-Stack Fact Restoration (enhancement)

**Goal:** When module B's assertion supersedes module A's, unloading B restores A's value instead of leaving the attribute absent.

### Work items

5.5.1 Walk MEAT history on retract: if the retracted (e, a) was last asserted by the unloading module, scan for the most recent assert from a still-loaded module and restore it.

5.5.2 Alternatively: maintain a per-(e, a) module-shadow stack so restoration is O(1) instead of a history walk. Pick implementation based on workload patterns observed in Phase 5+.

5.5.3 Tests: A asserts, B supersedes, unload B → A's value visible; chains of 3+ modules; module unload order independence.

### Reasoning
Phase 5 ships simple semantics: unload retracts; superseded values are gone for good. Real dev workflows may demand restoration so that "load A, load B (shadows A), unload B" returns to A's view — matching mental model of stacking patches. Defer until pain shows up. Likely interacts with schema (Phase 9.5) — a per-attribute `:retain-on-unload` flag could opt in.

---

## Phase 6 — REPL Module

**Goal:** REPL is an object, not a shell loop. Proves the substrate by making the user-facing front-end a fact-shaped module.

### Work items

6.1 Minimum surface syntax: bare selector (unary), keyword-with-arg (single keyword), `quit`/EOF, `target: <eid>`. Arg literals: integer → SmallInt; identifier → interned-selector EID; quoted → string Ref. No precedence, no cascades, no blocks. Full syntax is Phase 10.

6.2 Tiny hand-rolled Zig parser inside the repl module (`src/modules/repl/parser.zig`). Intentional throwaway — replaced when Phase 10 language module ships.

6.3 REPL prototype with `run`, `setPrompt:`, `setTarget:`, `stop` methods, materialized struct with `prompt`/`target`/`running` slots (REPL is hot — promote rather than EAVT-look-up per keypress).

6.4 Shell hands every input line to the REPL module via the existing `set_repl_handler` slot. Shell's main loop becomes a thin reader.

6.5 Extend `hello` (or add `repl_test` dylib) with a counter object: `inc` / `count` methods, mutable slot. Provides something the REPL session can actually exercise.

6.6 Tests: unary send, keyword send, target switch, EOF exit, unknown-selector graceful error, scripted multi-line stdin pipe.

### Open-question resolutions (Phase 6 design)

- **Result printing**: minimal `Ref → string` printer handling SmallInt, EID, string, NIL, pointer-shows-as-`<obj #eid>`. Improve incrementally; no specific later phase pinned.
- **Unknown selector**: print `unknown selector: X` and continue. Proper `doesNotUnderstand:` lands in Phase 7+ when reflection is richer.
- **Line editing / history**: skip in Phase 6 (naked stdin only). Add libtecla or similar when REPL is the primary interface — likely as part of Phase 10's user-experience polish.
- **Multi-line input** (blocks, methods): single-line only in Phase 6. Multi-line arrives with the real language in Phase 10.

### Reasoning
REPL-as-object is a litmus test for the substrate. If REPL works as a real fact-shaped object with mutable methods, the rest of the user-facing system follows the same template. Hand-rolled Zig parser is intentional throwaway — replaced when the language module is built atop birnam itself.

---

## Phase 7 — Datalog Core (Relational + Negation + Disjunction + History)

**Goal:** Ship the relational core of birnam's Datalog system. ~80% of reflection queries become expressible. See `DATALOG.md` for full semantics.

### Work items

7.1 Decide Phase 7 open questions (DATALOG.md §8.1): variable-index assignment timing, clause encoding in IdApi (lean: opaque builder), result-Ref identity (lean: fresh per execution), inequality types (lean: SmallInts), history-of-everything modifier (lean: separate `historyOf:`), MEAT-scoped query syntax (lean: `inModule:`).

7.2 `src/kernel/query.zig`: Query / Term / Clause types per DATALOG.md §2.1-§2.6. Builder API. Index selection table per §2.9. Naive nested-loop join engine per §2.7-§2.8.

7.3 `src/kernel/result.zig`: materialized RRB-of-Refs result type. Multi-var tuple wrapper.

7.4 IdApi extensions: query-builder fns (make_query, query_add_*), execute, count, first. Plus result iteration helpers.

7.5 `src/modules/query/query.zig`: `db` singleton with `findAll:where:`, `find:where:`, `count:where:`, `findAll:where:asOf:`, `findAll:where:since:`, `findAll:where:inModule:`. Bracket-array clause parser (extended from Phase 6 REPL parser).

7.6 Tests per DATALOG.md §2.14: identity, multi-var, multi-clause join, negation (vtables without init), disjunction (named-Point-OR-Vector), predicate (age < 30), history (asOf restored prior value), set semantics, unsafe-negation rejection, mismatched-disjunction rejection.

### Reasoning
Datalog is as central as the object model — it's how birnam introspects itself. Phase 7 delivers Datomic-level expressiveness short of rules, recursion, and aggregation. Without negation and disjunction, the query language is a toy; with them, ~80% of useful queries work. Rules/recursion (Phase 7.1) and aggregation (Phase 7.2) layer on additively without redesign.

---

## Phase 7.1 — Rules + Recursion

**Goal:** Add rule definitions, recursive evaluation, stratified-negation check. Datalog proper.

### Work items

7.1.0 **Phase 7 polish prologue.** Two items deferred from Phase 7 ship:
  - Wire `since:` and `between:` modifier execution in `query.execute` (mirror the existing asOf code paths).
  - Add IdApi builders for negation and disjunction clauses. Reuses the nested-clause ABI design needed for rule bodies in 7.1.4 — solve once. Likely shape: `query_begin_negation(*Query)` / `query_end_negation(*Query)` plus `query_begin_disjunction(*Query)` / `query_begin_branch(*Query)` / `query_end_branch(*Query)` / `query_end_disjunction(*Query)`. Internal builder maintains a stack of in-progress clause-lists.

7.1.1 Open questions per DATALOG.md §8.2: rule definition syntax (lean: assert-and-name), empty-match semantics (lean: silent empty), stratification timing (lean: lazy at query-execute).

7.1.2 `src/kernel/rule.zig`: Rule type + kernel-side registry. **Phase 7.1 stores rules in a HAMT registry** keyed by rule-name EID; the body lives as opaque Zig data (nested `Clause` structures), not as facts. The `[rule_eid :rule-name "..."]` fact provides ownership and reflection. Multiple rules per name = definitional disjunction, Datomic-style. **This is intentionally non-fact-shaped storage**; pure fact representation is restored in 7.3.7 after reification lands and clauses become addressable as fact entities.

7.1.3 Rule-invocation clause type added to Clause union. Planner walks rule-dependency graph, runs semi-naive fixpoint.

7.1.4 Stratification check at query execution: build rule-dependency graph annotated with negative edges; reject programs with negative cycles.

7.1.5 Termination guards: max-iteration / max-derived-tuple safety bounds. User-configurable; default 10^6.

7.1.6 Tests per DATALOG.md §3.6: transitive closure (ancestor), delegation-chain walk, mutual recursion (even/odd), stratification rejection, safety-bound trip.

### Reasoning
Recursion makes Datalog Datalog. Transitive properties (delegation chains, dependency graphs, reachability) become one-line queries instead of hand-coded walks. Stratified-negation check protects users from undecidable programs.

---

## Phase 7.2 — Aggregation

**Goal:** Aggregate functions in find clauses with implicit group-by. Datomic-style.

### Work items

7.2.1 Open questions per DATALOG.md §8.3: distinct-vs-bag aggregation (lean: distinct matches set semantics), type strictness (lean: error on type mismatch).

7.2.2 `src/kernel/aggregate.zig`: aggregate function registry. Built-ins: count, count-distinct, sum, min, max, avg, collect, sample.

7.2.3 Find-element extension: `AggregateSpec` alongside `var_idx`. Parser recognizes `(count ?m)` form in find list.

7.2.4 Group-by engine: post-process binding-set into partitions keyed by non-aggregated find vars. Apply aggregate per partition.

7.2.5 Tests per DATALOG.md §4.5: count by group, sum, max-tx, collect-into-RRB, multi-aggregate per group.

### Reasoning
Without aggregation, analytical queries (counts, sums, top-N) drop back to imperative code. Implicit group-by matches Datomic and feels natural in birnam.

---

## Phase 7.3 — Subqueries + Saved Queries + Pull

**Goal:** Compose queries. Persist named queries. Return hierarchical results.

### Work items

7.3.1 Open questions per DATALOG.md §8.4: pull recursion bound (lean: explicit `...:` marker + depth limit), saved-query args (lean: positional), subquery result shape (lean: multi-var via tuples).

7.3.2 `src/kernel/saved_query.zig`: Query stored as fact-entity. Name → query lookup. Resolve at call site. Module ownership (unload retracts saved queries).

7.3.3 Subquery support: clause references another query's result, fed in as virtual table.

7.3.4 `src/kernel/pull.zig`: pull-spec parser, recursive tree walker, materialized-map output. Recursion bounded by spec syntax.

7.3.5 Methods on db: `run: <name>`, `run: <name> withArgs: <vec>`, `runQuery: <q>`, plus pull spec in find clause.

7.3.6 Tests per DATALOG.md §5.5: saved-query round-trip, subquery pipeline, pull with flat + recursive specs.

7.3.7 **Fact reification + per-triple module filtering + inModule MEAT optimization + rule storage walk-back.** Phase 7 ships query-wide `inModule:` only, implemented as a linear scan of `factsOfModule` per fact check (O(N) per triple). Phase 7.1 stores rule bodies in a kernel-side Zig registry rather than as facts. By Phase 7.3 the saved-query / subquery work has cleared the conceptual ground for facts-as-entities (VISION.md §Reification gap explicitly defers this as additive). Take all four on here:
  - Promote `m` column from tuple field to a regular fact about the fact entity: `[fact_eid :module <module-eid>]`.
  - Each triple clause may add modifier clauses operating on the fact entity itself.
  - Surface: `[?vt :name ?n :as ?fact][?fact :module <module-eid>]` — reified fact bound to a variable, then queried as any other entity.
  - This subsumes per-triple module filtering and opens fact-level metadata (provenance, justification, belief markers) as future feature.
  - Migration: existing 6-column tuples continue to work; reification is opt-in via the `:as` modifier.
  - **Rule storage walk-back**: with clauses now addressable as fact entities, rule bodies move from kernel-side Zig structures (Phase 7.1's compromise) to fact-shaped storage. `[rule_eid :rule-body <body-clause-fact-eid>]` with body clauses themselves stored as facts. Restores the "everything is a fact" tenet.

### Reasoning
Composition is what turns queries from one-off into building blocks. Pull syntax matches the way users want to read graph data (hierarchical, not flat). Fact reification (7.3.7) preserves relational-model expressiveness — without it, the `m` column is special-case storage that resists the same query mechanisms applied to every other attribute. Date discipline holds: reification is additive, breaks no existing query.

---

## Phase 7.4 — Predicates + Functions

**Goal:** Inline custom predicates and computed bindings. Round out Datalog expressivity.

### Work items

7.4.1 Open questions per DATALOG.md §8.5: predicate ownership (lean: facts, module-tagged), purity convention (lean: convention only), predicate composition (lean: no formal termination check).

7.4.0 **Decidability: forbid recursion through function clauses.** Function clauses `[(fn ?args) ?target]` introduce new constants not present in any fact, breaking Datalog's Herbrand-universe boundedness. Without restriction, recursive rules over function clauses are non-terminating in general (`ancestor(Y) :- ancestor(X), [(+ X 1) Y]` runs forever). Implement Datomic-precedent: static check at rule-define time rejects rules whose body has both a rule-invocation AND a function-clause where the function output flows (transitively) into the rule-invocation arg. Phase 7.1's termination bounds remain as a backstop for very-large but technically-finite computations.

7.4.2 **Reopen Phase 7 Q4**: inequality types. Phase 7 shipped SmallInt-only `<`/`<=`/`>`/`>=`. By Phase 7.4 the predicate framework is in place; decide then whether to add tag-aware total order (string lex, EID numeric, etc.) as additional built-in predicates rather than overloading the comparison operators. Likely: built-in predicates `stringLessThan`, `eidLessThan`, etc. — explicit, type-tagged, no operator overloading.

7.4.2 `src/kernel/predicate.zig`: predicate registry. User predicates registered as facts with closure refs. Built-in arithmetic + comparison + string fns pre-registered.

7.4.3 Predicate clause: `[(pred-name ?args...)]` form. Caller binds args, predicate returns truth.

7.4.4 Function clause: `[(fn-name ?args...) ?target]` form. Computes target binding from inputs.

7.4.5 Tests per DATALOG.md §6.3: user predicate hit/miss, arithmetic chains, predicate over function result.

### Reasoning
Final piece of Datalog parity. Custom predicates let users embed domain logic in queries; function expressions remove need to materialize intermediate facts just to compose values.

---

## Phase 7.5 — Metacircular Bind

**Goal:** Allow `bind` to be redefined from birnam code, closing the metacircular hole left open by Phase 4.

### Work items

7.5.1 Cold-path bind checks EAVT for `[vt_eid :bind ?method]` before falling through to the Zig primitive. If a method is installed, use it.

7.5.2 Bind-of-bind: the override method itself eventually calls `lookup:`/`bind`. The recursion guard (already scaffolded in Phase 4) kicks in here — when guard > 0, bind unconditionally takes the Zig-primitive path.

7.5.3 Default `:bind` method on `vtable_vt`: a birnam stub that delegates to the primitive. Lets users wrap it (call super + extra logic) rather than replace it whole.

7.5.4 Mcache invalidation extended to flush on `(?, :bind, ?)` assert/retract — touching bind invalidates everything.

7.5.5 Tests: install a no-op `:bind` override; install a logging override; install a broken override that triggers the recursion guard; restore primitive via retract.

### Reasoning
The metacircular property is the soul of COLA/idst. Phase 4's primitive-only bind is a deliberate simplification. By Phase 7.5, we have queries (Phase 7) for introspection and Phase 5/6 modules for staging the override. Metacircular bind becomes the first real exercise of "rewrite the kernel from birnam." Acceptable risk of breaking dispatch is bounded by the recursion guard and easy retract-to-restore.

---

## Phase 8 — Promotion / Demotion

**Goal:** JIT-style materialization layer.

### Work items

8.0 **Reopen open question**: heuristic shape (kernel-tracked counter vs. policy-module-owned bookkeeping). Phase 4 shipped manual `promote`/`demote` only. By now Phase 7 (query) and Phase 10 (language) have produced real cold-dispatch traces. Decide with data in hand.

8.1 Manual `promote(eid)` and `demote(eid)` primitives first (already in IdApi from Phase 4). Allocate materialized struct from fact-set; free struct, leave facts.

8.2 Shape detector: walk EAVT for an entity, derive struct layout deterministically. Cache layouts by attribute-set.

8.3 Hotness counter on per-EID basis (incremented on send-with-cold-ref). Threshold-driven auto-promote.

8.4 Demotion triggers: shape mutation (new attribute asserted/retracted), explicit, snapshot save, memory pressure.

8.5 Tests: round-trip promotion preserves identity and behavior; mcache invalidates on demote; FFI auto-promote (later).

### Reasoning
Until promotion is working, every dispatch goes through index query (~30ns). Acceptable for correctness phase, painful for real workloads. Defer until correctness is proven; then add promotion to recover hot-path performance.

---

## Phase 9 — Alien `.so` Absorption

**Goal:** Iteratively wrap arbitrary shared libraries from the REPL. macOS-only first pass; linux follows in Phase 9.5.

### Decisions locked (Phase 9 decision doc, 2026-05-29)

| Q | Decision |
|---|---|
| Q1 FFI lib | **dyncall** vendored as static lib (~3k LOC C, BSD-licensed). Fallback: **typed-dispatch table** (Zig-comptime wrappers per (arg-kinds, return-kind) tuple) if dyncall vendor causes build pain or arm64 darwin variadic ABI breaks. Fallback is platform-portable but bounded to enumerated sigs; covers Phase 9 test plan (sqrt, strlen, printf) trivially. |
| Q2 Float repr | **Boxed f64** via new `FLOAT_VT` on pointer-tagged object |
| Q3 C pointer | **Boxed FFI pointer** via new `FFI_PTR_VT` (raw addr in slot 0; metadata via facts) |
| Q4 Variadics | **Constrained per-wrap shape** (sig-string + `:ffi-variadic?` flag). Fall back to **punt** if dyncall arm64 darwin ellipsis is broken |
| Q5 Symbol enum | **Hybrid**: dlsym for wrap-by-name; lazy fact-materialization on first `Module>>symbols` send; `:ffi-symbols-enumerated?` flag caches |
| Q6 Unload | **Deferred to Phase 9.5**: dlopen handle held forever; process exit cleans up |
| Q7 Platform | **macOS-only** Phase 9 (Mach-O reader). ELF + linux gate flip → Phase 9.5 |

### Documented risks

1. **dyncall arm64 darwin variadic ABI.** Tested in 9.7 via printf. If broken, demote Q4 to punt-all-variadics, document in commit, ship fixed-arity only.
2. **macOS code-signing / hardened runtime.** System libs (libSystem) fine; custom test `.dylib` fixtures may need `codesign -s -`. Address if hit.
3. **Long-running image leak.** No unload = leak across many `Module load:` cycles. Acceptable until 9.5.
4. **Boxed float / FFI-ptr alloc churn.** Invisible at Phase 9 scope. Revisit only if Phase 11+ profiling shows hot-loop boxing.

### Work items

9.0 Vendor dyncall (`vendor/dyncall/`); wire static lib into `build.zig`.

9.1 `src/kernel/macho.zig` (~300 LOC): magic-bytes check, parse `LC_SYMTAB`, walk `nlist_64`, yield `(name, addr, kind)` iterator.

9.2 New ref types: `mkFloat` / `untagFloat` / `isFloat` via pointer-boxed f64 (`FLOAT_VT`); `mkFfiPtr` / `untagFfiPtr` / `isFfiPtr` via pointer-boxed addr (`FFI_PTR_VT`). Bootstrap allocates both vts.

9.3 `Module load:` (in `src/modules/ffi/` or extension to query module): dlopen path; assert `[mod :ffi-handle <ptr-Ref> :ffi-path <str>]`. `Module>>wrap: sel as: name signature: sig`: dlsym + sig-parse + fact assertion.

9.4 Sig-string parser. Tokens per dyncall convention (`i`, `l`, `d`, `Z`, `p`, `)`, `e` or `...` for variadic). Output = arg-type array + return-type. Stored as facts for walk-back.

9.5 Dispatch shim: when wrap selector dispatched, materialize tiny closure that marshals Refs → dyncall types, calls `dcCallX`, marshals return → Ref. Cached per wrap. Bracketed with `promotion.ffiEnter()` / `ffiExit()` (Phase 8 primitive).

9.6 `Module>>symbols`: first call walks Mach-O sym table via 9.1, asserts `[mod :ffi-has-symbol <sym>]` per entry; sets `:ffi-symbols-enumerated?`. Subsequent calls = query.

9.7 Tests: libm `sqrt` (float in/out); libSystem `strlen` (Z→i fixed); libSystem `printf` (variadic shape `"Zi)i"`). Skip on non-darwin via `builtin.os.tag` gate. If printf fails on arm64, demote per Q4.

### Reasoning
COLA dream made concrete: interactively absorb any C library into the object world. dyncall is well-trodden; Mach-O reader is bounded (~300 LOC). FFI pin (Phase 8) prevents demotion mid-call. Boxed-on-pointer for Float and FFI-pointer keeps Ref tag-space intact for future immediates.

---

## Phase 9.5 — Module Wrapper + Multi-Platform FFI + Schema + Unload

**Goal:** Land the Module entity wrapper deferred from Phase 9, ELF reader for linux, FFI unload semantics, and schema-driven multi-cardinality attributes. Bundled because schema lands close to Phase 10 language work.

### Open questions (DRAFT — decision doc not yet locked)

Four sub-bundles, each with its own design questions. Tradeoffs + leans to be locked in a follow-up table before code.

#### A. Module entity wrapper

A1. **Module vt structure.** One shared `MODULE_VT` for all loaded libs, OR per-instance delegate-vt (child of `MODULE_VT`) so wrap installs are scoped to the specific Module?

A2. **API surface.** Minimum: `Module load:`, `Module>>wrap:as:sig:`, `Module>>symbols`. Optional: `path`, `handle`, `unload`. What ships in 9.5?

A3. **Wrap target vt.** Wrap installs the dispatch closure on the Module's own vt only (`[mod sqrt:]` works) OR a `wrap:as:sig:onVt:` variant lets the user extend any class.

#### B. ELF reader

B1. **Symbol-table source.** `.symtab` (debug-only, often stripped from prod libs) vs `.dynsym` (always present in shared libs; matches what dlsym sees).

B2. **Reader unification.** Mach-O `Reader.symbols()` returns a typed iterator. Define a shared `Symbol` + iter type with a `loader.zig` magic-byte dispatcher so callers stay format-agnostic.

#### C. Module unload

C1. **Pointer-ownership tracking.** Need `[ffi_ptr_obj :ffi-owner <mod_eid>]` on every boxed FFI pointer to know what to invalidate at unload. Other approaches?

C2. **Stale-flag check location.** Check `:ffi-stale?` once at `pushArg`-pointer time (per-call cost, simple) vs inside `untagFfiPtr` (every deref) vs a debug-only assert.

C3. **Refuse-unload criterion.** `ffi_depth > 0` alone (Phase 8 primitive), OR `ffi_depth > 0` OR any live-boxed-ptr-from-mod still around.

#### D. Schema / multi-cardinality

D1. **Default cardinality.** Absence of `:cardinality` fact = `:one` (backward-compat) vs explicit `:one` required everywhere.

D2. **`currentValue` semantics for `:many` attrs.** Returns latest (existing single-value callers unchanged) vs returns NIL vs errors. Plus new `currentValues(e, a)` iterator either way.

D3. **Retroactive schema change.** Asserting `:cardinality :many` on an existing single-value attribute: does it convert past asserts? Or forward-only?

D4. **Scope creep.** Phase 9.5 also adds `:unique`, `:index?`, other Datomic-style schema attrs? Or cardinality only.

#### E. Cross-bundle build order

E1. Module wrapper depends on Mach-O reader (have). ELF reader independent. Unload depends on Module wrapper + per-ptr ownership. Schema fully independent.
Proposed order: **Module → Schema → ELF → Unload.** Or interleave?

### Work items (placeholder — to be expanded after decision doc lock)

9.5.1 Module wrapper (A1, A2, A3) — new `src/modules/ffi/` dylib or kernel-side `Module` entity.

9.5.2 Schema cardinality (D1, D2, D3, D4).

9.5.3 ELF reader + `loader.zig` dispatcher (B1, B2).

9.5.4 CI gate flip: linux FFI tests turn on; Mach-O tests skip-gated to darwin.

9.5.5 Module unload (C1, C2, C3) — `Module>>unload` + stale-pointer flag + deref-check.

### Reasoning
Module wrapper turns Phase 9's flat `registerWrap` primitive into a send-dispatchable surface (`[Module load: "path"]`, `[mod wrap: #sqrt: as: "sqrt" signature: "d)d"]`). Schema lands here because Phase 10's user-defined types want `[parent :children ?]` semantics. ELF + unload finish what Phase 9 deferred for spike scope. Bundled phase keeps the platform-parity story coherent.

## Phase 10 — Language Module

**Goal:** Birnam source syntax, parsed into facts, evaluated against fact store.

### Work items

10.1 Reuse existing PEG module for grammar definition (already fact-shaped after Phase 5).

10.2 Birnam grammar (Smalltalk-ish): unary, binary, keyword messages; cascades; blocks; literals; assignments. PEG rules asserted as facts.

10.3 Parser run: source string → AST nodes asserted as facts under a per-parse EID.

10.4 Evaluator: walks AST facts, asserts/queries at each node. Simple, slow, correct first.

10.5 Method definition syntax: source → method body facts → installed as `assert_method`.

10.6 Tests: classic Smalltalk-y exercises (Fibonacci, list operations, point class definition).

### Reasoning
Once the language module works, the system is self-hosting in the sense that future modules can be written in birnam rather than Zig. Tree-walking interpreter is intentionally simple; bytecode + VM come later in Phase 11.

---

## Phase 11 — Bytecode + VM

**Goal:** Compile AST facts → bytecode facts; execute via VM.

### Work items

11.1 Bytecode design: stack-based, ~20 opcodes (push, pop, send, return, jump, branch, load_local, store_local, push_const, etc.). Each opcode is an entity.

11.2 Compiler: birnam module that walks AST facts, emits bytecode facts.

11.3 VM: Zig primitive interpreter (initial), can be replaced by a birnam module later. Threaded-code dispatch on hot path.

11.4 Inline caching at call sites in bytecode (per-pc mcache).

11.5 Tests: VM matches tree-walker output on the Phase 10 test suite.

### Reasoning
VM gives us portability and AOT. Bytecode-as-facts means the compiler is also queryable and diffable — we get a "what changed in the compiled form?" view for free.

---

## Phase 12 — Image Snapshot

**Goal:** Save and reload entire image state.

### Work items

12.1 Image format: directory with `roots.bin` (index roots + tx counter + selector table head) + `segments/` (arena segments mmappable).

12.2 Snapshot writer: walk persistent structures, identify dirty segments since last snapshot, write only those.

12.3 Snapshot loader: mmap segments, restore roots, rebuild mcache lazily (empty).

12.4 Selector identity across reload: kernel selectors use well-known EIDs; user selectors re-intern via relocation table.

12.5 **Arena split for strings.** Phase 4.5 ships strings via the kernel allocator. Split here: strings get a dedicated arena → separate mmap segment, faster image load, page-cache-friendly, easy memory accounting, and (in AOT) a read-only data segment. No runtime perf change — pure lifecycle win.

12.6 Tests: save → kill → load → run → identical behavior; verify strings segment is mmap-able and survives reload.

### Reasoning
Image persistence is what makes birnam a real Smalltalk-style environment vs. a script that has to rebuild state every run. Structural sharing in the persistent structures makes incremental snapshots cheap.

---

## Phase 13 — Pruning

**Goal:** History excise.

### Work items

13.1 Pruning policies: keep-last-N-txs, reachable-from-roots, manual-excise-of-named-entities.

13.2 Pruning runs as a separate tx that rewrites indexes excluding pruned facts. Old index roots become unreferenced and GC-able.

13.3 Tests: pruning preserves reachable state, removes excised entities, query results match pre-prune for non-excised data.

### Reasoning
History grows forever otherwise. Pruning must be user-initiated (don't surprise the user with lost history). AOT prune (most aggressive variant) reuses this machinery.

---

## Phase 14 — AOT

**Goal:** Ship standalone binaries.

### Work items

14.1 AOT compiler: walk live image, emit shell + VM + frozen image as a single executable.

14.2 Strip history attribute, demote all objects to facts (or pre-materialize all and freeze — decide per-AOT-mode).

14.3 Freeze indexes into read-only segments embedded in the binary.

14.4 Tests: AOT'd binary runs without compiler, without history, with same observable behavior on a test workload.

### Reasoning
This is the deliverable that takes birnam from "interesting toy" to "real systems language." If we can emit `<app>` as a stripped binary, then we can ship things written in birnam.

---

## Phase 15 — Native Codegen (Later)

**Goal:** Lower IR facts to machine code.

### Work items

15.1 Choose lowering target: hand-rolled emitter, QBE, or LLVM. Default: hand-rolled for x86_64 + arm64. Fall back to QBE if time-constrained.

15.2 Lowering passes: AST → SSA-ish IR → machine code. Each pass is a birnam module.

15.3 Replace bytecode VM with native code for hot methods.

### Reasoning
Native codegen is performance polish, not a requirement. Smalltalk-VM ballpark (which we get from bytecode + inline caching) is enough for self-hosting. Defer until everything else works.

---

## Phase 16 — GC (Later)

**Goal:** Reclaim materialized-struct memory.

### Work items

16.1 Mark materialized arena reachability from object table + currently-pinned (FFI) refs.

16.2 Sweep unreachable structs; demote them (facts already preserve identity).

16.3 Trigger heuristic: arena fill threshold.

### Reasoning
Persistent structures + pruning replace most GC pressure. Materialized arena is the one place we still allocate freely. Demotion is the natural reclaim path — facts are the canonical form, the struct is just cache.

---

## Dependency graph

```
Phase 0  (demolition)
   ↓
Phase 1  (HAMT + RRB)
   ↓
Phase 2  (tagged pointers + object table) ──┐
   ↓                                         │
Phase 3  (fact store + 4 indexes) ───────────┤
   ↓                                         │
Phase 4  (id kernel rewrite) ←───────────────┘
   ↓
Phase 5  (module system) ────┐
   ↓                          ↓
Phase 6  (REPL)         Phase 7  (query)
   ↓                          ↓
   └──────────┬───────────────┘
              ↓
         Phase 8  (promotion/demotion)
              ↓
         Phase 9  (alien .so)
              ↓
         Phase 10 (language)
              ↓
         Phase 11 (bytecode + VM)
              ↓
         Phase 12 (image snapshot)
              ↓
         Phase 13 (pruning)
              ↓
         Phase 14 (AOT)
              ↓
         Phase 15 (native codegen) — optional polish
         Phase 16 (GC) — optional polish
```

## Rough size estimate

Frozen kernel (Zig) at completion of Phase 4: ~1500 LOC.
Frozen kernel at completion of Phase 14: ~3000 LOC (adds VM interpreter, AOT emitter, Mach-O/ELF readers).
Birnam-side (fact-shaped, mutable): unbounded — that's the whole point.
