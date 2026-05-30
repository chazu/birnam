# Birnam Roadmap

Detailed work sequence from current zig/birnam (linear-scan vtables, no fact store) to the locked vision (relational substrate, tagged pointers, HAMT+RRB everywhere). Reasoning per phase.

This roadmap follows VISION.md. Open questions in VISION.md must be resolved before starting Phase 1.

## Status (as of 2026-05-30)

**Completed (substrate):**
- Phase 0–7.5 ✓ (collections, fact store, kernel, datalog, FFI, modules)
- Phase 9 / 9.5 ✓ (boxed Float + FFI ptr, dyncall, Mach-O / ELF readers, Module wrapper, schema cardinality, unload)

**Completed (Phase 10 — language):**
- Phase 10.0 ✓ (PEG engine + send-dispatched grammar surface)
- Phase 10.1.a ✓ (kernel slot block — 62 EIDs)
- Phase 10.1.b ✓ (real birnam grammar — ~25 rules)
- Phase 10.2.5 ✓ (variadic sendN — ABI-deep β refactor, ADR-10-Send-Arity RESOLVED)
- Phase 10.2 ✓ (desugar — CST → typed AST with kind-vts)
- Phase 10.3 minimal ✓ (eval methods on AST kind-vts, SmallInt primitives)
- Phase 10.4 ✓ (method install via `[vt method:#sel do:[block]]`, block params, compound symbols)
- Phase 10.5 ✓ (REPL — lang dylib drives stdin)
- Phase 10.x ✓ (binary ops `+ - * / < > =`, `ifTrue:ifFalse:`, `[block value]`)
- Phase 10.4.5 S1-S4 ✓ (fact-shaped SCOPE_VT / BINDING_VT entities, closure capture by reference, `let`, `:=` assignment, REPL persistence — ADR-10-E2 truth layer RESOLVED)
- Iteration primitives ✓ (`whileTrue:` / `whileFalse:` on CLOSURE_VT; `timesRepeat:` / `to:do:` on SMALLINT_VT — all Zig-native per 10.0.x lock, user-overrideable)
- `;` statement separator + multi-stmt block bodies via AST_STMT_SEQ_VT ✓
- Line comments `// ...` + block comments `/* ... */` ✓
- Float literals `3.14` ✓ (`make_float` IdApi extension; AST_LITERAL_VT carries boxed Float Ref)
- Non-local return `^expr` ✓ (ADR-10-E4 `g_unwinding` interim — first non-value-selector shim frame catches, value-frame shims propagate; Phase 11 retires per 11.5)

**Test count: 290/290 across 73 build steps.**

**Demoable today (live REPL via `./zig-out/bin/birnam libbirnam_peg libbirnam_lang`):**
- Arithmetic + comparisons + branching
- `let` bindings + `:=` mutation
- User-defined methods via `[vt method:#sel do:[block]]`
- Closures-over-enclosing-scope (`makeAdder := [:a | [:b | [a + b]]]`)
- REPL persistence across lines

**Deferred (not blocking demo):**
- Phase 10.4.5 S5 — parse-time slot resolution (speed-layer perf optimization)
- Floats with exponent notation (`1.5e10`); current grammar covers `digit+ "." digit+` only

**Phase 10 substantially closed.** Remaining items are perf polish (S5) or grammar nicities (float exponents). The language now has: arithmetic, comparisons, control flow, blocks with params, closures-over-enclosing-scope, mutation, recursion via method install, iteration primitives, statement sequencing, comments, floats, and non-local return.

**Next milestones (pick by priority):**
- Phase 10.4.5 S5 — optional perf optimization
- Phase 11 — bytecode VM (retires `g_unwinding` for non-local return, retires Zig-native iteration prims)
- Phase 12 — image snapshot + load (makes startup fast + preserves state across runs)

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

### Decisions locked (Phase 9.5 decision doc, 2026-05-29)

> **Terminology note:** birnam is strictly prototype-based (COLA / idst /
> Piumarta lineage per VISION.md). There are objects (with `vt` at offset
> `-1`) and vtables (themselves objects, themselves with vts). No class /
> instance split — a vtable IS a prototype, delegation via `:delegate`
> chain replaces inheritance. Whenever Phase 9.5 discussions say "Module
> vt" they mean the prototype-vtable that loaded-library objects delegate
> to, not a Smalltalk-80 metaclass.

| Q | Decision |
|---|---|
| A1 Module vt structure | **Per-Module delegate vt.** Each `load:` mints a fresh vt that delegates to `MODULE_VT`. Wraps scoped to instance. |
| A2a load: failure | **Error-entity Ref** with `:error-kind :ffi-load-failure :error-message <str> :error-path <str>` (Phase 7.8 diagnostic pattern). |
| A2b wrap: failure | **Return receiver on success, error-entity on failure.** Kinds: `:ffi-arity-too-high`, `:ffi-sig-parse-failure`, `:ffi-symbol-not-found`. Chainable on success. |
| A2c symbols return | **RRB of symbol-entity Refs.** Lazy fact-materialization on first send; `:ffi-symbols-enumerated?` flag caches. New EIDs: ATTR_FFI_SYMBOL_NAME/ADDR/KIND/EXTERNAL/MODULE, ATTR_FFI_SYMBOLS_ENUMERATED. |
| A2d path / handle | **Boxed types via fact-storage queries.** path = boxed string Ref via `[mod :ffi-path …]`; handle = boxed FfiPtr via `[mod :ffi-handle …]`. |
| A3 wrap target vt | **Module-scoped only.** `wrap:as:sig:` installs on receiver's instance vt. `onVt:` variant deferred (needs send4 + receiver-as-arg coercion convention). |
| B1 ELF symbol table | **`.dynsym`** — always present, matches dlsym view. Mach-O reader also aligned to drop N_STAB so both readers expose only callable symbols. |
| B2 Reader unification | **Fact-dispatched registry.** Each reader self-registers at boot: `[reader_eid :reader-magic <SmallInt>] [reader_eid :reader-impl <fn-ptr-Ref>]`. Shared `loader.Symbol` + `loader.SymbolIter` data contract. New file `src/kernel/loader.zig` (~30 LOC dispatcher). COLA-correct: no hardcoded switch. |
| C1 Pointer ownership | **`:ffi-owner` fact** per boxed FFI pointer via `boxed.mkFfiPtrOwned(allocator, addr, owner_mod) → Ref` variant. AVET indexes mod→ptrs lookup. New EID ATTR_FFI_OWNER. |
| C2 Stale-flag check | **At pushArg time** when a Ref crosses into a C call. Failure returns error-entity with `:error-kind :ffi-stale-pointer`. New EID ATTR_FFI_STALE. |
| C3 Refuse-unload | **`ffi_depth > 0` only** (Phase 8 primitive). C2 stale-flag protects post-unload use. |
| D1 Default cardinality | **Absence of `:cardinality` fact = `:one`.** Zero-migration; assert hot path early-outs when no cardinality fact exists. |
| D2 currentValue for :many | **Polymorphic Ref return.** See ADR-9.5-D2 below: `:one` returns value Ref unchanged; `:many` returns RRB-Ref materialized from atomic per-tuple storage (Datomic-style multi-datom); empty `:many` returns interned empty-RRB constant; NIL = never-asserted. 1NF preserved at storage layer. |
| D3 Retroactive schema | **Forward-only.** Schema change applies to future asserts only. Past history immutable (Date discipline). Storage already supports mixed-era datoms via multi-datom shape. |
| D4 Schema scope | **Cardinality only for Phase 9.5.** `:unique`, `:no-history` deferred to Phase 11.x with their own decision rounds. `:doc` works today without kernel changes. `:index?`/`:value-type` redundant in birnam's substrate. |
| E1 Build order | **Module → Schema → Module symbols → loader/ELF → Unload.** Module ships earliest user-visible win; Schema settles substrate before symbols (uses `:many`) and unload (uses `currentValues`). |

