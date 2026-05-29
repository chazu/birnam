# Birnam Roadmap

Detailed work sequence from current zig/birnam (linear-scan vtables, no fact store) to the locked vision (relational substrate, tagged pointers, HAMT+RRB everywhere). Reasoning per phase.

This roadmap follows VISION.md. Open questions in VISION.md must be resolved before starting Phase 1.

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

## Phase 6 — REPL Module

**Goal:** REPL is an object, not a shell loop.

### Work items

6.1 Decide minimum surface syntax for the REPL: unary, binary, keyword, cascade, block. Final syntax later.

6.2 Implement a tiny tokenizer/parser in birnam? No — too early. Use a hand-rolled Zig parser in the REPL module for now; replace when language module lands.

6.3 REPL prototype object with `run`, `setPrompt:`, `setTarget:`, `stop`, `describe` methods (mirroring C-birnam's repl.c, but as facts).

6.4 Shell drops its built-in REPL once `repl` module loads.

6.5 Tests: REPL handles unknown selector gracefully; can invoke any installed method.

### Reasoning
REPL-as-object is a litmus test for the substrate. If REPL works as a real fact-shaped object with mutable methods, the rest of the user-facing system follows the same template. Hand-rolled Zig parser is intentional throwaway — replaced when the language module is built atop birnam itself.

---

## Phase 7 — Query Module

**Goal:** Datalog as object messages.

### Work items

7.1 Surface syntax: `db query: { find: #(?m), where: [[vt #lookup: ?m]] }` — keyword-message API initially.

7.2 Query planner: trivial join on small queries. Pick indexes per clause. No optimizer yet.

7.3 Query executor: walk indexes per the plan, materialize results as RRB-Tree.

7.4 Cursor protocol for streaming large result sets.

7.5 Tests: equality joins, multi-clause joins, negation (later), aggregations (later).

### Reasoning
Reflection-as-query is one of the major wins of the relational substrate. Get a working subset (point queries + simple joins) before designing the surface syntax in earnest. Real Datalog (recursion, negation, aggregation) can wait.

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

**Goal:** Iterative wrapping of arbitrary shared libraries from the REPL.

### Work items

9.1 Symbol enumeration: bundled Mach-O reader (~300 LOC) and ELF reader (~300 LOC). Returns iterator of `(name, addr, kind)`.

9.2 `Module load: <path>` returns a Module object with `symbols` query.

9.3 Integrate dyncall (or libffi) for unknown-signature calls. Vendor as submodule.

9.4 `Module>>wrap:as:signature:` records the binding as facts: target selector, libc signature, target symbol.

9.5 Dispatch path: when sending a wrap message, materialize a tiny shim closure that marshals args via dyncall.

9.6 FFI pinning: while a call is outstanding, the involved objects must stay materialized (don't demote mid-call). Refcount during FFI boundary.

9.7 Tests: load libm, wrap `sqrt`, call. Load libc, wrap `printf` (variadic — punt or constrain).

### Reasoning
This is the COLA dream made concrete: interactively absorb any C library into the object world. Mach-O + ELF readers are small (no libelf dependency); dyncall is well-trodden territory. FFI pinning prevents the worst correctness hazard of the materialization layer.

---

## Phase 9.5 — Schema with Multi-Cardinality Attributes

**Goal:** Promote single-valued attributes (Phase 3 default) to a schema-driven mix of single + multi-valued.

### Work items

9.5.1 Add a `:cardinality` schema attribute on every attribute entity. Default `:one`. Set to `:many` for relations that accumulate.

9.5.2 Update `assert` to check the attribute's cardinality. `:one` → existing semantics (new assert implicitly retracts old). `:many` → assert appends, no implicit retract.

9.5.3 Update queries to return iterators where multi-valued attrs are involved.

9.5.4 Tests: `:many` accumulation, mixed-cardinality queries, schema-driven dispatch in fact_store.

### Reasoning
Phase 3 ships single-valued only — sufficient for vtable/method facts (one method per (vt, sel)) and the bootstrap kernel. Multi-cardinality is a user-modeling feature, not a kernel concern. By Phase 9.5, the language module (Phase 10) is about to land and user code will start modeling things like `[parent :children ?]` that genuinely need accumulation. Schema-driven (not blanket) keeps Date-discipline intact.

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

12.5 Tests: save → kill → load → run → identical behavior.

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