### Documented risks

1. **`:many` read-path allocation.** Every `currentValue` on a `:many` attr allocates one RRB head. Hot reads = churn. Spike accepts; intern by (e, a, latest-tx) later if profiling shows it matters.
2. **Mixed-era datoms.** An attr flipped from `:one` to `:many` mid-life will have history with both implicit-retract-on-overwrite (`:one`-era) and append-without-retract (`:many`-era) shapes. Both correct in their era. Documented for users.
3. **macOS shared cache.** Mach-O reader can't read libSystem from disk on Big Sur+; symbol enumeration tests use a build-fixture dylib (already in Phase 9 pattern).
4. **Schema migration ergonomics.** Forward-only cardinality means converting old single-value history to `:many` semantics requires a user-side migration script (assert new datoms explicitly). Substrate enforces nothing.



#### A. Module entity wrapper

A1. **Module vt structure.** One shared `MODULE_VT` for all loaded libs, OR per-instance delegate-vt (child of `MODULE_VT`) so wrap installs are scoped to the specific Module?

A2. **API surface.** Minimum: `Module load:`, `Module>>wrap:as:sig:`, `Module>>symbols`. Optional: `path`, `handle`, `unload`. What ships in 9.5?

A3. **Wrap target vt.** Wrap installs the dispatch closure on the Module's own vt only (`[mod sqrt:]` works) OR a `wrap:as:sig:onVt:` variant lets the user install on any existing vt (extending an existing prototype).

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

> **ADR-9.5-D2 (2026-05-29).** Locked: `currentValue(e, a)` returns a
> polymorphic Ref. `:one` returns the value Ref (Phase 0-9 behavior
> unchanged). `:many` returns an RRB-Ref materialized from multiple
> stored `(v, tx, op)` entries — **storage stays atomic per-tuple**
> (Datomic-style multi-datom), materialization is a derived view at
> read time. Empty `:many` returns an interned empty-RRB constant
> (distinguishable from NIL = never-asserted). Caller discriminates via
> `isRrbRef(r)`.
>
> **Date 1NF preserved** because storage is atomic tuples, not
> RRB-in-cell. **COLA-compliant** because dispatch is via Ref tag
> (already-used substrate pattern), cardinality is itself a fact
> (`[attr :cardinality :many]`) introspectable from REPL — no hidden
> Zig-side switch. **NIL admissible** because birnam NIL = the
> nothing-entity at well-known EID 0 (a real domain value), not
> SQL-NULL three-valued-logic absence.
>
> Cost accepted: one RRB head allocation per `:many` read in the spike.
> Optimization (intern per (e, a, latest-tx)) deferred until profiling
> shows it matters.

D3. **Retroactive schema change.** Asserting `:cardinality :many` on an existing single-value attribute: does it convert past asserts? Or forward-only?

D4. **Scope creep.** Phase 9.5 also adds `:unique`, `:index?`, other Datomic-style schema attrs? Or cardinality only.

#### E. Cross-bundle build order

E1. Module wrapper depends on Mach-O reader (have). ELF reader independent. Unload depends on Module wrapper + per-ptr ownership. Schema fully independent.
Proposed order: **Module → Schema → ELF → Unload.** Or interleave?

### Work items (locked per E1 ordering)

9.5.1 **Module wrapper.** Bootstrap allocates `MODULE_VT` proto-vt (delegates to `OBJECT_VT`). `Module load:` mints per-instance delegate vt + asserts `[mod :vtable <instance_vt>] [mod :ffi-handle <FfiPtr>] [mod :ffi-path <string>]`. Installs `load:`/`wrap:as:sig:`/`path`/`handle` methods on `MODULE_VT`. `wrap:as:sig:` uses Phase 9 `ffi.registerWrap` under the hood, installs returned closure on receiver's instance vt. Failure-mode entities per A2a/A2b.

9.5.2 **Schema cardinality.** New EIDs `ATTR_CARDINALITY`, keyword EIDs `KW_ONE` / `KW_MANY`. EAVT cell shape adjusted: `:many` attrs store multiple `(v, tx, op)` entries at same (e, a). `currentValue` becomes polymorphic per ADR-9.5-D2. New `currentValues(e, a) → iterator`. Forward-only schema (D3).

9.5.3 **Module symbols method.** `[mod symbols]` walks Mach-O reader on first call, asserts `[mod :ffi-has-symbol <sym_eid>]` (`:many` cardinality), one symbol entity per entry with `:ffi-symbol-name/addr/kind/external/module` facts. Subsequent calls = pure query via `currentValues`. Caches via `[mod :ffi-symbols-enumerated? true]`.

9.5.4 **Reader registry + ELF reader.** New `src/kernel/loader.zig`: fact-dispatched registry per B2. Each reader self-registers `[reader_eid :reader-magic <SmallInt>] [reader_eid :reader-impl <fn-ptr-Ref>]` at boot. Existing `macho.zig` lifts its `Symbol` type into `loader.zig`; aligns to drop N_STAB (B1). New `src/kernel/elf.zig` (~300 LOC) parses ELF64 `.dynsym`. Magic-byte dispatcher invokes correct reader. Linux FFI tests turn on; Mach-O tests skip-gate to darwin.

9.5.5 **Module unload.** New EIDs `ATTR_FFI_OWNER`, `ATTR_FFI_STALE`. Variant `boxed.mkFfiPtrOwned` threads owner via `:ffi-owner` fact at every pointer allocation (return values from FFI calls). `ffi.pushArg` for `.pointer`/`.cstring` checks `:ffi-stale?`; on stale, returns error-entity. `Module>>unload`: refuses when `promotion.ffiDepth() > 0`; otherwise walks `[?p :ffi-owner mod]` via `currentValues`, asserts `:ffi-stale? true` on each, retracts wrap closure facts, `dlclose`s handle, retains Module entity for walk-back.

### Reasoning
Module wrapper turns Phase 9's flat `registerWrap` primitive into a send-dispatchable surface (`[Module load: "path"]`, `[mod wrap: #sqrt: as: "sqrt" signature: "d)d"]`). Schema lands here because Phase 10's user-defined types want `[parent :children ?]` semantics. ELF + unload finish what Phase 9 deferred for spike scope. Bundled phase keeps the platform-parity story coherent.

## Phase 10 — Language Module

**Goal:** Birnam source syntax, parsed into facts, evaluated against fact store.

### Decisions locked (Phase 10 decision doc, 2026-05-29)

> **Terminology note:** birnam is strictly prototype-based (COLA / idst /
> Piumarta lineage per VISION.md). No class / instance split — vtables
> ARE prototypes, delegation via `:delegate` chain replaces inheritance.
> "Define Point as a Smalltalk class" = "mint a Point prototype-vt and
> install methods on it." Phase 10 grammar / evaluator MUST NOT
> introduce a class metamodel.

| Q | Decision |
|---|---|
| P1 Grammar encoding | **Pure facts.** Each PEG pattern is an entity with `:pattern-kind <keyword>` + combinator-specific facts. Phase 11+ materialization caching deferred. |
| P2 Packrat memo | **Transient AutoHashMap per parse**, freed at end. Memo is hot-path cache, lives outside facts (Phase 8 derived-state pattern). |
| P3 AST children | **`:ast-first-child` + `:ast-next-sibling`** linked list. All `:one`. Random access O(N). Pattern children use `:pattern-first-child` + `:pattern-next-sibling` (same shape). |
| P4 Source spans | **Byte `(start, end)` only.** Line/col derived on demand via `lineCol(source, offset)` helper. |
| P5 Parse failure | **Furthest-failure + `:peg-expected :many` set** of expected rule/literal name-EIDs at that position. Single diagnostic entity per failure. |
| P6 Charset encoding | **Fact-shaped range list**: linked list of range entities via `:charset-first-range` + `:charset-next-range`, each carrying `:charset-range-lo` / `:charset-range-hi` SmallInt code points. Renamed from `:char-class` per prototype tenet. |
| P7 Combinator scope | **Core 12 only**: `:literal`, `:seq`, `:choice`, `:star`, `:plus`, `:opt`, `:and`, `:not`, `:rule-ref`, `:charset`, `:any`, `:eof`. Captures / cut / actions / recovery deferred. |
| P8 Multi-grammar | **Multiple grammars coexist** as entities. Each parser invocation names one entry rule on one grammar. |
| P9 PEG API | **Both surfaces.** Kernel procedural API via IdApi for tests / engine internals; send-dispatched `Grammar>>defineRule:as:` / `Grammar>>parse:from:` for user code. |
| P10 Code location | **`src/modules/peg/` dylib** loaded after kernel boot. PEG is malleable-layer per VISION §296. Not in `src/kernel/`. |
| G2 Birnam syntax | **Visually Obj-C-y, semantically Smalltalk.** Bracket message sends `[recv msg: a with: b]`; Smalltalk blocks `[:x \| body]` or `[\| body]` (mandatory `\|` or `:` to disambiguate from sends, see below); strings `"..."` (no `@` prefix); assignment `:=`; statement separator `;`; comments `// ...` and `/* ... */`; messages-as-control-flow (no `if`/`while`/`for` keywords — `[cond ifTrue:[a] ifFalse:[b]]`); return `^`; no cascades; method definition via send (`[Point method: #foo do: [:x \| body]]`). |
| G2 Bracket collision | **Blocks must carry `\|` or leading `:`.** `[recv msg]` → message send. `[:x \| body]` or `[\| body]` → block. `[?e :attr ?v]` → pattern-send. Unambiguous via ordered choice in PEG. |
| G3 Keyword folding | **Smalltalk-standard compound folding.** `[obj a: 1 b: 2]` parses as single send of compound selector `a:b:` with 2 args. |
| G4 Reserved words | **Minimal**: `self`, `super`, `nil`, `true`, `false`, `^`, `let`. C-style control-flow words (`if`/`while`/`for`/`return`/etc.) are FREE — control flow is messages. |
| G6 Block temps | **`let name := expr` inside block body.** ST-style `\| t1 t2 \|` temp-decl syntax dropped. `let` is the sole binding-introduction form across method body, block body, and top-level scope (matches ADR-10-E2 "each `let x` claims `next_slot_index++`"). |
| G5 Pattern-send | **`?ident` variable marker.** PEG ordered choice discriminates `[?...]` as pattern-send before falling through to message send. |
| G5b Pattern-send eval | **Bare `[?e :a ?v]` at value position → Query entity Ref.** Parse-time: AST_PATTERN_SEND_VT stores `:pattern-clauses` (single clause for bare form; list for `#(...)` inside `where:`). Eval: assert new Query entity with `:query-clauses` Ref + scope-bound free vars → return query Ref. User sends `[query first]`, `[query findAll]`, `[query count]` (Phase 7 engine). Bare form = shortcut for `[db findAll: ?v where: #( [?e :a ?v] )]`. Inside `where: #(...)`, clauses remain quoted (Phase 7 mechanism, unchanged). |
| PR1 AST lifetime | **Permanent**, until Phase 13 prune. `:ast-parse <parse_eid>` back-ref enables per-parse retraction. |
| PR2 Source lifetime | **Method install copies its substring** to `[method :method-source <string Ref>]`. Parse-level source string is droppable independently. |
| PR3 Parse tx boundary | **One tx per parse call.** All AST-node + binding + literal-Ref asserts commit atomically. Parse failure → tx aborts, nothing persists. Eval-of-assignment and method-install are separate downstream txs (not folded). Tx-per-parse overhead is negligible (u64 increment + single RRB tx-log append). |
| PR4 Phase 10 GC interim | **Manual only in Phase 10.** `:reset` (full top-level wipe) and `:gc` (Phase 13.4 mark-sweep, user-triggered) debug-commands. Phase 13 lands the auto-policy: `:gc` runs automatically after every REPL line eval once 13.4 ships. Risk #2 (REPL session creep) accepted for Phase 10 spike period. |
| E1 Eval dispatch | **vtable + send.** Each AST kind has its own vt (e.g. `AST_KEYWORD_SEND_VT`, `AST_BLOCK_VT`, ...), eval is `[ast_node eval]`. COLA-pure dispatch; Phase 7.5 metabind directly enables per-kind overrides. |
| E2 Scope | **ADR-10-E2 (see below).** Fact-shaped binding entities for truth + parse-time slot resolution for runtime fast path. Two layers. |
| E3 Closures | **By reference**, Smalltalk-standard. Block holds `:block-scope <enclosing_scope_eid>`. Lifetime: Phase 13 reachability sweep (Phase 13.3). |
| E4 Non-local return | **Single-threaded side-channel `g_unwinding` + sentinel-Ref propagation** through eval frames. Phase 10.3 ship. Phase 11.5 retires this in favor of proper VM unwind opcode. |
| E5 Stack depth | **Depth guard at 8192** with `:stack-overflow` ERROR_VT entity on overflow. Phase 11 VM has no inherent limit (retires this). |
| E6 Error model | **ERROR_VT entities** (Phase 7.8 / 9.5.1 model, already shipped). Eval upgrades `bind`-returns-NIL on method-not-found to `:method-not-found` ERROR_VT at the eval layer for user-source sends only — kernel internals unchanged. |
| E6b Error self-contained | **Error entities snapshot diagnostic data at creation, never hold AST Refs.** Attributes captured: `:error-source-snippet` (boxed-string), `:error-line`, `:error-col`, `:error-byte-start`, `:error-byte-end`, `:error-kind`. AST mark-sweep (13.4) doesn't need error entities as roots; user-retained error Refs stay valid past AST prune. Trade: can't navigate from error back into live AST (re-parse if needed). |
| MD1 Install syntax | **`[vt method: #sel do: [block]]`** send. No special parser case. Symbol literals via `#ident` and `#kw1:kw2:` forms. |
| MD2 Install target | **Receiver = target vt** (no `onVt:` variant). Kernel-vt names **pre-bound in top-level scope** at language-dylib init via eager full-bind (single tx, `m = MOD_LANG`). Locked list: `Object`, `Vtable`, `Block`, `Float`, `FfiPtr`, `Module`, `Error`, `Scope`, `Binding`, `Nil`, `RRB`, `Boolean`, `True`, `False`, `SmallInt`, `String`, `Symbol`, `Vector`, `Closure`, `Selector`, `Predicate`, `Query`, `Grammar`, `Pattern`, `Db`. `self`/`super`/`nil`/`true`/`false` are G4-reserved AST literal-shape nodes, NOT bindings. User-shadowing via lexical scope. |
| MD3 Source provenance | **`:method-source` fact on closure entity** at install time. New selector `source` on `CLOSURE_VT` returns the boxed-string Ref (or NIL for anonymous closures). Also `:method-name`, `:method-installed-tx`. |
| MD4 super | **Prototype-super (Self / NewtonScript pattern).** `[super foo]` binds starting at enclosing-method-vt's `:delegate`, NOT the receiver's runtime vt. `self` identity preserved across send. Parse-time resolution: AST kind `AST_SUPER_SEND_VT` stores `:super-vt` = the syntactically enclosing method's vt. Use outside a method body → parse-time ERROR_VT `:super-outside-method`. |
| MD5 Reflection accessors | `[obj vtable]` → vt entity. `[vt delegate]` → delegate vt or NIL. `[vt name]` → string Ref (`:name` fact already kernel-shipped). `[vt methods]` → Query over AEVT for vt's selectors. User-facing vocab matches substrate vocab (no decoupling). |
| T1 Test corpus | **Minimum**: literals, arithmetic + binary ops, variable binding + assignment, block create + invoke, method install + dispatch, recursion (Fibonacci), control-flow messages, non-local return, pattern-send, error paths (every ERROR_VT kind). **Stretch**: hello-world print, Cons cell with `map`/`fold`, mutual recursion, block-closure-capture-aliasing. **Sub-dependency**: `SMALLINT_VT` + arithmetic methods + `FLOAT_VT` arithmetic + mixed-type coercion landed as Phase 10.0 prereq (10.0.x). |
| T2 REPL integration | **Phase 10 REPL is default**, with `:` prefix for Phase 6 kernel-debug commands (`:quit`, `:reload`, `:prompt`, `:scope`, `:tx`, `:reset`, `:source`, `:trace`, `:gc`). Multi-line input via PEG furthest-failure-at-EOF detection. |
| T2b REPL top-level lifetime | **Top-level scope = singleton entity** with `[scope :scope-top-level true]` fact, asserted once per REPL session. Acts as a 13.4 mark-sweep root: walk `[?b :bind-scope <top-level>] [?b :bind-value ?v]` → mark v subtree. Intermediate per-line AST without bind back-ref is sweepable. `:reset` debug-command retracts all `:bind-*` facts whose `:bind-scope` is top-level + drops scope-runtime-array slots, leaving the scope entity. `:gc` triggers 13.4 sweep on demand. |
| X1 PEG bootstrap | **Post-boot dylib**, loaded by shell before language module. Frozen kernel doctrine (VISION §296). |
| X2 Grammar persistence | **Asserted from Zig** at language-dylib init. ~50 rules. Module-owned (`MOD_LANG`); reload retracts + re-asserts. `.peg` source file format deferred to Phase 10.x / 11+. |
| X3 AST GC | **Reachable-from-roots mark-sweep via Phase 13.4** (now spelled out in Phase 13 work items). Root attribute set + edges to follow documented there. |
| X4 Left recursion | **Right-recursive rewrite** of birnam grammar (binary + unary chains only — keyword sends are bracket-delimited so no LR exposure). Warth packrat-with-left-rec deferred to Phase 11+ if grammar authors need it. |
| X5 Float literals | **Dot or exponent triggers Float**. `1.5`, `1e10`, `1.5e-3` → boxed Float Ref (Phase 9.2 type). Integer underscores `1_000_000` supported. Boxed Ref pre-allocated at parse, stored on AST node as `:literal-value`. |
| X5b Array literals | **`#( ... )` atoms-only literal**. Per VISION §224. Elements: number / string / symbol / nested `#(...)` / `nil`/`true`/`false`. No sends (literal-time eval). Parse-time allocates frozen `VECTOR_VT` Ref, stored on AST node as `:literal-value` (same pattern as X5). |
| X6 Self-hosting | **Trajectory documented, no committed phase.** Phase 10 ships interpreted (PEG + grammar + eval in Zig). E1 iii (vt + send) enables per-AST-kind eval override from birnam day one — partial self-host free. PEG / compiler / AOT live in malleable layer; can be rewritten in birnam as Phase 11.x evolution. Frozen kernel never self-hosts by design (VISION §296). |

### Documented risks

1. **PEG parse cost.** Pattern-graph traversal = 2 fact lookups per node visit. At spike scale invisible; Phase 11+ struct-cache materialization addresses if needed.
2. **AST entity accumulation.** Permanent ASTs grow until Phase 13 prune. REPL session memory creep accepted for spike.
3. **Scope leak via captured blocks.** Phase 13.3 reachability sweep cleans up. Spike-level scope accumulation accepted.
4. **Non-local return single-threaded.** `g_unwinding` global is single-threaded-only; matches birnam's broader single-threaded image assumption. Phase 11 VM goes per-thread.
5. **Stack overflow hard limit at 8192.** Pathological deep recursion fails gracefully via diagnostic entity, not crash. Phase 11 VM removes limit.
6. **Float precision.** `0.1 + 0.2 != 0.3` per f64 semantics. Document at user-language level; not a substrate issue.
7. **Send arity cap until 10.2.5.** Kernel `send0..send3` cap dispatch at 3 args. Phase 10.1.b lang grammar build sidesteps this with a procedural-IdApi alternative (`peg_mk_seq` / `peg_mk_choice` / `peg_mk_charset`) that takes a C array. The send-dispatched user surface for vector-arg PEG constructors is GATED on Phase 10.2.5 (sendN). Any user-source code attempting `[obj a: x b: y c: z d: w]` before 10.2.5 ships will hit the same cap. Phase 10.3 evaluator cannot land until 10.2.5 does.

> **ADR-10-Send-Arity (2026-05-30) — RESOLVED 2026-05-30.** Phase
> 10.2.5 shipped β (single variadic ABI). All 37 existing method
> impls migrated to `fn(*Send, [*]const Ref, usize) Ref`. send0..send3
> retained as thin packers. PEG `seq:`/`choice:`/`charset:` installed
> on PEG_VT via the variadic surface. Lang dylib's grammar build
> migrated off the procedural `peg_mk_*` IdApi entries onto sendN
> against PEG_VT. The deprecated IdApi entries remain in the struct
> (ABI discipline — appended-only) and are purgeable at the next
> image-format breaking change.
>
> Original decision context preserved below.
>
> **Why deferred from 10.1.b**: A.iii (variadic sendN) touches
> `send.zig`, `bind`, `mcache`, and closure ABI — frozen-kernel hot
> path. Landing it the same day as 10.1 grammar work risks regressing
> the entire dispatch surface (300+ tests gate on send0..send3 paths).
> Splitting into its own sub-phase between 10.2 (parser) and 10.3
> (evaluator) isolates re-validation and keeps the grammar build
> moving today.
>
> **Why scheduled before 10.3**: the evaluator's `[ast eval]` path
> for `AST_KEYWORD_SEND_VT` MUST be able to dispatch >3-arg keyword
> sends (e.g. `[obj at:put:into:withDefault:]`). Without sendN, the
> evaluator's compound-keyword dispatch is capped at 3 keywords.
>
> **Procedural IdApi entries in 10.1.b** (`peg_mk_seq`,
> `peg_mk_choice`, `peg_mk_charset`) are bootstrap-only conveniences.
> They are NOT user-facing. User-facing seq/choice/charset come via
> the send-dispatched surface once 10.2.5 ships.

> **ADR-10-E2 (2026-05-29).** Locked: fact-shaped binding entities for
> the truth layer + parse-time slot resolution for the runtime fast
> path. Two-layer design.
>
> **Truth layer:** each scope is an entity `[scope :scope-parent ?p]`.
> Each binding is its own entity with `:one` cardinality:
> `[bind :bind-scope scope] [bind :bind-name <interned-EID>]
> [bind :bind-value <Ref>]`. Walk-back, REPL introspection, debugger
> reach-back, and Phase 13 prune all work over this.
>
> **Speed layer:** at parse time, every variable reference resolves to
> `(scope-depth, slot-index)` against the lexical chain. Each `let x`
> in a scope claims `next_slot_index++`. AST nodes for variable
> references carry `:ast-bind-depth` + `:ast-bind-slot` + (debug-only)
> `:ast-bind-name`. Runtime scope = an array of `Ref` indexed by slot.
> Lookup = O(depth) frame-hops + O(1) slot read; depth is typically
> ≤ 4 for birnam methods. Effectively as fast as a Zig AutoHashMap
> while keeping the fact layer COLA-pure.
>
> **Mitigation across phases:**
>
> | Phase | Capability |
> |---|---|
> | 10.2 parser | tracks per-scope slot table during parse; resolves names → `(depth, slot)` in AST |
> | 10.3 evaluator | slot-indexed access at runtime; falls back to fact-query for REPL top-level / dynamic eval only |
> | 10.4 method install | method-body AST has fully resolved slots; method execution = pure slot lookups |
> | Phase 11 bytecode | compiles slot lookups to bytecode operands; no AST walk at runtime |
>
> **Assignment to undefined** = parse-time error (static scope).
> **Block closures:** parse-time slot resolution already accounts for
> the enclosing chain; block carries `:block-scope` Ref, invocation
> chains parents, slot access transparent.
>
> **Phase 8 promote not used here:** scopes are write-heavy by nature
> and would constantly demote under D6's any-mutation-demotes spike
> rule. Instead, the compiler-side slot resolution is the mitigation —
> facts truth, compiled slots are the cache. Same separation Phase 8
> uses, different mechanism.

> **ADR-10-E2-Interim (2026-05-30) — RESOLVED at truth-layer 2026-05-30 (S1-S4 shipped).**
> Phase 10.4.5 S1-S4 retired the interim. SCOPE_VT and BINDING_VT
> entities are now allocated per activation. Closure capture, `let`,
> `:=`, REPL persistence all work end-to-end. S5 (speed layer —
> parse-time slot resolution) remains deferred as a perf optimization;
> truth-layer correctness ships today.
>
> Historical record of the interim and its gaps preserved below for
> migration-path context.
>
> **What shipped (interim, lang/lang.zig):**
> 1. **`args[0] = current self Ref` threading.** Every eval method
>    takes a variadic Imp signature; the first argument carries the
>    current `self`. Threaded through nested send/keyword/literal
>    eval calls. AST_SELF_VT eval returns `args[0]`. Top-level eval
>    callers pass NIL (no self at the REPL prompt).
> 2. **`g_scope` flat module-static stack** (64 entries × `{name_eid,
>    value}` ScopeEntry). Pushed by blockEvalShim per param on
>    invocation; popped at return. AST_VAR_REF_VT eval consults it
>    via linear walk. Sufficient for block-as-method invocation;
>    insufficient for everything else.
>
> **What ADR-10-E2 spec requires (not shipped):**
> - **Truth layer**: per-scope SCOPE_VT entity with `:scope-parent`,
>   per-binding BINDING_VT entity with `:bind-scope` + `:bind-name`
>   + `:bind-value`. Kernel-EID slots reserved (100, 101, 135–139)
>   but no entity allocation or eval-path use yet.
> - **Speed layer**: parse-time slot resolution annotating AST_VAR_REF_VT
>   nodes with `:ast-bind-depth` + `:ast-bind-slot`. Kernel-EID slots
>   reserved (129–131) but parser doesn't compute or store them.
> - **REPL top-level persistence**: T2b singleton scope with
>   `:scope-top-level true`. Interim REPL has no persistent
>   bindings between lines.
> - **Closure-over-enclosing-scope** (E3 by-reference). Interim
>   blocks see only their own params (pushed at invocation) — they
>   cannot reference outer lexical bindings. A block defined inside
>   a method body has no link back to that method's params or temps.
> - **Method-frame back-pointer** on `^` (E4 + 13.4 reachability).
>   Parser doesn't emit `:ast-method-frame` because non-local return
>   isn't implemented yet.
>
> **Migration path (Phase 10.3+ scope-chain commit, deferred):**
> 1. Replace `args[0] = self_ref` with `args[0] = scope_eid` (a
>    SCOPE_VT entity). Add `:scope-self` attr; AST_SELF_VT eval reads
>    `[scope :scope-self ?]`.
> 2. Replace `g_scope` flat stack with BINDING_VT entity allocation
>    per block invocation. blockEvalShim asserts
>    `[bind :bind-scope new_scope :bind-name p_name :bind-value
>    args[i]]` for each param. AST_VAR_REF_VT eval queries
>    `[? :bind-scope <chain> :bind-name <n> :bind-value ?]`.
> 3. Add parse-time slot resolution as a second pass over the AST;
>    use the slot/depth annotations as fast-path lookups when present,
>    fall back to fact-query for REPL top-level dynamic eval.
> 4. The args[0] threading IS the path — once it carries a scope-Ref
>    instead of a self-Ref, every nested eval picks up the right
>    lexical chain automatically. No additional plumbing change.
>
> **Risk profile**: medium. Touches every eval method (10–15 file
> edits) and adds entity allocation per block invocation (perf cost
> until Phase 8 promotion kicks in or Phase 11 bytecode flattens it).
> Drops g_scope and the flat-stack code path. Bigger than 10.2.5
> sendN but smaller than its β refactor since dispatch ABI is
> unchanged.
>
> **What it unblocks**: closure capture of enclosing scope (E3),
> `:=` assignment (mutates existing binding), `let` introducing a
> new binding, REPL top-level persistence, full Smalltalk-class
> demo programs.

#### Original draft questions (preserved for context)

#### 10.0 — PEG module

P1. **Grammar-as-facts encoding.** Pattern nodes as fact entities (each
with `:pattern-kind` + child relations) vs pointer-Ref to Zig pattern
structs (Phase 7.1 rule-body pattern). Tradeoff: queryability /
walk-back vs allocation cost in parse hot path.

P2. **Packrat memo storage.** Transient per-parse AutoHashMap vs
fact-shaped persistent memo. Memo is hot read/write — fact-shaped
likely too slow.

P3. **AST representation.** `:ast-child` `:many` ordered (uses 9.5.2
cardinality) vs `:ast-first-child` + `:ast-next-sibling` linked-list.
Ordered-`:many` retrieval semantics need definition.

P4. **Source-span representation.** Byte (start, end) only, or line/col
too.

P5. **Parse failure reporting.** One diagnostic entity per failure vs
every-alternative-attempt entities. Stop-on-first vs recover-and-resume.

P6. **Char-class encoding.** Fact-shaped u8 ranges vs pointer-Ref to a
bitset.

P7. **PEG combinator scope.** Minimum (lit, seq, /, *, +, ?, &, !,
rule-ref, class) vs extras (named captures, semantic actions, error
recovery).

P8. **Multi-grammar coexistence.** Single global grammar vs many
grammars as entities. Birnam grammar + future config DSL + ...

P9. **PEG API surface.** Kernel procedural API (Zig fns via IdApi) vs
send-dispatched proto methods on a `PEG_VT` proto. Both?

P10. **Code location.** `src/modules/peg/` dylib only vs `src/kernel/
peg.zig` engine + thin dylib wrapper (Phase 7 query pattern).

#### 10.1 — Birnam grammar

G1. **Grammar source.** Inline Zig-asserted at module init vs `.peg`
source file. Bootstrap concern: parser-of-grammar is itself a grammar —
the first one is hand-coded.

G2. **Smalltalk syntax choices.** Exact precedence; cascade syntax;
block param syntax (`[:x|...]`); literal forms; comment style
(`" ... "` vs `// ...`); assignment (`:=` vs `<-`).

G3. **Keyword-message folding.** `[obj a: 1 b: 2]` parsed as a single
2-keyword `a:b:` send (Smalltalk-standard).

G4. **Reserved words.** `self`, `super`, `nil`, `true`, `false`,
`thisContext` reserved, or all bindings.

G5. **Pattern-send syntax.** Phase 7 query brackets `[?e :attr ?v]`
need parser-level recognition. `?`-prefix as var marker.

#### 10.2 — Parser runner

PR1. **AST entity lifetime.** Per-parse arena vs permanent. Phase 13
pruning later.

PR2. **Source-string lifetime.** Boxed-string Ref retained alongside
AST so byte offsets stay valid for `[method source]` introspection.

#### 10.3 — Evaluator

E1. **Evaluation strategy.** Recursive Zig per node-kind vs
fact-dispatched (`[node :eval-impl ...]`). COLA tension: hardcoded
switch vs malleable dispatch.

E2. **Scope representation.** Transient AutoHashMap during eval vs
persistent scope-entity chain (`:scope-parent` + `:scope-binding`
`:many`).

> **ADR-10-E2 (2026-05-29).** Locked: fact-shaped binding entities for
> the truth layer + parse-time slot resolution for the runtime fast
> path. Two-layer design.
>
> **Truth layer:** each scope is an entity `[scope :scope-parent ?p]`.
> Each binding is its own entity with `:one` cardinality:
> `[bind :bind-scope scope] [bind :bind-name <interned-EID>]
> [bind :bind-value <Ref>]`. Walk-back, REPL introspection, debugger
> reach-back, and Phase 13 prune all work over this.
>
> **Speed layer:** at parse time, every variable reference resolves to
> `(scope-depth, slot-index)` against the lexical chain. Each `let x`
> in a scope claims `next_slot_index++`. AST nodes for variable
> references carry `:ast-bind-depth` + `:ast-bind-slot` + (debug-only)
> `:ast-bind-name`. Runtime scope = an array of `Ref` indexed by slot.
> Lookup = O(depth) frame-hops + O(1) slot read; depth is typically
> ≤ 4 for birnam methods. Effectively as fast as a Zig AutoHashMap
> while keeping the fact layer COLA-pure.
>
> **Mitigation across phases:**
>
> | Phase | Capability |
> |---|---|
> | 10.2 parser | tracks per-scope slot table during parse; resolves names → `(depth, slot)` in AST |
> | 10.3 evaluator | slot-indexed access at runtime; falls back to fact-query for REPL top-level / dynamic eval only |
> | 10.4 method install | method-body AST has fully resolved slots; method execution = pure slot lookups |
> | Phase 11 bytecode | compiles slot lookups to bytecode operands; no AST walk at runtime |
>
> **Assignment to undefined** = parse-time error (static scope).
> **Block closures:** parse-time slot resolution already accounts for
> the enclosing chain; block carries `:block-scope` Ref, invocation
> chains parents, slot access transparent.
>
> **Phase 8 promote not used here:** scopes are write-heavy by nature
> and would constantly demote under D6's any-mutation-demotes spike
> rule. Instead, the compiler-side slot resolution is the mitigation —
> facts truth, compiled slots are the cache. Same separation Phase 8
> uses, different mechanism.

E3. **Block closure capture.** By-value vs by-reference (Smalltalk-style
aliasing — captured vars are shared mutable storage).

E4. **Non-local return.** Smalltalk `^` returns from enclosing method,
not just block. Ship in 10 or defer.

E5. **Tail calls / recursion depth.** Tree-walker blows stack on deep
recursion; bytecode VM (Phase 11) fixes. Document.

E6. **Error model.** ERROR_VT entities per Phase 7.8 / 9.5
ERROR_VT pattern (locked — already shipped).

#### 10.4 — Method definition

MD1. **Install syntax.** Source-form vs send-form
(`[Point method:#x do:[...]]`).

MD2. **Install target.** Receiver's own vt only vs explicit `onVt:`.

MD3. **Source provenance.** Keep `:ast-source-string-ref` on installed
closures for `[method source]` introspection.

#### 10.5 — Tests

T1. **Test corpus.** Fibonacci (recursion), Point prototype +
arithmetic, hello-string, scope/block capture, error path.

T2. **REPL integration.** Phase 6 REPL's hand-rolled line parser →
real PEG-driven parser. Replace or coexist (debug + production
parsers).

#### Cross-phase

X1. **PEG bootstrap order.** Kernel-bootstrap install of PEG facts vs
post-boot dylib load (like Phase 7 query module).

X2. **Birnam grammar persistence.** Asserted at language-dylib init,
once.

X3. **AST garbage collection.** Phase 13 pruning problem; Phase 10
spike leaks.

X4. **Left-recursion handling.** PEG natively can't express it.
Right-recursive rewrite vs PEG-extension (Packrat-with-left-rec).

X5. **Float literals.** `1.5` parses to boxed Float (Phase 9.5 type
already shipped).

X6. **Self-hosting trajectory.** Compiler rewritten in birnam: Phase
10 / 11 / 14? Doc only, not committing.

### Work items (locked per decision table)

**10.0 PEG module + arithmetic prerequisites.** Ships first as `src/modules/peg/peg.zig` dylib loaded by shell after kernel boot, before language module (X1, P10). New kernel EIDs:
- Combinator-kind keywords: `:literal`, `:seq`, `:choice`, `:star`, `:plus`, `:opt`, `:and`, `:not`, `:rule-ref`, `:charset`, `:any`, `:eof` (P7).
- Pattern attributes: `ATTR_PATTERN_KIND`, `ATTR_PATTERN_FIRST_CHILD`, `ATTR_PATTERN_NEXT_SIBLING`, `ATTR_PATTERN_CHILD`, `ATTR_LITERAL_VALUE`, `ATTR_RULE_REF_NAME` (P1, P3).
- Charset attributes: `ATTR_CHARSET_FIRST_RANGE`, `ATTR_CHARSET_NEXT_RANGE`, `ATTR_CHARSET_RANGE_LO`, `ATTR_CHARSET_RANGE_HI` (P6).
- Grammar entity + selectors: `GRAMMAR_VT`, `PEG_VT`, `SEL_DEFINE_RULE_AS`, `SEL_PARSE_FROM` (P8, P9).
- Failure diagnostic kind: `:peg-parse-failure` (P5) and `ATTR_PEG_EXPECTED` (`:many` cardinality).
Engine: pattern matcher in Zig, traverses fact-shaped patterns; per-parse `AutoHashMap(MemoKey, MemoResult)` for packrat (P2); furthest-failure tracking with expected-set; AST node allocation per match.
IdApi additions: `peg_define_rule`, `peg_parse`, `peg_grammar_new`.

**10.0.x SmallInt + Float + Boolean + Block-iteration prereq.** Before T1 tests can run any expression:
- New `SMALLINT_VT` kernel vtable + `FLOAT_VT` methods. Install `+`, `-`, `*`, `/`, `mod`, `=`, `<`, `>`, `<=`, `>=` on both. SmallInt + Float mixed dispatch via type-check in each method. `send.vtableEidOf` for SmallInt tags returns `SMALLINT_VT` (extends Phase 4 vtable resolution to immediate tags). Comparison ops return `TRUE_EID` / `FALSE_EID` Refs.
- New `BOOLEAN_VT` common-delegate proto + `TRUE_VT` / `FALSE_VT` with singletons at `TRUE_EID` / `FALSE_EID`. True/False delegate to BOOLEAN_VT via `:delegate` chain (Phase 4 mechanism). BOOLEAN_VT carries `not`, `and:`, `or:` default impls expressed in terms of `ifTrue:ifFalse:`. True/False each override `ifTrue:ifFalse:` (+ `ifTrue:`, `ifFalse:`).
- `BLOCK_VT` (also used by 10.1) gets `value`, `value:`, `value:value:` (eval block with N args).
- Iteration primitives — **Zig-native fast path, user-overrideable, owned by lang dylib (NOT kernel)**: `BLOCK_VT>>whileTrue:`, `whileFalse:`, `SMALLINT_VT>>timesRepeat:`, `to:do:`, `to:by:do:`. Body sends `value`/`value:` in a Zig loop to avoid tree-walker stack overflow (E5 8192 limit). Phase 11 bytecode VM retires these in favor of compiled loops.
- **Kernel-boundary discipline.** Only the SMALLINT_VT tag-resolution path (`send.vtableEidOf` extension for immediate tags) + new vt EID slots (TRUE_VT/FALSE_VT/BOOLEAN_VT/BLOCK_VT/SMALLINT_VT) live in frozen kernel — they are true substrate concerns (`obj[-1]` doesn't work on immediates). All method bodies (booleans, blocks, iteration) live in lang dylib, NOT `src/kernel/`. VISION §296 frozen-layer doctrine preserved.
- Install pattern: every method asserted as ordinary closure entity from lang-dylib init via Phase 7.4 native-fn-ptr pattern — `mkClosure` to allocate, assert `[closure :fn-native-ptr <FfiPtr-Ref>]` + `:method-name` + provenance, then `[vt sel closure_eid]` install fact (Phase 4 shape). From kernel POV indistinguishable from user-installed method; same dispatch path checks `:fn-native-ptr` for fast Zig call, falls back to AST eval otherwise. User can override any via 10.4 `[vt method: #sel do: [...]]`. `ifTrue:`/`ifFalse:`/`ifTrue:ifFalse:` ship as birnam-source one-liners (parsed at init, one-eval-frame); `value`/`whileTrue:`/`timesRepeat:` ship as native-fn-ptr closures.

**10.1 Birnam grammar.** ~50 PEG rules asserted from `src/modules/lang/lang.zig` at module init (X2). Encodes G2's locked syntax: bracket message sends with greedy compound-keyword folding (G3), Smalltalk blocks `[:x|body]` / `[|body]` with mandatory `|` or leading `:` to disambiguate from sends (G2-collision), pattern-sends `[?...]` (G5), strings `"..."`, numbers (X5 dot/exponent), symbols `#ident` / `#kw1:kw2:` (MD1), `:=` assignment, `;` separator, `//` + `/* */` comments, `^` return, reserved set (G4). Right-recursive rewrite for binary + unary chains to avoid left recursion (X4). New EIDs: `BLOCK_VT`, kind-vts for each AST node-kind (E1), `BINDING_VT`, `SCOPE_VT` (E2 truth layer).

**10.2 Parser runner.** Walks input source through the grammar via Phase 10.0 engine. Produces fact-shaped AST per P1+P3+P4 leans. Each AST node gets a kind-vt assigned per E1. **Parse-time slot resolution** (ADR-10-E2 speed layer): tracks per-scope slot table; variable references resolve to `(scope-depth, slot-index)` annotations on AST nodes (`:ast-bind-depth`, `:ast-bind-slot`, debug-only `:ast-bind-name`). Assignment to undeclared name → parse-time error. **Method-frame back-pointer**: parser tracks enclosing-method context during walk; every `^expr` AST node receives `:ast-method-frame <method_eid>` (new attr `ATTR_AST_METHOD_FRAME`) for E4 unwind-target resolution + 13.4 reachability. `^` outside any method body → parse-time ERROR_VT `:return-outside-method`. **Single tx per parse call** (PR3): all AST + binding + literal-Ref asserts commit atomically; parse failure aborts tx. Parse-failure → ERROR_VT entity with `:peg-parse-failure` kind + `:peg-expected :many` set (P5). AST entities permanent until prune (PR1) with `:ast-parse` back-ref.

**10.2.5 Variadic send (sendN) kernel extension.** Hard prerequisite for 10.3 — the evaluator must dispatch arbitrary-arity keyword sends from user source (`[obj a: x b: y c: z d: w]` is already 4 args). Current kernel caps at `send3` (matches Phase 9 FFI cap). Scope:
- New `Imp` signature variant for variadic-arity closures: `fn(*Send, [*]const Ref, usize) callconv(.c) Ref`.
- New `send.sendN(receiver, sel, args_ptr, count) Ref` dispatch path. Same `bind` lookup (mcache key is `(vt, sel)`, no change), but invokes the variadic Imp signature.
- Closure ABI: closures carry an arity-mode marker so `bind` can dispatch to the correct Imp signature. Either (a) reuse a slot on the closure struct (least disruptive — add `mode: u8` field, default = fixed-arity), or (b) reserve a separate kernel vt (`CLOSURE_VARIADIC_VT`) and discriminate by vt at dispatch.
- Existing `send0`..`send3` paths preserved unchanged — they remain the fast path for arity ≤ 3 (mcache hit + tail-call). `sendN` is the slow path that pays one extra indirection (vararg pack).
- IdApi: append `sendN` fn ptr; existing `send0..send3` unchanged.
- Bootstrap method-install: existing `installMethod` helper unchanged (installs fixed-arity); new `installMethodVariadic` for the variadic ABI.
- Tests: send-arity-4..send-arity-7 round-trip, mcache hits across mixed-arity sends on the same vt, fixed-arity-3 path unaffected by sendN's presence.

**Risk profile**: touches frozen-kernel hot dispatch path (`send.zig`, `bind`, `mcache`, closure ABI). Single-commit kernel-deep edit. Re-validates every existing test path under the extended dispatch surface. Comparable to Phase 7.5 metacircular bind in scope. ~500–800 LOC across `send.zig` + `closure.zig` + `id_api.zig` + `bootstrap.zig` + new tests. Probable: 2 sessions.

**Unblocks**: 10.3 N-arg keyword send dispatch; 10.4 method-install for >3-arg method bodies; 10.4+ user-source PEG grammar mutation via send-dispatched seq/choice (`[PEG seqWith: a with: b with: c with: d with: e]` no longer arity-capped); future Phase 9 FFI arity expansion.

**Scheduled here (between 10.2 parser and 10.3 evaluator)** rather than rolled into 10.3 because the kernel-deep change is independent of the eval logic and warrants its own re-validation pass before evaluator work begins. Phase 10.1.b (lang grammar) ships the procedural-IdApi alternative (`peg_mk_seq` / `peg_mk_choice` / `peg_mk_charset`) so the grammar build is unblocked TODAY without waiting for sendN — but those IdApi entries are bootstrap-only conveniences and stay procedural; the send-dispatched user surface for vector-arg PEG constructors lands once 10.2.5 ships.

**10.3 Evaluator.** Eval = `[ast_node eval]` send dispatched via each kind-vt's installed `eval` method (E1). Top-level scope pre-bound with full locked kernel-vt name list per MD2. Runtime scope = array of `Ref` indexed by parse-time slot (ADR-10-E2 fast path). Block closures hold `:block-scope <enclosing_scope_eid>` by reference (E3). Non-local return via `g_unwinding` side-channel + sentinel-Ref propagation (E4); `^` eval reads `:ast-method-frame` to identify unwind target frame; diagnostic on escaped-block-return (`:return-from-dead-method`). Depth guard at 8192 with `:stack-overflow` ERROR_VT on overflow (E5). Method-not-found upgraded from NIL to `:method-not-found` ERROR_VT at eval layer (E6). Error entities snapshot diagnostic data (E6b) — never hold AST Refs. Error propagation via value-pass-up (`isErrorEntity` check after sub-eval). **Requires Phase 10.2.5 (sendN)** for >3-arg keyword sends.

**10.4 Method install.** `[vt method: #sel do: [block]]` send (MD1). Method install asserts `[vt sel mkEid(closure_eid)]` (Phase 4 dispatch shape unchanged). Closure also carries `:method-name`, `:method-source` (substring copied per PR2), `:method-installed-tx` (MD3). New selector `source` on `CLOSURE_VT` returns the boxed-string Ref. Block-as-method-body picks up enclosing scope at install for free (Smalltalk capture semantics). Method body AST has fully resolved slots so dispatch = slot-index access (ADR-10-E2).

**10.4.5 Full scope chain (ADR-10-E2 spec). — S1-S4 ✓ shipped 2026-05-30; S5 deferred.** Retired the Phase 10.3/10.4 interim — `args[0]=self_ref` threading + `g_scope` flat stack — in favor of the locked design.
- Replace args[0]=self_ref with args[0]=scope_eid (a SCOPE_VT entity). New attr `:scope-self`; AST_SELF_VT eval reads from the current scope entity.
- BINDING_VT entity allocation per block invocation + per let. Asserts `:bind-scope` + `:bind-name` + `:bind-value`. AST_VAR_REF_VT eval queries via the scope chain.
- Parse-time slot resolution second pass annotates AST_VAR_REF_VT with `:ast-bind-depth` + `:ast-bind-slot`. Eval reads slots fast-path; falls back to fact query for REPL top-level / dynamic eval.
- T2b REPL top-level scope singleton with `:scope-top-level true`. Phase 13.4 GC root.
- E3 closure-over-enclosing-scope: block carries `:block-scope` Ref captured at AST_BLOCK_VT eval time (currently NIL).
- `:=` assignment + `let` binding desugar paths (depend on this; defer until scope ships).
- Method-install body's slot resolution runs at install time so dispatch is pure slot lookups.
- ADR-10-E2-Interim block in the Phase 10 risks section is marked RESOLVED when this lands.

**10.5 REPL replacement.** Replaces Phase 6 REPL surface: PEG-driven multi-line input, top-level scope persists across inputs, eval prints result or ERROR_VT diagnostic, `:` prefix for kernel-debug commands (`:quit`, `:reload <module>`, `:prompt <str>`, `:scope`, `:tx`, `:reset`, `:source <sel>`, `:trace <sel> <on|off>`) (T2). Multi-line continuation detection via PEG furthest-failure-at-EOF (T2).

**10.6 Tests.**
- *PEG engine*: pattern-match correctness across all 12 combinators, packrat-memo hits, furthest-failure error localization, multi-grammar coexistence.
- *Birnam grammar*: parse every G2 syntactic construct round-trips; bracket-disambiguation cases (block vs send vs pattern-send) work; left-recursive rewrite produces correct AST.
- *Parser*: AST shape per kind, source-span correctness, slot resolution coords, assignment-to-undeclared errors.
- *Evaluator*: T1 minimum corpus — literals, arithmetic on SmallInt and Float, mixed-type, variable binding + assignment, block create/invoke, method install + dispatch, Fibonacci recursion, control-flow messages (`ifTrue:ifFalse:`, `whileTrue:`, `do:`, `timesRepeat:`), non-local return from `do:`, pattern-send returns a Query value, every ERROR_VT kind tested. T1 stretch — Cons cell with `map`/`fold`, mutual recursion, block-closure-capture-aliasing.
- *Method install*: install + invoke round-trip, `[method source]` returns original substring, `:method-installed-tx` set.
- *REPL*: multi-line input with continued blocks, error display, `:` commands route correctly.

### Reasoning
Putting PEG before the birnam grammar keeps the layering clean — the engine is a reusable substrate, not entangled with one language's AST shape (X1 + P10 lean). Once the language module works, partial self-hosting is free: E1's vt+send eval dispatch means a user `[AST_KEYWORD_SEND_VT method: #eval do: [...]]` override works from day one (X6 trajectory). Full self-hosting trajectory: PEG / compiler / AOT all in malleable layer, can be rewritten in birnam as Phase 11.x evolution — kernel never self-hosts per VISION §296 frozen-layer doctrine.

Phase 11 inherits two spike concessions from Phase 10.3 — the `g_unwinding` side-channel for non-local return (replaced by proper VM unwind opcode per 11.5) and tree-walker stack overflow (replaced by VM stack). Phase 13 inherits AST mark-sweep + closure-scope reachability sweep (13.3 + 13.4).

---

## Phase 11 — Bytecode + VM

**Goal:** Compile AST facts → bytecode facts; execute via VM.

### Work items

11.1 Bytecode design: stack-based, ~20 opcodes (push, pop, send, return, jump, branch, load_local, store_local, push_const, etc.). Each opcode is an entity.

11.2 Compiler: birnam module that walks AST facts, emits bytecode facts.

11.3 VM: Zig primitive interpreter (initial), can be replaced by a birnam module later. Threaded-code dispatch on hot path.

11.4 Inline caching at call sites in bytecode (per-pc mcache).

11.5 **Non-local return replacement (Phase 10.3 deferred work).** Per ADR-10-E4 locked in Phase 10.3, the tree-walker ships non-local return via a single-threaded side-channel global (`g_unwinding`) plus a sentinel-Ref-propagation protocol. Phase 11 replaces this with a proper bytecode mechanism:
- Dedicated `RETURN_FROM_METHOD <frame_offset>` opcode unwinds the call stack to the named frame, drops intermediate block activations, and resumes there with the return value.
- Each method invocation pushes a frame on the VM call-stack; `^` compiles to the unwind opcode with the static method-frame-depth resolved at compile time.
- The "escaped block returning from dead method" edge case is detected by checking the VM call-stack for an active matching frame; absent → emit ERROR_VT entity (`:error-kind :return-from-dead-method`), matching the tree-walker's diagnostic.
- Side-channel global retired; non-local return becomes per-VM-thread (forward-compatible with concurrency landing later — VISION §multi-thread already calls this out).
Tests: parity with tree-walker behavior across all Phase 10 non-local-return test cases, plus VM-only escape detection.

11.6 Tests: VM matches tree-walker output on the Phase 10 test suite.

### Reasoning
VM gives us portability and AOT. Bytecode-as-facts means the compiler is also queryable and diffable — we get a "what changed in the compiled form?" view for free. Bytecode also lets us retire two Phase 10 spike concessions: the side-channel `g_unwinding` global for non-local return (11.5), and tree-walker stack overflow on deep recursion (proper VM stack handles it natively).

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

**Goal:** History excise + entity reachability sweep.

### Work items

13.1 Pruning policies: keep-last-N-txs, reachable-from-roots, manual-excise-of-named-entities.

13.2 Pruning runs as a separate tx that rewrites indexes excluding pruned facts. Old index roots become unreferenced and GC-able.

13.3 **Closure-scope reachability sweep (Phase 10.3 deferred work).** Per ADR-10-E3 locked in Phase 10.3, block closures capture by reference — each block holds a `:block-scope` Ref. Scope entities are reachable iff any live block holds them or any reachable scope chains to them. The reachable-from-roots pruning policy (13.1) must walk:
- `[?b :vtable BLOCK_VT] [?b :block-scope ?s]` — every live block roots its captured scope
- `[?s :scope-parent ?p]` — transitively root parent scopes via chain
Any scope entity not reached by this walk is dead and gets pruned along with its `:bind-*` binding entities. Tests must include: block-outlives-method (scope stays alive), block-dies-with-method (scope sweepable), nested-blocks (parent chain reachability), block-of-block (transitive root).

13.4 **AST mark-sweep (Phase 10 deferred work).** Per PR1 / X3 locked in Phase 10.2/10.3, AST entities live permanently until pruned. The reachable-from-roots policy must walk from the following root attributes and mark every transitively-reached AST node before sweeping unmarked nodes tagged `:ast-parse <parse_eid>`. Edges to follow: `:ast-first-child`, `:ast-next-sibling`, `:ast-method-frame` (Phase 10.3 E4 frame-reference back-pointer on `^` nodes), `:pattern-first-child`, `:pattern-next-sibling`, `:charset-first-range`, `:charset-next-range`. Root attribute set:
- `[?m :method-body-ast ?]` — installed methods (Phase 10.4)
- `[?b :vtable BLOCK_VT] [?b :block-body ?]` — live blocks (Phase 10.3)
- `[?s :saved-query-spec ?]` — saved queries (Phase 7.3, already shipped)
- `[?r :rule-pattern ?]` — grammar rule patterns (Phase 10.0)
- `[?s :scope-top-level true] [?b :bind-scope ?s] [?b :bind-value ?v]` — REPL top-level bindings (Phase 10.5 T2b)
- `[?w :ffi-sig ?]` — FFI wrap entities carry a pointer-Ref to a heap Sig struct (Phase 9.3 — special-case: not an AST root proper but lives in the same prune sweep; needs Zig-side free callback registered with the sweep)
Tests: every-root-keeps-its-tree (one test per root attribute), orphan-AST-pruned (parse without install), grammar-rule-protected (rules survive prune), refloating method (re-install via `method:do:` reroots).

13.5 Tests: pruning preserves reachable state, removes excised entities, query results match pre-prune for non-excised data. Plus the closure-reachability cases above and the AST mark-sweep cases.

### Reasoning
History grows forever otherwise. Pruning must be user-initiated (don't surprise the user with lost history). AOT prune (most aggressive variant) reuses this machinery. The closure-reachability sweep is the substrate's GC-by-reachability — Phase 10.3 accepts spike-level scope leak; Phase 13 cleans up via reachability walk over the fact graph.

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
