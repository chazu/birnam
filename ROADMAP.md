# Birnam Roadmap

Detailed work sequence from current zig/birnam (linear-scan vtables, no fact store) to the locked vision (relational substrate, tagged pointers, HAMT+RRB everywhere). Reasoning per phase.

This roadmap follows VISION.md. Open questions in VISION.md must be resolved before starting Phase 1.

## Status (as of 2026-06-03)

**Headline — Phases 0–14 complete; Phase 15 (heap GC) SHIPPED + merged: four-arena value-heap mark-sweep (:gc + auto-trigger, FFI Sig free) + index epoch arena (15.I, compact frees the old HAMT tree wholesale, future-proofed for retained snapshots). Remaining: const-pool RRB finalizer + AST-drop policy (gated on 15.R retention). New arc Phase 15.R = retention/snapshots/excision/cold-history. 400/400 tests, 0 leaked.**
- **9.5.6** ✓ — FFI surface reachable from the language + file-in. `Module` prototype bound globally (`Module load:`/`resolve`), `language` prototype with `fileIn:`/`fileIn:onError:`, shell `:in`/`:in!`. FFI wraps refactored to **declarative facts** (`:ffi-wrap-symbol`/`:ffi-sig-str`/`:wrap-selector`, M_USER) with **deferred `dlopen`**: a single `resolveModule` does the open+`dlsym`+install at commit (REPL line / file-in / image load). Wraps now **relink by symbol name** across processes (fixes stale-`:ffi-addr` persistence; foundation for freezing a wrapper into a module dylib). Also fixed a latent keyword-send arg-arity overrun (stale `mkEid(0)` `:next-sibling` on recycled eids). No "class" concept — prototypes only. See the Phase 9.5.6 §Status block.
- **0–11** ✓ — substrate, datalog, FFI, modules, language, bytecode VM. Phase 11.12 shipped the lang switchover: user methods dispatch through the bytecode VM (not the tree-walker). Loose ends (in-band mcache wiring, full `g_unwinding` retirement, tail-call refactor) deferred, non-blocking.
- **12** ✓ — image snapshot (`.bim`, single self-describing file: facts + embedded manifest + interner). Provenance split (`M_USER`), Approach-B closure rebuild, shell `:save`/`:load`. A REPL session survives process restart (proven). See the Phase 12 §Status block.
- **13** ✓ — pruning: reachable-from-roots mark-sweep + `FactStore.compact` + shell `:prune`. Re-derived roots/edges for the bytecode-closure model. See Phase 13 §Status.
- **14** ✓ — AOT. `birnam --aot=<static|dynamic> [--strip=malleable] [--strip=history] <image> [--entry=<sel>[:<recv>]] -o <out>`. All six matrix cells driver-built per-app: static + dynamic (portable dlopen-at-startup bundle via `$BIRNAM_DYLIB_PATH`/exe-relative), `--strip=malleable` (pure-VM, AST/frontend stripped), `--strip=history` (RO-frozen store). General `--entry` resolves a SmallInt or a named top-level binding. Only the mmap-relocation RO-*segment* perf path is deferred → Phase 12.5 (not a capability gap). See Phase 14 §Status.

The per-phase **§Status** blocks (Phases 12/13/14) are the authoritative current state. The detailed Phase 10/11 notes below are kept as history.

**Completed (substrate):**
- Phase 0–7.5 ✓ (collections, fact store, kernel, datalog, FFI, modules)
- Phase 9 / 9.5 ✓ (boxed Float + FFI ptr, dyncall, Mach-O / ELF readers, Module wrapper, schema cardinality, unload)
- Phase 9.5.6 ✓ (FFI reachable from language: `Module`/`language` prototypes, file-in `:in`/`:in!`, declarative deferred-resolve wraps, relink-by-name across save/load)
- Phase 9.5.6c ✓ (freeze: `:freeze <base>` reconstructs canonical wrapper source from wrap facts + emits & `zig build-lib`s a standalone wrapper module dylib; loading it re-files-in via `language fileInSource:` → re-`dlopen`s the alien lib. `freeze.zig` + shared `module_abi.zig` mirror. Proven e2e: compiled dylib re-establishes wraps in a fresh process.)

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
- Float methods on FLOAT_VT ✓ (`+`, `-`, `*`, `/`, `=`, `<`, `>`, `negate`, `asInt`, `exp:`). SmallInt args auto-coerce to f64. `asFloat` on SmallInt. `[1.5 exp: 10]` substitutes for grammar exponent notation `1.5e10`.
- Phase 10.4.5 S5 superseded by Phase 11 ✓ (see note below — no interpreter implementation needed)

**Phase 11 — substrate substantially shipped (2026-05-31 / 2026-06-01):**
- 11.1 ✓ — opcode set (16 ops, ADR-11 addendum locked), annotation EIDs (kernel_eids 165..182), bytecode encoder/decoder API.
- 11.2 ✓ — AST → bytecode compiler. Coverage: literals (smallint inline + pool), stmt-seq, return-expr, self, sends (0/1/N with site_idx), blocks (separate bytecode-method entities, `:block-parent-method`), let / var-ref / assign with compile-time scope map, block params as locals[0..n]. Deferred: AST_SUPER_SEND_VT, **closure scope capture** (see below).
- 11.3 ✓ — VM dispatch (while+switch loop; all 16 opcodes; tail-call refactor deferred — probe already validates lowering). `bytecodeDispatcherImpl` makes compiled methods first-class via SEND through existing send.bind + mcache.
- 11.4 ✓ — per-method MethodMCache structure (16B-aligned MCEntry, 16B atomic store writer, 3-load re-check-vt reader, walk-invalidate). In-band wiring at SEND_* deferred (today routes through send.sendN's global mcache); structure ready when bench warrants.
- 11.5 partial — `RETURN_FROM_METHOD` opcode + `Vm.unwind_value` / `unwind_target_remaining` shipped. Tree-walker `g_unwinding` global stays alive until lang module switches over to bytecode dispatch.
- 11.6 ✓ — reify-stack API + real `Vm.frames` tracking. `runMethod` push/pop, pc write-through, frees on drain. `reifyStack(...) → []u64` asserts `:frame-pc` / `:frame-method` / `:frame-self` / `:frame-parent`.
- 11.7.i ✓ — `Vm` struct (frames + pc + bytecode + current_self + unwinding + recursion_guard + ffi_pause + bump_arena_slot). `g_recursion` migrated from send.zig; `g_ffi_depth` from promotion.zig.
- 11.7.iv ✓ — atomic EID + tx counters via `std.atomic.Value(u64)`.
- 11.7.v ✓ — `tx_engine.commit(op)` consolidated entry point.
- 11.7.vi ✓ — `Interner.writer_lock` (`std.Thread.Mutex`) + lock-free reader fast path with under-lock recheck.
- 11.7.vii — image manifest mode tag; folded into Phase 12 (embedded manifest carries the thread-mode tag). The closure fn-ptr serialization boundary was resolved via **Approach B** (rebuild closures from facts on load), not Approach A — see Phase 12 §Framing decision.
- 11.8 ✓ — annotation emit at materialize time (`:bytecode-stream`, `:constant-pool`, `:method-body-ast`, three sizing SmallInts, `:block-parent-method`).
- 11.9 ✓ — tail-call probe CI gate (1M-depth two-handler chain, ReleaseFast). Failure = build error per ADR-11 addendum #7.
- 11.10 partial — kernel-side end-to-end tests cover every opcode + composite programs (let + binary send → 6). Full parity vs tree-walker = lang module switchover gate.

**Lang switchover infrastructure shipped:** `IdApi.compile_block_as_method(block_ast_eid) → method_eid` appended (ABI discipline preserved). Kernel-side `cCompileBlockAsMethod` calls `compiler.compileBlockAsMethod` + `materialize`. Lang dylib mirror struct updated. **`vtMethodDoImpl` switchover now LIVES (Phase 11.12 below) — this "NOT YET wired" note is historical.**

**Phase 11.11 ✓ — closure scope capture (by-value, 2026-06-01):**
- Compiler `State` gains `parent_state` chain. `resolveVarSlot` walks parents recursively; each intermediate level records a `CaptureSpec` so the runtime chain stays flat (each block frame reads only from its immediate parent).
- `CompiledMethod.captures: []CaptureSpec` materializes to a heap `CaptureSpecArrayHeader` + inline tail, pointer-Ref stored under `:method-captures-ptr`.
- `MAKE_BLOCK` reads spec at runtime; if non-empty, allocates a `BytecodeBlock` (extern struct: method_eid + captures_len + inline `[]Ref` tail), copies parent-frame locals at `parent_slot`, pushes the pointer Ref. Non-capturing blocks stay on the fast `mkEid(method_eid)` path.
- `INVOKE_BLOCK` tag-dispatches on the block Ref (EID = no captures, Pointer = with captures), extracts captures, threads through `runMethod` which copies them into the compiler-assigned local slots.
- End-to-end test: `let x = 5; [x]` returns a `BytecodeBlock` pointer Ref with captures=[5]; invoking it returns 5.
- **By-reference semantics deferred to Phase 11.12** — captured-then-mutated values don't propagate to outer scope. Phase 11.12 will replace the per-MAKE_BLOCK copy with a shared scope-entity heap allocation (or pointer-to-locals + escape-analysis).

**Phase 11.12 ✓ — by-reference closures + lang switchover lives (2026-06-01):**

`BytecodeClosure` carries `(method_eid, parent_self_ref, parent_frame_id, captures)`. `MAKE_BLOCK` always allocates a real CLOSURE_VT entity wrapping the `BytecodeClosure` and pushes `mkEid(closure_eid)` so `[block value]` dispatches uniformly via `send.bind` regardless of caller context (tree-walker or bytecode).

`bytecodeDispatcherImpl` branches on `closure.data` shape:
- `isEid` (direct method install): `self = s.receiver`, no captures.
- `isPointer` (block from MAKE_BLOCK): `self = parent_self_ref` from the BytecodeClosure, captures from inline tail. Smalltalk-semantic `self` inside the block matches the enclosing method's receiver.

`closureValueImpl` in lang.zig polymorphic: dispatches through the receiver's `closure.method` (function pointer). Tree-walker blocks route to `blockEvalShim`; bytecode methods route to `bytecodeDispatcherImpl`. Same Imp signature, single indirect call.

Non-local return propagation through SEND: `handleSendUnwind` helper runs after every `SEND_*` opcode. `Vm.unwind_target_remaining` counts down through nested frames; depth==0 catches and returns `unwind_value` as the method's normal result. `Vm.next_frame_id` + `Frame.frame_id` give RETURN_FROM_METHOD stable target identity (currently informational; full unwind-by-id is a follow-up).

**Lang switchover at `vtMethodDoImpl` LIVES.** All 32 lang_module_tests pass including DEMO 10.4b (`[3 plusOne: 7]` → 8) and DEMO 10.x (`^88` through value-frame to enclosing method). Bytecode VM substrate is now the real eval path for user-installed methods.

**Materialize correctness fixes (2026-06-01, found during 11.12 debug):**
1. `promotion.onEntityMutate` demotes any materialized entity on every fact assert. Previous materialize put the closure FIRST then asserted facts → demote cleared the table entry. Fix: put closure LAST.
2. Closure must conform to libid-object layout `[vt_ref][method][data]` with `vt_ref` at offset -1. Materialize previously allocated raw Closure → `send.vtableEidOf` read garbage memory before the struct. Fix: 3-Ref buffer with `mkEid(CLOSURE_VT)` at -1, mirroring `id_api.cMakeClosure`.

**Test count: 350/350 across 85 build steps.**

---

**Pre-Phase-11 baseline (for historical reference, 2026-05-30): 291/291 across 73 build steps.**

**Demoable today (live REPL via `./zig-out/bin/birnam libbirnam_peg libbirnam_lang`):**
- Arithmetic + comparisons + branching
- `let` bindings + `:=` mutation
- User-defined methods via `[vt method:#sel do:[block]]`
- Closures-over-enclosing-scope (`makeAdder := [:a | [:b | [a + b]]]`)
- REPL persistence across lines

**Deferred (no remaining blockers — both items resolved as "won't ship in interpreter"):**
- Phase 10.4.5 S5 (parse-time slot resolution): **superseded by Phase 11**. The bytecode VM compiles `AST_VAR_REF_VT` nodes directly to bytecode slot-load operands. Implementing S5 in the tree-walker would just be thrown away when Phase 11 lands. Truth-layer correctness (S1-S4) is sufficient for the spike interpreter; speed comes from Phase 11 codegen, not from annotated AST nodes.
- Float exponent notation (`1.5e10`): **superseded by `exp:` method**. User writes `[1.5 exp: 10]` instead. Grammar simpler, no overlap with `e`-as-identifier-start ambiguity.

**Phase 10 closed.** The language has: arithmetic + comparisons on SmallInt and Float (with mixed-type coercion), control flow via `ifTrue:ifFalse:`, blocks with parameters, closures-over-enclosing-scope captured by reference, mutation via `:=`, recursion via method install, iteration primitives (`whileTrue:`, `whileFalse:`, `timesRepeat:`, `to:do:`), statement sequencing via `;`, comments (`//` and `/* */`), string literals, symbol literals (including compound `#kw1:kw2:`), float literals, non-local return `^`, and full user-overridable method install via `[vt method:#sel do:[block]]`. REPL drives the stdin loop with persistent top-level bindings. All ~28 demo cases run in 73 build steps with 291 passing tests.

**Next major arcs (in dependency order):** *(2026-06-04: 11 ✓, 12 ✓, 13 ✓, 14 ✓ — Phase 15 GC next; see per-phase §Status)*
1. Phase 11 ✓ — bytecode VM (ADR-11 decisions 1-7 locked 2026-05-30; retires `g_unwinding`, retires tree-walker stack overflow, supersedes 10.4.5 S5)
2. Phase 12 ✓ — image snapshot + load (`.bim`; pure-data save, no `cc` required)
3. Phase 13 ✓ — **logical prune** (fact-level reachability mark-sweep: AST/scope, history excise; AOT prune mode reuses). *Note: drops facts + clears object-table slots; does NOT free backing heap — that is Phase 15.*
4. Phase 14 ✓ — AOT (`birnam --aot=<static|dynamic> [--strip=malleable] [--strip=history]`; all six matrix cells driver-built; only the 12.5 mmap-relocation perf path deferred)
5. Phase 15 — **heap GC** — core SHIPPED + merged. Four-arena non-moving mark-sweep (boxes/captures/closures_vt/strings), `:gc` + growth-gated auto-trigger, FFI Sig free. Remaining standalone items: `index` epoch arena, const-pool RRB finalizer, AST-drop policy (see §shipped-vs-remaining). Distinct from Phase 13: 13 reclaims *facts*, 15 reclaims *bytes*.
5b. Phase 15.R — **Retention / Snapshots / Excision** (the temporal-database layer the persistent substrate was built for; depends on 15.I epoch arena). db-values as first-class pinning snapshots, retention horizon policy, explicit excision, on-disk cold-history tier. See Phase 15.R §spec. Distinct from Phase 13/15: 13 = which facts, 15 = which bytes, **15.R = which history, for how long, and where it lives**.
6. Phase 16 — Collections (language-level: list/vector proto + literal + `do:`/`collect:`/`inject:`/`size`; spec TBD)
7. Phase 17 — Standard Library (strings, printing/`displayString`, comparison, `doesNotUnderstand:`, core protocols; spec TBD)
8. Phase 18 — Native codegen (final: bytecode → machine code)

**ADR-11 — Phase 11/12/14 design decisions locked 2026-05-30:**
1. Bytecode encoding = bytestream + indexable annotations (queryable compiled artifact at tool-relevant granularity, fast dispatch)
2. Frame rep = native per-thread stack + on-demand reification (Phase 7.3 pattern at execution layer)
3. mcache = native Zig array, atomic 16-byte writes, race-tolerant reads
4. Dispatch = tail-call (`@call(.always_tail, ...)`), build-time `jmp`-not-`call` probe
5. Image shape = D3 hybrid `.bim` (data + manifest; AOT wraps same bytes as executable)
6. AOT strip = `--strip=malleable` (parser/AST/compiler/REPL) + orthogonal `--strip=history`
7. Multi-thread shape = Datomic (single-writer transactor + per-thread `Vm`); Phase 11 ships single-threaded with seven keep-viable items
- JIT explicitly out of scope (user direction). VM and AOT only.
- Naming locked: `birnam` = executable; `.brn` = source extension; `.bim` = image archive.

**ADR-11 addendum — pre-implementation review 2026-05-31:**
1. Blocks = own bytecode-method entities (separate `:bytecode-stream` per block). MAKE_BLOCK pool-idx → block-method-ref. INVOKE_BLOCK = standard frame push.
2. AST edge name = `:method-body-ast` (reused from Phase 10.4). Drop the ADR-11 #1 `:source-ast` synonym.
3. `:method-ast-range` = cardinality-many Ref to range-entity (`:range-method`, `:range-pc-start`, `:range-pc-end`, `:range-ast`). 13.4 sweep gains `:range-ast` edge.
4. Opcode set frozen at 16 ops (4 spare). Selectors = inline u32 EID in SEND_*. Constant pool = vec of Refs only. New annotations: `:method-send-site-count`, `:method-local-count`, `:method-arg-count`.
5. mcache reader = re-check-vt 3-load (vt1, method, vt2; accept iff vt1==vt2==target). Writer 16B atomic store defensive. Invalidation = walk-on-`:method-install` tx hook (mcache stays 16B per entry).
6. Metacircular `bind` = kernel `bind_default` Imp + optional `:vt-bind` slot looked up via raw EAVT (no send recursion).
7. Carry-overs: thread-mode tag in `manifest.txt` only (drop from roots.bin); `--strip=malleable` cascade-retracts `:method-body-ast` + `:method-ast-range` + range-entities; tail-call probe failure = build error (no fallback dispatch); selector intern runs under transactor; reify-stack is single-thread today, thread-safe variant deferred; frozen-layer LOC budget revised to ~2400 post-Phase-11; bytecode-format-version in manifest.txt.

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
| PR4 Phase 10 GC interim | **Manual only in Phase 10.** `:reset` (full top-level wipe) and `:prune` (Phase 13.4 logical mark-sweep over facts, user-triggered) debug-commands. Phase 13 lands the auto-policy: logical prune runs automatically after every REPL line eval once 13.4 ships. Risk #2 (REPL session creep) accepted for Phase 10 spike period. **Command-naming locked (2026-06-04):** `:prune` = logical (fact) reclaim; `:gc` reserved for Phase 15 **heap** reclaim — do not double-bind `:gc` to prune. |
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

**Goal:** Compile AST facts → bytecode facts; execute via VM. Lock thread/AOT-compatible substrate.

### ADR-11 — Decisions locked 2026-05-30

| # | Decision | Choice | Tenet check |
|---|----------|--------|-------------|
| 1 | Bytecode encoding | **C: bytestream + indexable annotations** — method entity holds `:bytecode-stream <byte-array-Ref>`, `:constant-pool <vec-Ref>`, `:source-ast <ast-eid>`, plus annotation facts (`:method-sends ?sel`, `:method-reads-slot ?slot-eid`, `:method-ast-range <pc> <ast-eid>`) regenerated each recompile. | Date 1NF: bytestream is single-value attr (Ref to byte array). Annotations are normal triples. COLA: substrate carries data + annotations; materialization (VM) walks bytestream. |
| 2 | Frame representation | **C: native per-thread stack + on-demand reification** — `Vm.frames: []Frame { pc, method_ref, self_ref, bp, locals_inline }` in bump arena. Reified via `(reify-stack)` API → transient frame entities (retracted on next reify). | COLA layering clean: native = materialization, reified = substrate. Date 1NF on reified entity. |
| 3 | mcache location | **B + i: native Zig array, atomic 16-byte writes** — per-method parallel `mcache[pc]` of `{ vt: Ref, method: Imp }`. Atomic store on fill, race-tolerant on read (mismatch → fall through to `bind`). | Materialization only. -1 vt trick + mcache co-exist (different fast paths). |
| 4 | Dispatch style | **C: tail-call (`@call(.always_tail, ...)`)** — each opcode = own handler fn with variadic-β-ABI shape, ends with tail-call into next handler via dispatch table. Build-time probe asserts Zig lowers to `jmp` not `call` on x86_64 + arm64. | Pure Zig in frozen kernel. No inline asm. Multi-thread safe (stateless handlers, `Vm*` carried). |
| 5 | Image shape | **D3 hybrid: `.bim` archive (roots.bin + segments/ + manifest.txt) for live save; AOT wraps same data bytes as executable shell.** Pure data write — no `cc` needed at save time. | Substrate identity across modes preserved. |
| 6 | AOT strip set | **`--strip=malleable` (drops parser/AST/compiler/REPL; keeps `method:do:` via dylib API per `--aot=dynamic`) + orthogonal `--strip=history` (collapses to latest tx, drops `tx`/`op` columns, RO indexes).** Two flags, four combinations. | Substrate untouched even at max strip; materialization (annotations, AST, parser) optionally removed. |
| 7 | Multi-thread shape | **C: Datomic — single-writer transactor + concurrent readers + per-thread `Vm`.** Seven keep-viable items (see 11.7). Phase 11 ships single-threaded; design choices forbid single-thread-only state. | VISION §290 alignment. Persistent indexes already lock-free for readers. |

### Work items

11.1 **Bytecode encoding (ADR-11 #1, addendum-locked).** Define `:bytecode-stream` attr (Ref to materialized byte array in bytecode arena per 12.5). Define `:constant-pool` attr (Ref to RRB vec of Refs only — number/string literal entities, block-method Refs, symbol entities; selectors NOT in pool). Method entity also carries `:method-body-ast <ast-eid>` (reused from Phase 10.4 — provenance + 13.4 sweep root).

Opcode set (16, addendum-locked):

| Opcode | Operands | Size | Notes |
|---|---|---|---|
| `PUSH_CONST` | pool-idx u16 | 3 B | non-smallint literals via pool |
| `PUSH_SMALLINT_I32` | i32 | 5 B | inline smallints, skip pool |
| `PUSH_LOCAL` | slot u8 | 2 B | local/arg slot |
| `STORE_LOCAL` | slot u8 | 2 B | |
| `POP` | — | 1 B | |
| `LOAD_SELF` | — | 1 B | |
| `LOAD_CONST_TRUE` / `FALSE` / `NIL` | — | 1 B each | dedicated, no pool hit |
| `SEND_0` | sel-eid u32, site-idx u16 | 7 B | unary |
| `SEND_1` | sel-eid u32, site-idx u16 | 7 B | binary |
| `SEND_N` | sel-eid u32, arg-count u8, site-idx u16 | 8 B | keyword |
| `RETURN` | — | 1 B | from current frame |
| `RETURN_FROM_METHOD` | method-frame-depth u8 | 2 B | non-local |
| `JUMP` | offset i16 | 3 B | |
| `BRANCH_IF_FALSE` | offset i16 | 3 B | |
| `MAKE_BLOCK` | pool-idx u16 | 3 B | pool slot = block-method Ref |
| `INVOKE_BLOCK` | arg-count u8, site-idx u16 | 4 B | block frame push |

Annotation facts (compiler-emitted, recompile-retracted):
- `:method-sends ?sel` — cardinality-many; selectors this method sends
- `:method-reads-slot ?slot-eid` — cardinality-many; slot reads
- `:method-ast-range ?range-eid` — cardinality-many; debugger source mapping
- Each `range-eid` entity: `:range-method`, `:range-pc-start u32`, `:range-pc-end u32`, `:range-ast <ast-eid>`
- `:method-send-site-count u16` — mcache sizing
- `:method-local-count u8` — frame sizing
- `:method-arg-count u8` — caller arg-pop count

Block-method entities (addendum #1): each source `[: args | body]` compiles to its own method-entity with `:bytecode-stream`, `:constant-pool`, `:method-body-ast`, the annotations above, plus `:block-parent-method <method-eid>` (for `RETURN_FROM_METHOD` static depth resolution).

11.2 **Compiler.** Birnam module that walks AST facts, emits bytecode bytestream + constant pool + annotation facts in one tx. Reuses Phase 10.4.5 scope chain — `AST_VAR_REF_VT` lowered to `PUSH_LOCAL <slot>` resolving against compile-time scope walk (this also retires the Phase 10.4.5 S5 "superseded" status — slot resolution moves from never-implemented tree-walker to compiler pass).

11.3 **VM interpreter (ADR-11 #4).** Zig kernel: per-opcode handler fns with signature `fn(*Vm, [*]const u8 bytecode, usize pc) callconv(.c) Ref`. Dispatch table indexed by opcode byte. Tail-call lowering verified by build-time probe. `Vm` struct per ADR-11 #2.

11.4 **mcache (ADR-11 #3, addendum-locked).** Per-bytecode-method `mcache: [N]MCEntry` where `N = :method-send-site-count`. Lazy-allocated in bump arena on first method execution. `MCEntry { vt: Ref, method_impl: Imp }` = 16 bytes, 16-byte aligned.

Writer (mcache fill on miss): `@atomicStore(u128, &entry.full, pack(vt, imp), .release)`. Defensive — x86_64 `movdqa` aligned is atomic; arm64 `stp` may tear pre-8.1; reader catches either way.

Reader (3-load re-check-vt):
```
vt1 = atomicLoad(entry.vt, .monotonic)
imp = atomicLoad(entry.imp, .monotonic)
vt2 = atomicLoad(entry.vt, .monotonic)
if (vt1 == receiver_vt and vt1 == vt2) tail-call imp(...)
else fall through to miss-handler
```

Miss-handler (metacircular bind, addendum #6): raw EAVT lookup of `[receiver-vt :vt-bind ?custom]`. If present, invoke as bytecode method (push frame, run, capture Imp from return). Else call kernel `bind_default` Imp directly. Result fills mcache, dispatches.

Invalidation on recompile (addendum #5): `:method-install` tx commit hook walks all live methods' mcaches; entries with `method_impl == old_impl` zeroed via 16B atomic store. Methods currently executing on stack keep running old Imp until RETURN (Smalltalk-80 semantics).

11.5 **Non-local return retirement (Phase 10.3 deferred).** Per ADR-10-E4: tree-walker `g_unwinding` global retired. Phase 11 replacement:
- `RETURN_FROM_METHOD <method-frame-id>` opcode unwinds VM stack to named method frame, drops intermediate block activations, resumes with value.
- Compiler resolves `^expr` to this opcode with static method-frame-depth via Phase 10.4.5 scope chain walk.
- Escaped-block edge case: VM checks call-stack for active matching frame; absent → emit ERROR_VT entity (`:error-kind :return-from-dead-method`), matching tree-walker diagnostic.
- `unwinding` flag moves from global → `Vm.unwinding` (per-thread by construction).

11.6 **Reify-stack API.** `(reify-stack vm)` materializes current `Vm.frames` to transient entities with `:frame-pc`, `:frame-method`, `:frame-self`, `:frame-parent`. Retracted on next reify or debugger exit. Builds on Phase 7.3 lazy-fact-EID pattern.

11.7 **Multi-thread keep-viable items (ADR-11 #7).** All shipped in Phase 11 even though execution stays single-threaded:
- **i.** `Vm` struct holds all per-thread state (`frames`, `pc`, `current_self`, `unwinding`, `recursion_guard`, `ffi_pause`, `bump_arena_slot`). No new globals.
- **ii.** `g_unwinding` global retired (subsumed by 11.5).
- **iii.** Existing recursion guard + FFI pause counter migrate from globals to `Vm` fields.
- **iv.** EID allocator + tx counter become `std.atomic.Value(u64)`. No-op on single-thread; forward-compat.
- **v.** Tx commit path consolidates to single function (`tx_engine.commit(...)`). Single-threaded today; channel-receiver tomorrow without scattered changes.
- **vi.** Selector intern table: single-writer + atomic-published root pointer. Readers lock-free.
- **vii.** Image manifest carries `mode: single|datomic` tag. Default `single`. Forward-compat to transactor introduction without format break.

11.8 **Annotation facts as compile-time emit.** Compiler emits `:method-sends`, `:method-reads-slot`, `:method-ast-range` in same tx as bytestream. On recompile: prior annotations retracted, fresh set asserted.

11.9 **Build-time tail-call probe.** Build step compiles probe with two opcode handlers, disassembles output, asserts final instruction is `jmp` not `call` on target arch. CI gate. **Probe failure = build error, no fallback dispatch** (ADR-11 addendum). Port-blocker by design: an arch where Zig can't tail-call requires kernel re-architecture before that arch is supported.

11.10 Tests:
- VM-vs-tree-walker parity across full Phase 10 test suite (~50 tests).
- Bytecode encoding round-trip: AST → bytecode → execute = AST → tree-walk.
- mcache atomic fill correctness (single-threaded micro-bench + race-tolerant property).
- `^` escape-detection (matching method frame absent → ERROR_VT entity).
- Reify-stack produces correct frame chain.
- Annotation facts query: `[?m :method-sends #foo]` returns all methods sending `#foo`.
- `Vm` struct passes through all per-thread state without globals (grep test).

### Reasoning
VM gives portability + AOT. **C-shape bytecode (bytestream + annotations)** keeps execution fast while preserving "compiler is queryable" property at the granularity that matters for tooling (which methods touch what). **Native per-thread `Vm` + atomic counters** unlocks the Datomic multi-thread future without committing to it now. **Tail-call dispatch in pure Zig** keeps frozen-kernel boundary clean (no inline asm). All seven decisions cross-checked against Date 1NF + COLA layering + VISION §290 multi-thread + VISION §296 frozen layer.

**Accepted costs:**
1. **No in-place opcode patching.** Live-patch of single opcode requires full-method recompile (bytestream is per-method Ref). Smalltalk doesn't do this either; annotation set still mutable for tooling.
2. **Reified frames stale immediately.** Debugger must re-reify each step. Same pattern as Phase 7.3 rule walk-back.
3. **mcache lost on image reload.** Rebuilds lazily on first send-site execution per method (Phase 12.3 already specified).

---

## Phase 12 — Image Snapshot (`.bim`)

**Goal:** Save and reload entire image state. Pure data format — no `cc` required at save.

### Decisions locked (ADR-11 #5)

- **Extension:** `.bim` (avoiding `.bri` per personal-name overlap).
- **Shape D3:** archive used for Mode 1 live save AND as data input to AOT pipeline. Same bytes underneath both.
- **Save is pure write:** segments mmap-flushable, manifest is text, roots are fixed-layout binary. No toolchain dependency.

### Framing decision (2026-06-02) — callable representation & the fn-ptr serialization boundary

**Forced by serialization analysis.** The object table (materialization layer) holds every materialized entity as `mkPointer(addr)` — a raw heap address. This is correct: it is a *cache*, and facts are canonical (the locked vision). On load, the cache is rebuilt from facts; nothing in the object table is serialized verbatim. Every entity obeys one invariant: **the materialized struct is pure cache; its truth lives in facts.** A boxed float caches an `f64` that is also a fact; a string, an RRB node — same. Throw the struct away, rebuild from facts.

**One entity violates the invariant: the closure.** `Closure { method: *const anyopaque, data: Ref }` (`closure.zig:16`) holds a **raw function pointer** in `.method`, and that pointer is *not* a fact and *not* derivable from facts. It is the only materialized state that cannot be rebuilt from the fact store. It is also the one pointer class that points *outside the image* — into a dylib's `.text` — so it cannot be relocated by mmap + pointer-patch the way intra-image arena pointers can. It must be symbolically relinked against the freshly-`dlopen`'d module, or avoided.

**Root cause is a representation choice, not the `-1` trick.** The `-1` trick (vt_ref at offset −1, `send.zig:190`) is pure class-lookup and is orthogonal — it works for any body shape. The misfit is that Phase 11.12 dressed *every* callable in the same `Closure{fnptr, data}` costume so all dispatch funnels through one fn-ptr call (`send.zig:235`). That conflates two fundamentally different kinds of callable:

| kind | behavior lives in | `.method` fn ptr is… |
|---|---|---|
| **native primitive** (e.g. `smallintPlusImpl`) | dylib `.text` (machine code) | **irreducible** — there is no data form, and there should not be |
| **bytecode method** (user `[vt method:#sel do:[…]]`) | **facts** (`:bytecode-stream`, `:constant-pool` — 11.8) | **redundant** — a dispatch shim over data already canonical |

For native primitives, the unserializable fn ptr is *correct and inherent* — native code cannot be data. For bytecode methods, the fn ptr (`&bytecodeDispatcherImpl`, `dispatch.zig:338`) is an artifact of the uniform representation; the real content (`method_eid` → bytecode facts) is fully serializable.

**Decision (chosen 2026-06-02) — Approach B: rematerialize closures from facts on load. Representation unchanged at runtime.**

The fn-ptr-in-`.method` problem is a *serialization* problem, not a runtime one. A bytecode method already stores its behavior as facts (`:bytecode-stream`, `:constant-pool`, … on `method_eid` — `compiler.zig:514`); the fn-ptr Closure `{&bytecodeDispatcherImpl, mkEid(method_eid)}` is a *cache* over those facts (`compiler.zig:553`). So we never serialize the fn ptr — we rebuild it on load:

- Closures are **excluded from the persisted struct arena** (12.1). They are not pointer-patched.
- `12.3` loader, after `dlopen`: (a) re-run each module's primitive-install so the ~60 native fn-ptr closures (54 lang + 6 query + 3 repl) exist at known eids; (b) for every entity carrying `:bytecode-stream`, re-run the materialize tail to re-mint its fn-ptr Closure against the freshly-loaded module. User bytecode methods + at-rest block closures round-trip as **plain facts**; the fn ptr is reconstructed, never stored.
- **No hot-dispatch change.** `bind`/`sendN`/mcache and the `CLOSURE_VT` `value`/`whileTrue:` path stay exactly as 11.12 shipped.

This keeps Phase 12 minimal and low-risk: the non-serializable surface collapses to "things rebuilt by running module code on load," which native primitives already need anyway.

**Deferred — Approach A (future cleanup, own work item): bytecode methods as a distinct entity kind.** Introduce `BYTECODE_METHOD_VT`; lift the isEid/isPointer branch out of `bytecodeDispatcherImpl` (`dispatch.zig:366`) up to the `bind`/`sendN` boundary (`send.zig:234`); enter the VM directly for compiled methods; stop minting fn-ptr closures for the *method* case (`allocClosureEntity`, `dispatch.zig:330`). Removes the native/data conflation at the representation layer and simplifies Phase 14 AOT codegen (methods emit as data, no fn-ptr reversal). Touches hot dispatch + mcache `*Closure` rep + the `value`/`whileTrue:`-on-`CLOSURE_VT` path, so it earns its own design+commit pass rather than riding Phase 12. **Not required for image round-trip — B already gets that.** Revisit before/with Phase 14.

**Amends 12.1 / 12.3 below** per Approach B.

### Format refinement (2026-06-02) — single self-describing file, manifest embedded

ADR-11 #5 sketched `.bim` as a *directory archive* (`roots.bin` + `manifest.txt` + `segments/`). Revised: **one self-describing file** with the manifest as an embedded header **section**, not a sidecar. Rationale: the snapshot is already a single byte stream; one file = atomic write, no multi-file consistency to manage, still "pure data, no toolchain." Layout (format v4, shipped in `image.zig`):

```
[MAGIC 8][format_version u32]
[manifest: bytecode_format_version u32, thread_mode u8,
           module_count u32, {name_len u32, name bytes, abi_hash u64}*]
[counters: next_tx u64, next_eid u64]
[facts: count u64, {m,e,a, value_kind u8, value...}*]
```

This does **not** preclude the future mmap fast-path (12.5): make sections page-aligned and add an offset table to the header — still one file. `manifest.txt` as a separate artifact is dropped; the thread-mode tag (ADR-11 addendum) and bytecode-format-version live in the embedded manifest. `image.load` returns the parsed `Manifest` (module deps + names allocated in the store allocator) so the loader can `dlopen`+verify before replaying facts.

**Open detail for the format spec (not a blocker):** block-closure captures are raw `Ref`s copied from locals (`dispatch.zig:242`); a capture may be a pointer (boxed float / scope entity). Persisted captures must be fact-backed eids (scope/binding entities are — 10.4.5), or the capture's referent must itself rematerialize from facts. Resolve when specifying capture serialization. **Snapshot is only coherent at rest** (`vm.frames` empty — `parent_frame_id` is a live-stack id); assert this at save.

**Orthogonality note (corrects a prior read):** finishing the lang switchover / retiring `g_unwinding` (11.5) does *not* remove the fn-ptr problem — `.method` holds a fn ptr regardless of which dispatcher it points at. Phase 12 serialization is independent of 11.5; do not gate one on the other.

### Partition decision (2026-06-02) — provenance split, image persists data only

**Problem found:** the module (`m`) column does not separate user data from module code. Each module's runtime-assigned `g_module_eid` (`module.zig:63`) tags *both* its code facts (primitive installs, grammar rules, kind-vt wiring — reconstructed by re-running init) *and* user data facts (parsed AST, scope/binding entities, REPL state — must persist). User compiled methods use a third tag, fixed `M_LANG` (`id_api.zig:373`). No clean line exists; reachability-from-roots doesn't escape it (distinguishing a user vt-method override from a primitive still needs provenance).

**Latent bug it also exposes:** `retractAllOf(module_eid)` (`module.zig:121`) retracts *every* fact under a module eid on unload — including user data sharing `g_module_eid`. Unloading lang would delete user state.

**Decision (full split):** introduce well-known **`M_USER`** (fixed kernel eid). Classify by **assert call-context**, not by attribute:
- **CODE** (stays under the code-module eid; rebuilt every boot by `dlopen`+init): any fact asserted *inside module init*. May ride the unstable runtime `g_module_eid` — it is reconstructed, never replayed, so a fresh eid each boot is fine.
- **DATA** (→ `M_USER`; persisted by the image): any fact asserted *in a runtime Imp path* (parse, eval, `method:do:` install, `:=`, bind, REPL mutation). Must ride the stable `M_USER` eid so the image finds it and it survives a module-eid reassignment.

**Image rule: persist `M_USER`-tagged facts only.** Loader: boot kernel + `dlopen`+init (rebuilds all CODE) → replay `M_USER` facts → Approach-B rematerialize closures. Cross-references from DATA to CODE entities resolve because they point at stable kernel eids (kind-vts, kernel vts) or other persisted `M_USER` entities; the rare DATA→runtime-module-eid reference (e.g. REPL `:target` on the repl module's own eid) is session state, not persisted.

**Bonus:** the split fixes the `retractAllOf` unload bug — unloading a code module no longer touches `M_USER` data.

### Work items

12.0 **Provenance split (prerequisite).** Add `M_USER`; sweep every runtime-data assert across lang/peg/repl/query from `g_module_eid`/`M_LANG` to `M_USER` (parsed AST, scope, bindings, user methods, counters), leaving init/code installs under code-module eids. Image writer filters to `m == M_USER`. Verify unload no longer retracts `M_USER` data.

12.1 **Image format.** Archive (directory or single concatenated file):
- `roots.bin` — EAVT/AEVT/AVET/VAET root pointers + tx counter + EID allocator state + selector-table-head Ref. (Thread-mode tag NOT here — manifest.txt only, per ADR-11 addendum.)
- `segments/` — arena segments, mmappable (HAMT/RRB pages, materialized struct arena slabs). **Closures excluded — rematerialized from facts on load, not persisted (see Framing decision 2026-06-02).**
- `strings/` — dedicated string arena (split from 12.5 below).
- `bytecode/` — bytecode bytestream arena (split for AOT RO segment future).
- `manifest.txt` — dylib dep list with platform-native names, version tags, ABI hashes; thread-mode marker (`mode: single|datomic`); image format version; bytecode-format version (separate counter — opcode set evolution).

12.2 **Snapshot writer.** Walk persistent index roots, identify dirty segments since last snapshot, write only those. Manifest regenerated each snapshot.

12.3 **Snapshot loader.** Parse manifest → `dlopen` each module by name (verify ABI hash on load) → **re-run each module's primitive-install so native fn-ptr closures exist at known eids (Framing decision 2026-06-02, Approach B)** → mmap segments → patch root pointers → **rematerialize every `:bytecode-stream` entity's fn-ptr Closure from its facts** → fresh mcache (rebuild lazily per Phase 11 ADR #3). User bytecode methods + block closures rebuild from facts; the fn ptr is never serialized.

12.4 **Selector + entity identity across reload.** Kernel selectors at well-known EIDs (already done — `kernel_eids.zig` range 0..255). User selectors re-intern via relocation table embedded in manifest (old-EID → new-EID).
- **Finding (2026-06-02, from `:load` smoke):** the same unstable-eid hazard hits the **lang top-level scope**. REPL top-level bindings are persisted M_USER data, but they reference the lang module's `g_top_scope_eid`, which is allocated fresh each session — so a reloaded binding dangles against the new session's scope. Fix options: (a) give the top-level scope a **stable well-known eid** (kernel constant, like `M_USER`) so bindings re-attach automatically; (b) relocation-remap scope eids on load alongside selectors. (a) is simplest and mirrors how the method-on-kernel-vt case already round-trips (stable target eid). Methods installed on stable kernel vts work today; user bindings + methods-on-user-vts need this fix.

12.5 **Arena split.** Strings, bytecode, regular fact-store segments live in separate arenas → separate mmap segments → faster load, page-cache-friendly, RO-segment-ready for AOT.

12.6 **Incremental save policy (note for future implementation, gated on perf measurement).** Once 12.1-12.5 ship, layer policy on top:
- `manual` — explicit `:save-image` call only (Phase 12 default).
- `per-tx` — every committed tx triggers segment flush. Worst write-amplification; debugging crash-safety.
- `per-gc-sweep` — snapshot after each Phase 13 sweep. Memory-stable checkpoints.
- `per-n-tx` — every N txs. Tunable middle ground.
- `idle-fsync` — background snapshot when tx queue idle.

Implementation deferred — needs real workload to pick defaults. Policy lives as fact-store config entity; switchable at runtime.

12.7 Tests: save → kill → load → run → identical behavior; strings segment mmap-able; bytecode segment mmap-able; manifest ABI hash mismatch rejected at load; thread-mode tag round-trips. **Closure round-trip: install user method → save → kill → load → send → identical result (fn-ptr Closure rebuilt from facts, not from saved bytes).**

12.8 **(Deferred — Approach A cleanup, own commit) Bytecode methods as `BYTECODE_METHOD_VT`.** Not required for image round-trip (12.3 Approach B handles that). Lift isEid/isPointer branch out of `bytecodeDispatcherImpl` to the `bind`/`sendN` boundary; enter VM directly for compiled methods; retire the fn-ptr indirection for the method case. Removes native/data conflation; simplifies Phase 14 AOT codegen. Earns its own design pass (hot dispatch + mcache rep + `value`/`whileTrue:`-on-`CLOSURE_VT`). Revisit before/with Phase 14.

### Reasoning
`.bim` makes birnam a real Smalltalk-style environment vs. a script that rebuilds state every run. **D3 hybrid** unifies live save and AOT input — same bytes underneath. **Pure data write** (no `cc`) makes save cheap, no toolchain pin. **Arena split** sets up Phase 14 RO segments. **Incremental save policy noted but not specced** — needs perf data first.

### Status (2026-06-02) — core complete

**Shipped (`src/kernel/image.zig`, format v5, single self-describing file):**
- 12.0 provenance split (`M_USER`) ✓ — DATA persisted, CODE reconstructed; fixes unload bug.
- 12.1 value codec ✓ — eid/smallint/keyword/nil, string, float, recursive RRB, and `:method-captures-ptr` (CaptureSpecArray, attribute-aware). Embedded **manifest** section + **interner** section. On-disk `saveToFile`/`loadFromFile`.
- 12.3 loader ✓ (Approach B) — `rematerializeClosures` rebuilds fn-ptr closures from `:bytecode-stream` facts; shell `:save`/`:load` commands wired (shell builds manifest from loaded modules).
- 12.4 identity ✓ — stable `TOP_SCOPE_EID` (185) + interner persistence so user names/bindings keep eids across reload.
- **Proven**: kernel e2e (`^42` survives save→reboot→dispatch); lang-driven REPL-binding regression test (`let x := 41` survives fresh-boot reload); verified through the real `birnam` binary.

**Deferred (intentionally — not blocking the core round-trip):**
- **ABI-hash verify (12.3).** Manifest carries an `abi_hash` field (placeholder 0 today). NOT enforced: a dylib *content* hash would false-reject after any recompile; a correct check needs a module-exported **ABI-surface** hash (stable across compatible rebuilds), which is its own mechanism. Field stands ready; verification lands with that mechanism.
- **mmap fast-path / arena split (12.5).** Format is replay-based today (correct, simple). mmap is a perf optimization — gated on perf data per this section's own reasoning. Single-file layout already supports it later (page-aligned sections + offset table).
- **Incremental save policy (12.6).** Deferred until a real workload picks defaults.

### Reasoning (Approach A revisit)
Approach A (bytecode methods as `BYTECODE_METHOD_VT`, removing the fn-ptr indirection — see Framing decision) remains deferred to item 12.8 / Phase 14. Not needed: Approach B round-trips everything, proven.

---

## Phase 13 — Pruning

**Goal:** History excise + entity reachability sweep.

### Work items

13.1 Pruning policies: keep-last-N-txs, reachable-from-roots, manual-excise-of-named-entities.

13.2 Pruning runs as a separate tx that rewrites indexes excluding pruned facts. Old index roots become unreferenced and GC-able.

**Roots/edges re-derivation (2026-06-02) — bytecode-closure + provenance model.** The original 13.3/13.4 (below, struck) were written in the Phase 10.3 tree-walker era and reference representations Phase 11.11/11.12 + 12.0 superseded: `:block-scope`/`:block-body`/`BLOCK_VT` (gone — blocks are bytecode-method entities referenced via constant-pool + `:block-parent-method`), `:bind-scope` (the live model is the `:scope-first-binding`/`:bind-next` head-chain), and grammar-rule/kernel-method roots (now CODE, reconstructed every boot — not garbage). Three simplifications fall out: execution uses frame locals + `CaptureSpec` so there are **no per-block scope entities to GC**; **prune scope = M_USER entities only** (CODE is rebuilt by module load); AST is debug-only now (bytecode is canonical), so orphan-AST sweep is hygiene, not correctness.

13.3 **Scope/binding reachability (re-derived).** The only persistent scope is the top-level REPL scope at the stable `TOP_SCOPE_EID` (12.4a) — always a live root. Walk its binding chain and mark:
- `[TOP_SCOPE_EID :scope-first-binding ?b]` then `[?b :bind-next ?b2]*` — the live REPL bindings (head-chain, not `:bind-scope`)
- each binding's `[?b :bind-value ?v]` roots `?v` — if `?v` is `mkEid(?m)` with `[?m :bytecode-stream]`, that method (and its closure) is live; follow into 13.4's method walk
- `[?s :scope-parent ?p]` / `[?s :scope-self ?v]` — only if transient nested SCOPE_VT entities persist (tree-walker path); chain to parents
A binding shadowed by a later `:=` (replaced head) leaves the old binding entity unreachable → swept. Tests: live-binding-kept, shadowed-binding-swept, binding-holding-a-method keeps the method, redefined-binding drops the old value's method if otherwise unreferenced.

13.4 **Entity mark-sweep over M_USER (re-derived).** Mark every transitively-reachable M_USER entity from the roots below; sweep unmarked M_USER entities (orphan AST, dead methods/blocks, dead range/scope/binding entities). CODE entities are out of scope.

Roots:
- **Installed methods** — M_USER facts `[?vt ?sel mkEid(?m)]` where `[?m :bytecode-stream]`. `?vt` may be a stable kernel vt or a user vt. (Replaces the `:method-body-ast` root: a method is live because it's *dispatchable*, not because it has an AST.)
- **REPL bindings** — the 13.3 chain from `TOP_SCOPE_EID`.
- **Saved queries** — `[?s :saved-query-spec ?]`.
- **FFI wraps (M_USER only)** — `[?w :ffi-sig ?]`; the `:ffi-sig` value is a pointer to a heap Sig struct → needs a Zig-side free callback on sweep. (Module-installed wraps are CODE — skipped.)
- (Dropped vs old spec: grammar-rule patterns and kernel/primitive methods — both CODE, reconstructed each boot.)

Forward edges (transitive mark):
- bytecode method → `:constant-pool` (RRB): for each element that is `mkEid(?c)` with `[?c :bytecode-stream]`, mark `?c` (block child — recurse). Literal pool entries (strings/floats/smallints) are owned data.
- bytecode method → `:method-body-ast` → AST subtree (debug); → `:method-ast-range` → range entity → `:range-ast` → AST node.
- AST subtree edges: `:ast-first-child`, `:ast-next-sibling`, `:ast-stmt-first`, `:ast-receiver`, `:ast-first-arg`, `:ast-block-body`, `:ast-block-params`, `:ast-let-init`, `:ast-assign-target`, `:ast-assign-value`, `:ast-return-value`, `:ast-method-frame`, `:ast-name`/`:ast-value` (only follow eid values that are themselves swept-class entities; `:ast-selector` → interned selector, **not** swept).
- `:block-parent-method` is a **back-edge** (non-local-return target) — NOT a mark edge.
- `:method-captures-ptr` → heap CaptureSpecArray (owned data, no entity edge).

Tests: installed-method-kept + its block children (via pool) + its AST; orphan-AST-pruned (parsed, never installed); redefined-method drops the old method_eid; M_USER-only (a CODE grammar-rule entity is untouched by the sweep); FFI-sig free callback fires once on sweep.

---

<details><summary>Superseded 13.3/13.4 (Phase 10.3-era, pre-bytecode — kept for history)</summary>

13.3 (old) Closure-scope reachability over `[?b :vtable BLOCK_VT][?b :block-scope ?s]` + `:scope-parent`. Dead: blocks are no longer BLOCK_VT entities with `:block-scope` Refs.

13.4 (old) AST mark-sweep rooted at `:method-body-ast`, `[?b :block-body ?]`, `:saved-query-spec`, `:rule-pattern`, `[?b :bind-scope ?s]`, `:ffi-sig`. Stale roots: `:block-body`, `:bind-scope`, and the grammar-rule root (CODE).

</details>

13.5 Tests: pruning preserves reachable state, removes excised entities, query results match pre-prune for non-excised data. Plus the closure-reachability cases above and the AST mark-sweep cases.

### Status (2026-06-02) — reachable-from-roots shipped

**Shipped (`src/kernel/prune.zig`, `FactStore.compact`, shell `:prune`):**
- 13.2 mechanism ✓ — `compact(keep)` rebuilds all five indexes from current facts keeping the accepted set; history collapses; old persistent roots GC. Suppresses demote-on-mutate during re-assert so kept materialized closures survive.
- 13.1 **reachable-from-roots** policy ✓ — the primary policy. 13.3 (scope/binding chain from `TOP_SCOPE_EID`) + 13.4 (M_USER entity mark-sweep: roots = installed methods + their install subjects, REPL bindings, saved queries, FFI wraps; conservative forward eid-edge walk) folded into `prune.pruneReachable`. CODE never swept; object-table slots for swept entities cleared.
- 13.5 tests ✓ — orphan-AST swept; redefined-method drops old + clears slot; binding chain + bound method kept; CODE-never-swept; demote-suppression.

**Deferred:**
- 13.1 other policies — **keep-last-N-tx** (compact currently collapses *all* history to current = "keep-current-only"; granular N-tx retention needs per-fact history replay) and **manual-excise-named-entities**.
- **FFI Sig free callback ✓ (Phase 15.E, 2026-06-04).** Resolved as a targeted eager free (`ffi.freeSig`) at wrap re-resolve + module-unload, not a prune/sweep hook — see Phase 15.E. The original framing (free on prune sweep) was wrong: wraps are prune roots and never swept, and the Sig's internal `args` slice isn't Ref-reachable so it can't be arena-swept anyway.
- Mark precision — currently conservative "follow all eid-valued facts" (safe over-marking). Tighten to the typed edge set (constant-pool/`:method-body-ast`/`:range-ast`/scope chain) if a workload shows retained garbage.

### Reasoning
History grows forever otherwise. Pruning must be user-initiated (don't surprise the user with lost history). AOT prune (most aggressive variant) reuses this machinery. The closure-reachability sweep is the substrate's GC-by-reachability — Phase 10.3 accepts spike-level scope leak; Phase 13 cleans up via reachability walk over the fact graph.

---

## Phase 14 — AOT

**Goal:** Ship standalone executables. Two link modes × two strip flags = four sensible artifacts.

### Decisions locked (ADR-11 #5 + #6)

CLI: `birnam --aot=<static|dynamic> [--strip=malleable] [--strip=history] <input.bim> --entry=<module>:<selector> -o <output>`

| `--aot=` | `--strip=` | Result |
|----------|------------|--------|
| static | (none) | Single executable, kernel + all modules + data linked in, full malleable tower kept (compiler, parser, AST present and runnable). |
| static | malleable | Single executable, no parser/AST/compiler/REPL. Pure VM over bytecode. `method:do:` not exposed. |
| static | malleable + history | Same as above, plus `tx`/`op` columns stripped, RO indexes, smallest possible binary. End-user "ship the app" target. |
| dynamic | (none) | Executable + dylibs loaded at runtime per baked manifest. Full malleable tower. |
| dynamic | malleable | Executable + dylibs. Parser/compiler gone but `method:do:` survives — runtime-loaded dylibs can install bytecode methods via API. |
| dynamic | malleable + history | Executable + dylibs, no source compile, no history. Runtime method install still works. |

### Work items

14.1 **AOT compiler driver.** Reads `.bim`, validates entry selector exists, emits Zig source `aot_image.zig` containing:
- Baked `roots.bin` as `const ROOTS: [...]u8 = .{ ... };` (or `@embedFile`).
- Baked segments as RO arrays.
- For `--aot=static`: linked-in module init calls.
- For `--aot=dynamic`: embedded `manifest.txt` for runtime `dlopen`.
- `main()` that bootstraps kernel from baked data, calls entry selector.
Final step: invoke `zig build-exe` to link kernel + emitted source + (static-only) module dylibs.

14.2 **`--strip=malleable` implementation.**
- Skip serializing entities tagged `:ast-parse ?p` (AST nodes), `:rule-pattern ?` (PEG grammar), `:pattern-kind ?` (PEG patterns), `:method-body-ast ?` (source back-refs), `:method-ast-range ?` + reachable range-entities (ADR-11 addendum cascade — debug-only, dangling without AST).
- Skip baking parser/compiler/REPL module dylibs (`--aot=static` link list excludes them).
- For `--aot=dynamic` + `--strip=malleable`: skip parser/compiler/REPL from manifest; keep `method:do:` selector exposed for runtime dylib-load API.
- Reachability pre-pass (reuses Phase 13.4 mark-sweep machinery) — anything only reachable from stripped roots gets dropped.

14.3 **`--strip=history` implementation.**
- Collapse all history into latest-tx snapshot (reuses Phase 13.1 pruning machinery — "AOT prune" mode).
- Drop `tx`/`op` columns from index segments; freeze indexes as RO.
- Skip baking tx engine writer path; baked tx counter at final value.
- Compile-time assert: no live entity carries `:history-required` flag.

14.4 **Static link mode.**
- Link kernel + all listed module dylibs as `.a` static libs (or compile from source; Zig handles either).
- Resulting executable has no dynamic deps beyond libc/libSystem.

14.5 **Dynamic link mode.**
- Kernel + baked data linked in.
- Module list loaded from embedded manifest at startup via `dlopen`.
- Allows post-ship plugin dylibs (user can drop new `.dylib` next to binary, kernel `dlopen`s on demand if `method:do:` exposed).

14.6 **Manifest embedding (dynamic mode).** Embedded text section: dylib search paths, dep list with ABI hashes, runtime `dlopen` policy (eager vs lazy).

14.7 Tests:
- `--aot=static` builds Phase 10 hello-world, runs, no dylib deps (`otool -L` clean except libSystem).
- `--aot=dynamic` builds, loads dylibs at startup, runs.
- `--strip=malleable` removes parser symbols (`nm` check); attempting `[lang parse: ...]` in resulting binary fails as expected.
- `--strip=history` removes tx writer symbols; binary cannot mutate facts.
- `--strip=malleable` + `--aot=dynamic`: runtime-loaded dylib successfully installs a method via `method:do:` API.
- Same Phase 10 test workload run under each combination produces identical user-visible output.

### Re-spec (2026-06-02) — align to what Phases 12/13 shipped

The CLI/matrix above stays locked. The work-item *mechanism* (14.1–14.7) was written against the pre-Phase-12 archive sketch (`roots.bin` + `segments/` + `manifest.txt` + RO mmap) and before the load path existed. Reality is simpler and partly already built. Supersedes the mechanism bullets:

**M1 — AOT driver = embed `.bim` + reuse the boot/load path (replaces 14.1's roots.bin/segment baking).** Phase 12 ships a single self-describing `.bim` (embedded manifest + interner, replay-based, no `roots.bin`/segments). So the emitted `aot_image.zig` is small:
- `@embedFile("app.bim")` the snapshot bytes (RO `const`);
- `main()`: construct store/table/interner → `bootstrap.boot` → run kept module inits (see M3) → `image.load(store, BYTES, interner)` → restore `next_eid` → `image.rematerializeClosures` → send the entry selector.
- `zig build-exe` links kernel + emitted source + (static) the kept module sources.
All four of those load steps already exist and are tested. RO mmap segments (old 14.1) depend on the deferred 12.5 — **not required**; replay-load works.

**M2 — `--strip=malleable` is two distinct strips, not one.** The old 14.2 conflated them:
- **Code strip** (don't link/run parser + compiler + tree-walker eval): a module-init tiering problem — see M3.
- **Data strip** (drop the user's parsed AST trees, which are M_USER data): reuse the Phase 13.4 mark-sweep as a pre-bake prune that treats AST roots as dead. `prune.compact` already does the rewrite.

**M3 — `lang` must expose a tiered init (the key new work).** `lang.birnam_module_init` currently bundles, in one symbol: (a) **primitives** — SmallInt/Float/Bool/Closure arithmetic + control + iteration (`installMethods`' method installs); (b) **method-install** — `vtMethodDoImpl` (`method:do:`); (c) **parser/compiler/eval** — `buildGrammar`, the `AST_*_VT` eval closures, `parseExpr:`/grammar export, the REPL handler. `--strip=malleable` must keep (a) [+ (b) under `--aot=dynamic`] and drop (c) — but (a) lives *inside* the same init, so dropping the module wholesale would kill arithmetic. Split `lang` init into callable tiers:
  - `installPrimitives(api, eid)` — always linked/run.
  - `installMethodDo(api, eid)` — kept under dynamic; dropped under static+malleable (no runtime install).
  - `installFrontend(api, eid)` — grammar + AST-eval + parseExpr + REPL; dropped under any `--strip=malleable`.
  The dylib `birnam_module_init` calls all three (unchanged behavior). The AOT generator calls the tier subset per flags. `peg` (pure parser) is dropped entirely under `--strip=malleable`.

**M4 — static link via Zig namespacing (replaces 14.4's `.a` story).** Don't fight the shared `birnam_module_init` export symbol. The emitted `aot_image.zig` `@import`s each kept module by source path and calls its init (or chosen tier) namespace-qualified — separate Zig namespaces, no symbol collision, no `dlopen`. `--aot=dynamic` keeps today's `dlopen` + manifest path (Phase 12 manifest already embedded — 14.6 mostly done).

**M5 — `--strip=history` ✓ (2026-06-04).** The replay-based image is *already* current-values-only (no tx/op persisted), so the data effect was the default. **Built:** the symbol/RO-index half — `FactStore.read_only` + `freeze()` (assert/retract reject), and `image.loadFrozen` replaying through the lean `frozenInsert` (skips the tx-writer path). The store is frozen read-only after FFI resolve. The remaining mmap-relocation RO *segments* (page-aligned arena mmap + pointer relocation, skip replay) is a **pure load-perf** optimization tracked under Phase 12.5 — not an AOT capability, so not a Phase 14 gap.

**M6 — entry-point semantics ✓ (2026-06-03).** `--entry=<selector>[:<recv>]`: `<recv>` is a bare integer (SmallInt) or an identifier resolved as a top-level binding in the baked image (a Module / user object — the general module-entity entry). `aot_main` resolves a name via a `TOP_SCOPE` binding-chain walk (`api.query_eavt`), sends the selector with zero args after load, and exits with the result's low 8 bits (SmallInt) or 0. Runs-and-exits in every build variant; absent → REPL (static-full / dynamic) or nothing (malleable).

**Net:** all of M1–M6 shipped (2026-06-04). Phases 12/13 de-risked the hard parts (serialization, closure rebuild, prune); the AOT layer added the tiering (M3), driver (M1/M4), data-strip (M2), dynamic dlopen bundle, RO freeze (M5), and general entry (M6). `:history-required` flag (old 14.3) — dropped; not needed. Only the 12.5 mmap-relocation perf path remains, by design.

### Status (2026-06-02)
- **M3 tiering ✓** — `lang` init split into `langSetup` / `langPrimitivesTier` / `langMethodDoTier` / `langFrontendTier`; dylib path calls all (unchanged, 373/373).
- **M1 + M4 ✓ (static, no-strip artifact)** — `src/aot_main.zig` + `zig build aot` produce `birnam-aot`: a standalone exe (`otool -L`: no birnam/dyncall deps) that `@embedFile`s a `.bim`, statically inits peg+lang (namespaced `moduleInitImpl`, no dlopen), loads the snapshot, rematerializes, runs the REPL over baked state. Export-collision solved by gating the `birnam_module_init` C export on `build_opts.export_init` (true=dylib, false=static). Proven: baked `answer` resolves to 42 in the standalone binary.
- **M2 + strip=malleable artifact ✓** — `birnam-aot-malleable`: comptime-stripped pure-VM binary (skips peg + `langFrontendTier`; `image.load` runs before `langPrimitivesTier` so primitive interns hit the restored interner). `prune.stripAst` drops user AST trees + `:method-body-ast`/`:method-ast-range` back-refs (M2). Minimal M6 entry: send selector to a SmallInt, exit with result. Proven: `nm` shows 0 `parseExprImpl`, AST stripped, baked `double 5 => 10` (exit 10).
- **Interner fix (load-before-primitives)** — `restoreEntry` refuses to shadow a kernel-seeded selector (eid < `USER_EID_START`): an image can carry a stray duplicate of e.g. `+` at both `SEL_PLUS=152` (compiler-baked) and a user eid; without the guard, post-load primitive installs went to the user eid while bytecode sent 152.
- **`birnam --aot` driver ✓ (2026-06-03)** — `birnam --aot=static [--strip=malleable] [--strip=history] <image.bim> [--entry=<sel>[:<recv>]] -o <out>`. Copies the image to a per-app embed path (`src/.aot_app.bim`, gitignored — not the fixture), runs the parameterized `zig build aot-build -D…` (embed/strip/entry options; `@embedFile(build_opts.embed_path)`), copies `zig-out/bin/birnam-aot-app` → `<out>`. Proven e2e: a user image → standalone exe (`otool -L` clean), full-REPL (no-strip) and one-shot (`--strip=malleable --entry=double:5` → exit 10, `nm` parser-free) both work. Driver lives in `shell.zig` (branches before kernel boot); needs repo root + `zig` on PATH (build-time tool, like `:freeze`).
- **General M6 entry ✓ (2026-06-03)** — `--entry=<sel>:<recv>` now takes an identifier for `<recv>`, resolved as a top-level binding in the baked image (send to a Module / user object), not just a SmallInt. Driver parses recv as int-or-name; `aot_main` resolves a name via a `TOP_SCOPE` binding-chain walk (`api.query_eavt`, no lang dependency → works under `--strip=malleable`). Entry runs-and-exits in the full-tower path too (was REPL-only); result low-8-bits → exit code. Proven: `run:app` → 42 across static / malleable / SmallInt-recv.
- **`--aot=dynamic` ✓ (2026-06-03)** — portable dlopen-at-startup artifact. Kernel + baked image linked in; modules `dlopen`'d at startup from the image's embedded manifest (`image.peekManifest` parses it before replay), resolved by basename via `src/dylib_search.zig` (a PATH-like resolver: `$BIRNAM_DYLIB_PATH`, then cwd + below, then the exe's own dir). `aot_main` is comptime-branched (dynamic | static-malleable | static-full) so the static-tower helpers that reference peg/lang are *not analyzed* under `-Daot-dynamic` → not linked (`nm`: 0 frontend symbols vs 3 in static). The driver stages each manifest dylib beside the `-o` binary. Proven: bundle runs from a foreign cwd (exe-relative resolve) and via `$BIRNAM_DYLIB_PATH`; entry → 42; REPL (`[6+7]`→13). Note: the manifest travels *in the image* (self-describing); a loaded dylib declaring its own further deps (nested manifest) is the natural transitive-dep extension — peg/lang are leaf modules so unused today.
- **`--strip=history` ✓ (2026-06-04)** — functional RO freeze (the chosen depth; the mmap-relocation RO *segments* remain a pure load-perf 12.5 item, not a capability gap). `FactStore.read_only` + `freeze()`; `assert`/`retract` reject `error.ReadOnly` once frozen. `image.loadFrozen` (comptime-frozen `loadImpl`) replays through a lean `frozenInsert` (tx=1, no history churn / implicit-retract / mutate hooks) instead of `assert` — safe over a current-values-only image. `aot_main` freezes the store **last** (after FFI resolve, which writes during load). Proven: static+history and malleable+history load frozen and run entry → 42; unit tests cover frozen build + write-rejection + `loadFrozen` round-trip.

### Status (2026-06-04) — Phase 14 COMPLETE
All six matrix cells now driver-built per-app: `static`/`dynamic` × `(none)`/`malleable`/`malleable+history`. The two-flag CLI (`--aot=`, `--strip=`) + general `--entry=` are fully wired. **Phase 14 closed.** The one explicitly-deferred slice is the mmap-relocation RO-segment *perf* optimization, which is tracked under **Phase 12.5** (load-time speed, not an AOT capability) — not a Phase 14 gap. Suite 379/379.

### Reasoning
**This is the deliverable that takes birnam from "interesting toy" to "real systems language."** Two-flag combinatorial surface kept small (four sensible outputs) instead of knob-soup. `--aot=dynamic` preserves your stated AOT-with-runtime-dylib-loading requirement. `--strip=malleable` + `--aot=dynamic` is the "ship a plugin host" preset. `--strip=malleable` + `--strip=history` + `--aot=static` is the "ship a frozen end-user app" preset. Both extremes natural; not bolted on.

---

## Phase 15 — heap GC (required)

**Goal:** Reclaim **heap bytes** during a live session. Phase 13's `:prune` is
*logical* GC — it drops facts and clears object-table slots but leaks every
backing allocation. This phase frees the bytes. Distinct mechanism, distinct
command (`:gc`, not `:prune`).

**Promoted from optional → required (2026-06-03):** sustained use of the live
image (the whole point of a Smalltalk-style environment) needs real reclamation.

### Ground truth (audited 2026-06-04)

The only heap reclaimed today is the **materialized struct** — `demote()` frees
it correctly (`promotion.zig:197-198`), and that path is already reachability-
respecting and FFI-pin-aware (`ffi_pause`). **Everything else leaks.** The actual
leakers — none in the object table, none with a defined root:

| Leaker | Allocated | Reachability root |
|---|---|---|
| interner string dupes + name boxes | `interner.zig:80,84` | live iff EID still interned / fact references it |
| string / float boxes | `strings.zig`, `boxed.zig` | fact `v` column + materialized slots |
| constant-pool RRB nodes | `compiler.zig` | live method's `:constant-pool` |
| BytecodeClosure / capture arrays | `vm.zig:235`, `:method-captures-ptr` | live closure entity / enclosing method |
| FFI `Sig` struct + args | `ffi.zig:417` | live `:ffi-sig` wrap entity |
| abandoned HAMT/RRB index nodes | every `FactStore.compact()` (`fact_store.zig:601-605`) | structural-sharing-reachable from current roots |

So 15.1's original "mark materialized-arena reachability from the object table"
targeted the **one already-solved case** and missed all of the above. Rewritten
below. **Hard constraint:** HAMT/RRB use structural sharing — old roots cannot be
naively freed; live nodes are shared with the new tree and across snapshots.

### §Substrate — Phase 12.5 arenas pulled forward (decided 2026-06-04)

GC reclaims by **arena epoch**, not per-allocation GPA tracking. Build the
12.5 arena split first (it also unlocks the AOT mmap RO-segment perf path):

- **S1 — segmented arenas.** Replace scattered GPA `create`/`alloc` with named
  arenas: `struct` (materialized objects — partly exists via demote), `strings`
  (interner dupes + string/name boxes), `boxes` (float/ffi-ptr), `bytecode`
  (streams + constant pools), `captures` (closure capture arrays), `index`
  (HAMT/RRB nodes per epoch), `sig` (FFI Sig holders). `Vm.bump_arena_slot`
  (today a null placeholder) becomes the per-thread transient arena.
- **S2 — epoch tagging.** Each `compact()` / prune opens a new index-arena epoch;
  survivors copy forward, the prior epoch's whole arena frees wholesale once no
  live root references it. Resolves the structural-sharing free problem (don't
  free nodes — free epochs).
- **S3 — snapshot/load reset.** `:load` over a live session frees the prior
  session's arenas before replaying (today: leak).

### §Collector — non-moving mark-sweep (locked 2026-06-04)

**Decision: non-moving now; a compacting pass deferred until fragmentation measurably bites.** Allocations never relocate — a Ref is a stable address for life. Sweep frees unmarked cells in place (per-arena free-list / occupancy bitmap).

Rationale (substrate-specific, not generic GC):
- **Refs live in the immutable fact HAMT.** Non-moving never rewrites it. A moving collector only gets the index rewrite "free" *during* `compact()`, welding it to prune cadence — it could not run on demand mid-session.
- **FFI hands raw box addresses to alien C** (FfiPtr slot 0, strings as `char*`). Non-moving keeps those valid; moving would require a mandatory pin set + movable/pinned arena split.
- **A live image is mostly-live** (long-lived methods/facts/interned names) → non-moving's sweep-the-heap pause is competitive; moving's win is on mostly-garbage heaps.
- **Highest churn = method redefine** (fresh closure + constant-pool + bytecode each time). Only non-moving frees it eagerly (15.5) without waiting for a prune.
- **Target = Smalltalk-VM ballpark.** Fragmentation is the least-urgent problem; the arena-per-type split (S1) already bounds it (fixed-cell box/Sig arenas barely fragment).

Deferred compactor (future, if frag bites) reuses the same arena epochs; it would forward Refs across the four holder kinds (fact HAMT during compact, struct slots, RRB-pool internals, closure captures) with a forwarding-pointer protocol + FFI pin set. Out of scope for Phase 15.

### §Threading-compat (note, 2026-06-04 — does NOT preclude the ADR-11 Datomic shape)

The arena registry is single-threaded today but aligns with the locked Datomic target; the evolution is additive. Requirements threading will impose (recorded so they aren't re-derived):
- **Shared arenas are transactor-owned, single-writer.** `Arena.live` is a plain hashmap (not thread-safe). Under Datomic, mutations of a shared arena serialize through the transactor — same discipline as the tx engine. The `index` arena is already single-writer (only the transactor commits facts). The cross-image shared registry (§Registry-sharing / IdApi) is the transactor-owned registry in embryo, not an obstacle.
- **Transient eval allocations move per-thread.** `Vm.bump_arena_slot` (ADR-11 #2, reserved, currently a null placeholder) is the home for closures/boxes created mid-eval that never escape — freed at frame/thread boundaries, no GC. Only **escaping** values (e.g. a float box bound into a committed fact) move into the shared transactor arena at commit.
- **GC is a transactor safepoint op.** Sweep mutates arena liveness + frees → must run single-writer, and the mark must reach **every thread's `Vm` roots** (frames/stack), not just the object table + current facts (single-thread today: stack empty at `:gc`). Non-moving (chosen) helps: stable addresses mean a reader thread holding a box pointer isn't invalidated by a concurrent sweep of *other* boxes, provided its own roots are marked.
- **Invariant to preserve:** box/closure allocation must stay behind the `arenas.X()` accessor (never a hardcoded global deref at call sites), so the per-thread/transactor split is a registry-internal change. Already followed (`boxAlloc`, `vm.allocBytecodeBlock`, interner `string_allocator`).

### §Single-transactor — rationale + escape-hatch ladder (note, 2026-06-05)

**Don't frame the single writer as a bottleneck to overcome — it is load-bearing for the time-travel feature, and for the image-as-runtime use case it is a non-issue.** Recorded so the reasoning isn't re-litigated:

- **It's a non-issue for the design center.** Writes are human-/program-paced; a single transactor sustains thousands–tens-of-thousands tx/sec. A REPL / live image / dev runtime never approaches that ceiling. Optimizing write concurrency now is premature.
- **It is what makes `as-of` cheap.** One writer = one global total order on `tx` = a clean linear history. That is *exactly* what makes time-travel / `diff` / audit trivial (Phase 15.R). Going multi-writer forfeits the single global timeline → `as-of` then needs vector clocks / per-shard timelines / merge semantics. So the single transactor is the **enabling condition** for the headline temporal story, not a tax paid for simplicity. You'd be trading the crown jewel for write throughput the use case doesn't need.
- **Not a dead end — the persistent/snapshot substrate makes the escape hatches unusually cheap, in order:**
  1. **Group commit / batching.** One thread, many logical tx per physical sync. 10–100× write throughput with zero loss of serialization or global order. Always the first move.
  2. **Shard by `m` (module/namespace).** Disjoint data → N transactors, each single-writer internally with its *own* clean timeline. The shard key already exists (the fact `m` column). Per-shard `as-of` stays clean; only *cross*-shard temporal queries need coordination (rare).
  3. **Per-entity serialization (actor/grain model).** Each aggregate is its own tiny transactor; independent entities commit in parallel. Drops global snapshots unless coordinated — fine for workloads that don't need them.
  4. **Branch/merge optimistic concurrency (the ambitious one).** Persistent values are mergeable: concurrent writers each branch from a snapshot, write independently, a merge reconciles (per-`(e,a)` LWW / conflict detection / CRDT-style). This is **Irmin** (OCaml's Git-like mergeable immutable store) — existence proof, and birnam's substrate is already the right shape. The transactor's job shrinks to *adjudicating conflicts*, not serializing every write.
- **The ONE thing to protect, starting now:** the lock-free-reader + immutable-snapshot discipline. Never introduce ad-hoc locks or shared-mutable state "for convenience" — that is the only thing that forfeits the ladder above. Keep single-writer + immutable and overcoming it later stays a *choice*, not a rewrite. Revisit only when a concrete high-write workload appears; first two moves are group-commit then `m`-sharding.

### Work items

**Scope (resolved 2026-06-04).** The sweep reclaims **immutable, non-cache heap only**: string/float/ffiptr boxes, BytecodeClosures, capture arrays, constant-pool RRB, bytecode streams, FFI Sig structs. Materialized **structs are NOT in the sweep** — they are pure cache, reclaimed by `demote` (Q3 below). The fact-store's own HAMT/RRB index nodes are NOT in the sweep — reclaimed by arena epoch on compact (S2 above). Mark runs over the **current-value reachable set**; `:gc` presupposes a history collapse (Q2 below).

15.1 **Arena substrate (S1–S3 above).** Thread the named arenas through kernel allocation sites: `strings`, `boxes` (float/ffiptr), `bytecode`, `captures`, `sig`, plus the existing `struct` path and per-epoch `index`. Each arena segregates one allocation type → an address self-types by which arena holds it (removes any reliance on vt-at-`-1` for the raw-pointer kinds: Sig/captures/pool/bytecode). This is the bulk of the phase.

15.2 **Mark.** Single-threaded stop-the-world mark over a worklist of Refs into a per-arena liveness bitmap (Q5: fixed-cell arenas → bit-per-cell; variable-size `strings`/`bytecode`/`struct` → alloc header carries size, addr→bit via arena offset).

  **Roots (seed):**
  - **R1 — object table.** Every live (non-cleared) slot's pointer Ref.
  - **R2 — current facts only** (latest non-retracted assert per `(e,a)`; NOT the MEAT history — see Q2 gate). Every value Ref that is heap-owning: tag-`100` strings; tag-`000` boxes; and the four raw-pointer attrs `:ffi-sig` / `:method-captures-ptr` / `:constant-pool` / `:bytecode-stream`.
  - **R3 — FFI-pinned set.** Every Ref live under `ffi_pause > 0` — conservative pin, never swept (an address may be held by alien C).

  **Follow (pop Ref, dispatch by arena/tag/vt):**
  | Ref kind | typed by | action |
  |---|---|---|
  | string box (tag 100) | tag | leaf — mark |
  | float / ffiptr box | vt @ -1 | leaf — mark |
  | closure / BytecodeClosure | vt @ -1 | mark; push `data` + each capture + `parent_self_ref` |
  | constant-pool RRB | arena | mark spine nodes; push each element Ref (recurse children) |
  | capture array | arena | mark; push each Ref |
  | Sig | arena | leaf (`args` = kind enums, no Refs) — mark |
  | bytecode stream | arena | leaf bytes — mark |
  | materialized struct (tag 000, user/kernel vt) | vt @ -1 | **mark source only** — push each slot Ref; struct itself reclaimed by demote, not freed here |
  | eid / smallint / keyword / nil | tag | skip — not heap |

  **Invariant relied on:** the raw-pointer kinds (Sig/captures/pool/bytecode) appear *only* as attribute values on entities (reached via R2), never nested inside a struct slot or RRB leaf — so transitive following meets only self-typed Refs. The arena-as-type split (15.1) makes this robust even if the invariant is later violated.

15.3 **Sweep.** Per box-arena, free every cell whose liveness bit is clear. Skip the `struct` arena (demote owns it) and FFI-pinned cells (R3). Non-moving — survivors stay put, Refs never rewritten (see §Collector).

15.4 **Integrate with logical prune + prune-demotes fix. ✓ (2026-06-04).** `:prune` (Phase 13) cleared an entity's object-table slot **without demoting → leaked its struct alloc**, and `demote` can't fix it afterward (`table.get(eid)` already cleared). **Fixed:** `pruneReachable` + `stripAst` now `promotion.demote(e)` each swept entity *before* clearing its slot (no-op if promotion uninitialized / entity not materialized). Proven by a leak-checked test (struct on `std.testing.allocator`, store/table on arena). No orphaned structs remain → 15.3 needn't scan the struct arena.
  - **FFI Sig free hook ✓ (2026-06-04, Phase 15.E).** The Phase 13 §loose-end imagined freeing a *swept* FFI-wrap's `:ffi-sig`, but FFI wraps are seeded as **roots** in `markReachable` — a wrap is never swept by prune. A Sig leaks only on wrap **retract / re-resolve / module-unload**, so it's a targeted eager free, NOT an arena-sweep target (the `Sig` holds an internal `args` slice that isn't Ref-reachable — a sweep couldn't keep it). **Implemented:** `ffi.freeSig(store, alloc, wrap_eid)` frees the holder + its slice and **retracts** `:ffi-sig` (the shim has no stale-guard, so the dangling fact must go — `resolveWrap` then misses → shim returns NIL). Called from `buildWrapClosure` (frees the prior Sig before a re-resolve supersedes it) and `methodUnload` (per owned entity; runs only at `ffi_depth == 0`, which unload already enforces). Leak-checked test.

15.5 **Reclaim superseded methods via the arena sweep (was: "eager free at tx"). ✓-safe-slice (2026-06-04).** Redefining a method supersedes its old BytecodeClosure / constant-pool / bytecode-stream. **Implemented:** `compiler.materialize` now routes the method-closure wrapper to the swept `closures_vt` arena, so a superseded, slot-cleared, *uncaptured* method wrapper is reclaimed by `:gc` (the capture-aware mark from 15.A follows wrapper `data`).

  **Design correction — eager-free-at-the-redefine-tx is unsafe, dropped (2026-06-04).** The original "the dead allocs are known exactly" premise is false in two ways:
  - **Captured blocks.** A method's inner block can be captured into a still-live closure (`x := [...]` holding a block from the old method). The captured closure resolves its block's method-eid *through the object-table slot*, so neither freeing the old method's allocations nor clearing its slot is safe at the tx — only a full reachability mark knows whether the block survives. That mark is exactly what mark-sweep does, so "free eagerly without a sweep" cannot be made correct (would UAF captured closures — violates the non-moving §never-free-live tenet).
  - **REPL AST rooting.** At the REPL, redefining does **not even orphan** the old method: its *source AST* stays rooted in `TOP_SCOPE` (the PR4 Risk #2 "session creep"), and `markReachable` reaches the compiled method through the AST. So `:prune` reports 0 swept after a redefine; the superseded method is genuinely still reachable until an **AST-drop / REPL-history policy** ships. Reclaiming redefine churn therefore needs that policy first — a separate work item — not heap freeing.

  Net: the safe + correct realization of 15.5 is *route to the swept arenas + frequent sweep (15.6 auto-trigger)*, not tx-time freeing. Bytecode-stream + constant-pool routing/sweep tracked under 15.D (strings/bytecode/pool arenas).

15.6 **Trigger + `:gc` command. ✓ (2026-06-04).** Manual `:gc` debug command (distinct from `:prune`; `:prune` = logical/fact reclaim, `:gc` = heap reclaim). `:gc` runs the collector and **requires/implies a current-only store** (Q2). **Auto-trigger shipped:** `gc.maybeCollect` runs at the REPL line boundary (quiescent — empty value stack) when the swept arenas grow past `GC_GROWTH_THRESHOLD` (50k cells) since the last collection; growth-gated (a stable large live set never re-fires), `ffi_pause`-respecting (collectBoxes re-checks). `resetAutoTrigger()` at `arenas.init`.

### Phase 15 — shipped vs remaining (2026-06-04)

**Shipped (merged to `main`, commit 77ef888 + ae78fc6):** non-moving mark-sweep over four arenas — `boxes` (float/ffiptr), `captures` (BytecodeClosure), `closures_vt` (CLOSURE_VT wrappers, tree-walker + bytecode + materialized-method), `strings` (interner `:name` boxes + method bytecode-stream). `:gc` + growth-gated auto-trigger. 15.4 prune-demotes-before-clear. 15.E FFI Sig free on re-resolve/unload. The mark is capture-aware (R4 follows wrapper `data`), so it is the ONLY correct reclaimer of redefine churn (see 15.5 correction). 398/398, 0 leaked.

**Remaining (each its own focused work item — NOT safe to rush; the fact store is the crown jewel):**

- **15.I — `index` epoch arena ✓ (2026-06-05).** SHIPPED via the epoch-arena route (b) below, future-proof gate included. `FactStore.compact()` (every `:prune`) snapshots current facts, resets all five HAMT roots (`eavt/aevt/avet/vaet/meat`) to `empty()`, and re-asserts — **abandoning the entire old index tree**. The HAMT is persistent and has NO `deinit` (structural sharing makes per-node free unsafe in general). At compact, though, the old roots are *wholly* dead: the new index is rebuilt fresh and shares no node with the old, and nothing external holds index nodes (object-table slots hold Refs, not nodes; `meatIter` snapshots out before the reset; reify side-tables are cleared). So the old tree is safely reclaimable — but ONLY two ways: (a) a hand-written recursive free that descends nested HAMTs (`eavt` values are themselves HAMTs; `avet` is triple-nested) — high double-free risk on the core; or (b) **epoch arena (preferred, future-proof):** route index-node allocation to an `index` arena (a plain `std.heap.ArenaAllocator`, NOT the recording `arena.zig` — the index wants wholesale free, not per-cell sweep) carrying an epoch. `compact` builds the new index into a fresh epoch, then frees the OLD epoch — but **the free is GATED on the oldest retained basis-T (15.R), not unconditional.** Today (current-only, no snapshot API) the gate trivially = "now" so it behaves as "free on compact"; but coding it as a gate, not a hardcode, is what keeps cheap time-travel from being silently foreclosed (a held snapshot pins its epoch → free skips at-or-after the oldest retained root). The HAMT/RRB code is UNTOUCHED — the allocator is already centralized in the map struct and threaded through `updateValueLeaf`/`updateMeat`/`updatePresenceLeaf` (incl. nested maps + the `ValueHistVec` RRB); only the ~25 `self.allocator` index sites swap to `self.indexAlloc()` (query-result `mkRrb` + transient collectors stay on `self.allocator`). Threads the index arena through `init`/`compact`/`frozenInsert`/`deinit`. Touches the hottest path → its own session with dedicated tests (compact-then-reassert round-trip, leak-checked old-epoch free, many-assert-then-compact reclaims spine garbage, no-corruption of the live index, no live iterator/node-ptr held across a compact).

  **Reclaims BOTH index leak kinds at once:** (1) per-mutation spine garbage (every `assert`/`retract` path-copies, abandoning the old spine — accumulates in the live epoch) and (2) compact wholesale (the whole old current tree). Both die when the epoch is freed. Between compacts the index still grows (same as today), but now it all dumps at the next compact → reclaim cadence = prune cadence (frequent prune ≈ continuous; rare prune = grow-then-dump). Continuous per-node reclaim is rejected: it needs refcounting/marking, and HAMT nodes carry no Ref tag so the value-heap collector can't reach them — epoch-at-compact is the 80/20.

- **15.P — constant-pool RRB finalizer.** `compiler.materialize`'s `boxed.mkRrb` is multi-alloc (libid object + `RrbRef` struct + internal tree nodes, none individually Ref-reachable), so it can't be naive-swept like a box. Reclaim needs a **type-aware sweep finalizer**: when an unmarked RRB libid object is swept, also free its `RrbRef` tree (`RrbRef.deinit`). Low yield — constant-pool RRBs only churn on method redefine, which **doesn't orphan at the REPL** (AST rooting, see 15.5), so this is gated behind 15.A-AST below. Group with 15.I (both are persistent-structure node reclamation).

- **15.A-AST — AST-drop / REPL-history policy (unblocks redefine-churn reclaim).** Redefining a method at the REPL does NOT orphan the old method: its source AST stays rooted in `TOP_SCOPE` (PR4 Risk #2 session creep), so `:prune` reports 0 swept and none of the routed method allocations (wrapper/bytecode/pool) ever become reclaimable. Reclaiming redefine churn requires a policy for dropping superseded top-level AST / REPL history — a **product decision** (when is REPL input history collectible? per-line? on `:prune`? keep-last-N?), not a mechanical fix. Once it ships, the existing sweep reclaims the churn with no collector changes.

### Resolved design questions (2026-06-04)
- **Collector — non-moving** (locked, see §Collector). Compacting pass deferred until fragmentation measurably bites.
- **Q2 — as-of history: current-values-only, gated.** Mark roots = current facts (R2), not retained MEAT. `:gc` presupposes a history collapse — safe because heap reclaim is the "make the image small" op, run exactly when history is dropped (matches the shipped keep-current-only prune policy). When **keep-last-N retention** ships, marking over MEAT-within-window is a purely **additive** widening of R2 — no redesign.
- **Q3 — structs left to demote.** Sweep reclaims immutable boxes only; structs are cache, reclaimed via `demote` (manual now, pressure-driven in Phase 8). The one gap — prune orphaning struct allocs — closed by the 15.4 prune-demotes-before-clear fix.

### Reasoning
Persistent structures + logical prune remove most GC *pressure*; demotion is the
natural reclaim path for materialized structs (facts are canonical, the struct
is cache). But the parse/eval/redefine churn above the substrate is unbounded and
must be collected for the image to be a place you live in, not restart. The
arena-epoch model (vs. per-allocation tracking) sidesteps structural-sharing free
hazards and reuses the same page-aligned segments AOT needs for mmap relocation
(Phase 12.5) — one mechanism, two payoffs. Non-moving keeps the immutable fact
HAMT and alien-held FFI addresses untouched; the box arenas self-type by segment,
so the mark walk needs no per-allocation type tag.

---

## Phase 15.R — Retention / Snapshots / Excision (R1–R4 + F1–F4 SHIPPED 2026-06-06)

### §Status (2026-06-06) — COMPLETE. 418/418, 0 leaked

Phase 15.R is finished. Two branches: `phase-15r-retention` (R1–R4, merged
`7e83a90`) then `phase-15r-finish` (F1–F4, the deferred follow-ons).

**Finish pass (F1–F4):**
- **F1** `exciseTxRange(from,to)` — basis-range hard-delete via event collect →
  filter-out range → replay (current value reverts correctly); live + archives;
  refuses while pinned.
- **F2** auto-eviction cadence — `maybeSpillEpochs(dir, keep_hot_n)` keeps the N
  newest retained epochs hot, spills the rest (unpinned only); wired into the
  shell line boundary (keep_hot_n=2).
- **F3** `basis_t` clamped to a monotonic floor (>= next_tx) across image load
  (re-derived, no format bump — snapshots don't cross saves).
- **F4** db-value language surface: `SNAPSHOT_VT`/`DB_PROTO` objects; `[db now]`,
  `[db asOf: t]`, `[snap basisT]`, `[snap get: e attr: a]`, `[snap release]`,
  `[db retainAll|retainCurrent|keepLast:|keepSince:]`, `[db excise: e]`. IdApi
  appended (snapshot/retention/excise C fns); db prototype installed in the
  always-kept primitives tier.

**Genuinely remaining (each its own pass, non-blocking):**
- **Snapshot GC auto-release** — needs a value-heap sweep FINALIZER (Arena.sweep
  only frees cells; a `*Snapshot` is a separate alloc needing `unpin`). Until
  then, `[snap release]` is explicit (correct + leak-free).
- **`[snap find: ?x where: #(...)]`** — datalog-over-snapshot from the language
  (point `[snap get:attr:]` ships now; the kernel already supports full datalog
  over a snapshot view — only the lang→query-module wiring remains).
- **mmap RO segment fast path** vs replay-based fault-in — this is Phase 12.5,
  not 15.R.

### R1–R4 kernel mechanism (commits R1a→R4)
- **R1a** separate monotonic `basis_t` (never resets on compact; `next_tx`
  stays the per-epoch coordinate).
- **R1b** refcounted index epoch registry; `compact` frees an old epoch only
  when `refcount==0 && max_basis < oldestRetainedBasisT()` (no-snapshot path =
  identical to 15.I). `pinEpoch`/`unpinEpoch`.
- **R1c** first-class pinning `Snapshot` = read-only FactStore view capturing
  the 5 roots by value; `snapshotNow()` pins its epoch so compact retains it.
  Because a Snapshot IS a FactStore, all datalog/iterators work against it
  (Decision B).
- **R2** `basis` added to history entries → exact `currentValueAsOfBasis`.
  `RetentionPolicy {current_only|keep_all|keep_since_basis|keep_last_n}` +
  `setRetention`; `compact` rebuild via `compactInsert` (single collapse basis);
  retained epochs store their `IndexRoots` → queryable history archives;
  `snapshotAsOf(basis)` routes to the covering epoch.
- **R3** `exciseEntity` (GDPR) — surgical removal from live index AND every
  retained archive; refuses (`error.SnapshotPins`) while a live snapshot pins
  an epoch (Decision C).
- **R1d** shell surface: `:retain`, `:excise`, `:asof` (kernel-linked, zero
  module-ABI risk).
- **R4** on-disk cold tier: `applyAssert`/`applyRetract` extracted so replay is
  faithful; `spillEpoch`/`faultInEpoch` serialize an epoch's event log to a
  segment file, free RAM, fault back on demand; `snapshotAsOf`/`exciseEntity`
  auto-fault cold archives.

---

**Why a distinct phase.** The persistent (HAMT/RRB) substrate was chosen so that history, snapshots, and time-travel are free (an index root = a zero-copy snapshot). Phase 13/15 framed reclamation as "GC for a database" — correct for the value-heap, but a category error for the fact layer. The fact layer's question is not "is this byte reachable?" but "which history do we keep, for how long, and where does it live?" That is **retention policy + snapshot pinning + excision**, the Datomic bargain. This phase makes it real and keeps it **tiered: invisible by default, surfaced only when a user reaches for time-travel** (a user who only wants "now" never becomes a DBA). See VISION §Memory ¶Retention + §Image/Snapshot.

**Depends on 15.I** (the epoch arena, whose free is already coded as gated-on-oldest-retained-basis-T rather than hardcoded — that gate is this phase's hook).

**Decision (2026-06-05):** build it. Approved scope includes the on-disk cold-history tier (history spilled to disk *during runtime*, not just at image-save).

Milestones (each its own design+commit pass; order is dependency order):

- **R1 — db-value as a first-class pinning snapshot.** `as-of: tx` / `db now` returns an immutable handle = the index roots at that tx. While a handle is reachable (an ordinary object, held in a binding/closure/fact), its index epoch(s) are pinned: the 15.I epoch free skips any epoch at-or-after the oldest live snapshot's basis-T. Dropping the last reference makes those epochs collectible at the next compact. **This is the reachability question for the fact layer — over retained roots, not nodes.** Needs: a snapshot registry (the set of live retained roots → their min basis-T), and the epoch arena consulting it.

- **R2 — retention horizon policy.** Configurable floor below which history is collectible: keep-last-N-txs / keep-since-T / keep-reachable-from-roots. Default = current-only (horizon = now), so R1/R2 are inert until opted into. The horizon, the live snapshot set, and the AOT `--strip=history` all feed one `oldestRetainedBasisT()` the epoch free already consults.

- **R3 — explicit excision.** Hard-delete of named entities or `tx` ranges within the retained window (GDPR-shaped). The ONE op that can invalidate a held snapshot — so it is always user-invoked, never automatic, and either refuses or explicitly tombstones when a live snapshot covers the excised range. Distinct from `prune` (which only collapses *below* the horizon).

- **R4 — on-disk cold-history tier (runtime).** History inside the retention policy but older than the live working set spills to disk *at runtime*: warm/current epochs stay in RAM; cold tx-epochs are written to epoch-granular segment files and evicted from RAM, faulted back only when an `as-of` query reaches them. Bounds a long-lived image's resident set by working set, not total history. Substrate = the single-file page-aligned image layout (Phase 12); the epoch arena's epoch granularity is exactly the spill/evict unit. Interacts with the deferred Phase 12.5 mmap-segment fast path (a cold epoch can be `mmap`'d RO rather than replayed).

### Work items (fleshed 2026-06-06)

**Substrate baseline (15.I, shipped).** `FactStore.index_arena: ?*ArenaAllocator` (single epoch), `retain_history: bool`, and `compact()` which (a) snapshots current facts via `meatIter()`, (b) swaps to a fresh epoch, (c) **resets `next_tx` to 1**, (d) rebuilds current-only, (e) frees the old epoch iff `index_arena != null and !retain_history`. 15.R generalizes (a)/(c)/(e) and replaces the bool gate with `oldestRetainedBasisT()`.

**Data-model changes that thread through all of R1–R4:**
- **Monotonic basis-T.** A **separate** global commit counter (Decision A, locked 2026-06-06) that **does NOT reset on compact** — distinct from `next_tx`, which stays free to reset to 1 on compact as today. `basis_t` is the durable user-facing time coordinate for `asOf:`, snapshot identity, and epoch `min_basis_t`; `next_tx` remains per-epoch internal. Smaller blast radius on existing compact logic; image format gains one `basis_t` field.
- **Epoch list, refcounted.** Replace the single `index_arena` + transient `old_epoch` with `epochs: []Epoch` where `Epoch = { arena, min_basis_t, refcount }`. Head = current (writable). compact pushes a new head; old epochs live until their refcount hits 0 AND they're below `oldestRetainedBasisT()`.
- **`oldestRetainedBasisT()`** = `min(` live-snapshot basis-Ts, retention-horizon floor, AOT `--strip=history` floor `)`; defaults to "now" (current-only) so all machinery is inert until opted in.

#### R1 — db-value as a first-class pinning snapshot
- **R1.1 Snapshot value.** Reserve `SNAPSHOT_VT` kernel eid. A snapshot entity wraps `{ eavt, aevt, avet, vaet, meat (5 roots by value), basis_t, epoch_idx }`. Constructed by `[db now]` and `[db asOf: tx]`. Immutable; an ordinary GC object (value-heap, Phase 15).
- **R1.2 Snapshot registry.** `SnapshotRegistry` in fact_store: live snapshots → their `(basis_t, epoch_idx)`. `register()` on snapshot construct (incr epoch refcount); `unregister()` when the SNAPSHOT_VT wrapper is GC-swept (hook in `gc.zig` sweep of SNAPSHOT_VT). `oldestRetainedBasisT()` reads the min.
- **R1.3 Epoch pinning + gate swap.** Generalize `index_arena` → epoch list (above). `compact()` free condition changes from `!retain_history` to: free epoch E iff `E.refcount == 0 && E.min_basis_t < oldestRetainedBasisT()`. (Today, no snapshots held → reduces to "free on compact," identical to 15.I behavior — must stay green.)
- **R1.4 Read-through-snapshot — full datalog (Decision B, locked 2026-06-06).** The query engine takes an optional root-set; `[snap find: ?x where: #( ... )]` runs ANY datalog clause against the snapshot's own 5 roots, not the live store. Touches `query.zig` dispatch (thread a root-set param through clause evaluation instead of reading `store.eavt` etc. globally). Time-travel is fully usable on landing — the headline feature complete in R1, not a follow-on.
- **API:** `[db now] → snapshot`, `[db asOf: tx] → snapshot`, `[snap get:attr:]` (point read), `[snap find:where:]` (full datalog), `[snap basisT]`. Releasing the last ref → epoch collectible at next compact.

#### R2 — retention horizon policy
- **R2.1 Policy value.** `RetentionPolicy = current-only (default) | keep-last-N-txs | keep-since-T | keep-reachable-from-roots`. Stored on the store; settable via `[db retain: <policy>]`.
- **R2.2 Horizon feeds the one gate.** `oldestRetainedBasisT()` folds horizon floor with the live-snapshot min + AOT strip floor. No new free path.
- **R2.3 compact preserves history within window.** The meaty change: when horizon > now, `compact()` must replay **history facts within the window** (not just `meatIter()` current values) into the new epoch, and must **stop resetting basis-T**. Below the horizon, history still collapses (this is `prune`). Default current-only → unchanged from 15.I.

#### R3 — explicit excision (GDPR-shaped)
- **R3.1** `[db excise: entity]` / `[db exciseTxRange: a to: b]` — hard-delete of facts WITHIN the retained window (distinct from `prune`, which only collapses below the horizon).
- **R3.2 Snapshot safety — refuse (Decision C, locked 2026-06-06).** Excise **errors** if any live snapshot covers the excised range; the user must drop those snapshots first. A snapshot value is never silently corrupted — no invalid-snapshot state to handle anywhere. Conservative default; can relax to tombstone-and-invalidate later if GDPR-delete-always-wins is needed. Always user-invoked, never automatic.
- **R3.3** Excise rewrites affected epochs (can't structurally-share a hole) → forces the touched epoch(s) to rebuild.

#### R4 — on-disk cold-history tier (runtime)
- **R4.1 Epoch → segment spill.** An epoch below the live working set but within policy spills to a page-aligned segment file (Phase 12 image layout is the substrate; epoch granularity = spill unit) and is evicted from RAM.
- **R4.2 Fault-in.** An `asOf:` query reaching a cold basis-T faults its segment back — replay-based first; the `mmap` RO fast path ties to the deferred Phase 12.5 work.
- **R4.3** Resident set bounded by working set, not total history. Verify with a long-lived-image growth test.

**Existing-code touchpoints (for the implementer):** `fact_store.zig` — `index_arena`→epoch list, `indexAlloc()`, `compact()` free gate + no-reset basis-T + history-window replay; `next_tx` semantics. `gc.zig` — SNAPSHOT_VT sweep → registry unregister. `kernel_eids.zig` — `SNAPSHOT_VT`. `query.zig` — optional root-set injection (Decision B). Shell — `:asof`, `:retain`, `:excise` commands mirroring the db-value API.

### Decisions locked (2026-06-06)
- **A — basis-T identity:** SEPARATE global `basis_t` counter (never resets); `next_tx` stays free to reset on compact. Smaller blast radius.
- **B — R1 read scope:** FULL datalog-over-snapshot in R1 (query engine takes an optional root-set). Headline feature complete on landing.
- **C — excise vs live snapshot:** REFUSE when a held snapshot covers the range (user drops snapshots first). No invalid-snapshot state.

**Non-goals / explicitly deferred:** distributed/multi-peer history (Datomic's transactor/peer split — ADR-11 threading note covers the single-transactor shape only); cryptographic history integrity; per-attribute `:no-history` (Phase 11.x schema item, orthogonal).

## Phase 16 — Collections (spec TBD)

**Goal:** Language-level collections — the thing you reach for first when
exploring. The persistent-collection KERNEL (HAMT/RRB, Phase 1) already exists;
this exposes a user-facing protocol on top of it. NOT previously a roadmap phase
(the original intent was "build Cons cells in birnam" — T1 stretch); promoted to
a real phase 2026-06-03.

### Stdlib design DNA — Clojure, *adapted* to a message-passing fact store (2026-06-05)

Take Clojure's ideas, but don't import them as a checklist — most either fall out of the substrate or are subsumed by it. Principles that govern Phase 16/17:

- **Immutable-first is the default, not an option.** Canonical aggregates = persistent vector / map / set (kernel HAMT/RRB), value-returning APIs. Mutable arrays are the rare explicit escape hatch (steal Clojure **transients**: locally-mutable build, frozen on publish = "build in a scratch arena, freeze"). Do NOT expose a mutable `at:put:` array as the *primary* collection.
- **`reduce:`/`inject:into:` is the kernel of the protocol** (with an early-out `reduced`-style sentinel). `do: collect: select: detect: count` derive from it — written **in birnam source**, not Zig. Build a single `seq`/iteration protocol unifying vector/map/set, **query results**, strings, ranges — so a datalog answer and a vector answer the same `do:`/`reduce:`.
- **Reference types = thin sugar over entity+attribute+tx, NOT new machinery.** `cell`/`ref`/`atom`-style identities: read = `currentValue`, update = assert (new tx). They get for free what Clojure's atoms structurally can't: `@thing asOf: tx`, `[thing history]`, durable + queryable. **Lead with this** — it's the realization of Clojure's epochal time model that beats Clojure. Coordinated multi-entity change = one transactor tx (`transact:` block); no STM engine (see §Single-transactor — STM is subsumed, do NOT port refs/MVCC/retries).
- **Transducers — later, as the implementation substrate.** Good fit (no intermediate allocs, composable across collections + future channels) but premature before `collect:/select:` exist. Order: eager reduce-based protocol → refactor `map/filter/take` onto a transducer core → expose `comp` once channels exist to justify the decoupling.
- **Laziness is opt-in** (explicit `lazy`/ranges/streams), not pervasive. Transducers give the no-intermediate-alloc win *eagerly*, which suits a message-passing language better than Clojure's pervasive lazy seqs (head-retention / side-effect-timing footguns).
- **Steal the map literal, keep the objects.** A lightweight persistent map literal (`{name: "x", n: 3}` → a keyword-keyed HAMT *value*, distinct from a heavyweight entity) gives ad-hoc structured data without minting entities. But you still *send messages* to it — don't clone Clojure's function-centric shape (`(map f coll)`). birnam is `[coll collect: f]`: the collection is the receiver. Steal the semantics (persistent, reduce-based), not the syntax (global fns + `->>`). Two ways to do everything = a confused stdlib.
- **Cheap syntactic steals:** **destructuring** in `let` + block params (`[:a :b]` from a seq, map destructuring) — biggest ergonomics-per-effort win; **metadata** = just more facts on the entity (`withMeta:`/`meta` free, equality already ignores it). Threading macros — skip (message chaining is already left-to-right).

**16.0 — the `|>` pipe operator (the immutable replacement for the dropped cascade).** Cascades (`;`, many messages to one *mutable* receiver) were dropped for poor syntactic fit — correct, because immutable values *thread* (each op returns a NEW value → wants it as the next *receiver*), they don't cascade. The real hole: **keyword messages don't chain** — `coll with: 1` returns a new coll, and chaining means `[[coll with: 1] with: 2]` (nested-bracket hell), exactly the dominant immutable-collection shape. `|>` fixes it: `x |> with: 1 |> with: 2 |> select: even` desugars to `[[[x with: 1] with: 2] select: even]`. **Thread-as-receiver** (Clojure `->`, not `->>`), because birnam collections ARE the receiver. ONE operator (don't pre-build a thread-last twin — that's the "two ways" trap). Pure grammar + desugar, no kernel change.

### §Status — Phase 16 FULLY COMPLETE 2026-06-06 (A–E merged; 430/430)

**Tier E (reference types + applicative + transducers + metadata) SHIPPED
2026-06-06 (branch phase-16-tier-e):** the lead feature.

- **Reference cells** (`CELL_VT`=202, `:cell-value`=203). A cell **IS an
  entity** carrying `[cell :vtable CELL_VT]` + `[cell :cell-value v]`, returned
  as an EID Ref so its messages dispatch to CELL_VT (no new boxed type — pure
  fact-store sugar). `db cell: v` / `ref: v` / `atom: v` all mint one (single-
  transactor subsumes Clojure's atom/ref/agent split). `[c value]` =
  `currentValue`; `[c set: v]` = `assert` (new tx, chainable); `[c asOf: tx]` =
  history-walked read; `[c history]` = vector of every value held, oldest→newest
  — **time-travel rides 15.R per-cell history for free**. `db transact: aBlock`
  = coordinated change (single-transactor → atomic by construction, no STM).
  Cells refuse a NIL value (the store forbids it — Date); seed with a real value.
- **Metadata** (`:meta`=204) — `withMeta:` / `meta` on OBJECT_VT (entities only;
  immediates answer self). Free: just another fact; equality ignores it.
- **Applicative *patterns* (17.6, not the abstraction):** `validate:` (error-
  ACCUMULATING — gathers ALL failures vs short-circuit; pairs with schema-as-
  facts 9.5.2), `zip:` (independent positional pairing), `whenAll:` (apply only
  when every element present). `|>` is the sequential pipe; `zip`/`whenAll:` the
  independent combinator.
- **Transducers** (`Xf` factory, `XF_VT`=205/`XF_PROTO`=206): `Xf map:` /
  `filter:` / `take:` return rf→rf' transducers; `coll transduce: xf` runs one
  eagerly in a **single fold (no intermediate vectors)**; `take:` counts via the
  accumulator's own size + `reduced` early-out (no mutable cell). Composition =
  `aXf then: bXf` on CLOSURE_VT (data flows self-then-other). **`comp` deferred
  per DNA** — `then:` is the honest 2-ary compose until channels justify `comp`.

**Earlier deferred items (branch phase-16-deferred, merged):**

(1) `reduced` early-out
sentinel (inject:into: stops + unwraps; detect: uses it). (2) integer ranges
`[lo to: hi]`/`to:by:` (lazy RANGE_VT seq) + **seq-unification** — collect:/
select:/count/detect:/asVector moved to OBJECT_VT over `inject:into:`, so
vectors/ranges/maps/strings all answer one protocol (each provides inject:into:).
(3) strings dispatch (STRING_VT) — size/at:/`,`/`=`/asSymbol/do:/inject:into:
(byte seq). (4) `let #[a b] := seq` destructuring (desugar → temp + indexed
lets). Remaining nicety: block-param destructuring (param-position pattern).

### Tiers A+B+C+D SHIPPED 2026-06-06 (A-D merged)

**Tier D (ergonomics + reflection):** D1 bool/nil — `isNil`/`ifNil:`/`ifNotNil:`
(default on OBJECT_VT + immediates, override on NIL_VT), bool `not`/`and:`/`or:`
(short-circuit blocks); also fixed a latent debug panic where the `nil` literal
asserted a NIL value. D2 `detect:` (self-hosted via isNil) + `min:`/`max:`/
`between:and:` (over `<`). D3 number protocol — `mod:` / Float `sqrt`/`floor`/
`ceil`/`round` (Zig prims) + `abs`/`even`/`odd` (self-hosted). D4
`doesNotUnderstand:` — sendN routes a dispatch miss to SEL_DNU (kernel eid 198)
if a handler exists (else NIL); default ERROR_VT diagnostic rendered by message.
**Deferred:** destructuring `[:a :b|]` (syntax-heavy), seq-unification for
strings/ranges (needs those types), `reduced` early-out, `&`/`|` (`|` collides
with the block-param bar).

### Tiers A+B+C SHIPPED 2026-06-06 (branch phase-16-tier-a, merged)

**Tier C (maps):** `{k: v}` keyword-keyed HAMT map literal + ASSOC_VT dict
protocol (`at:put:`/`at:`/`at:ifAbsent:`/`includesKey:`/`size`/`keys`/`values`/
`merge:`/`do:`). Literal desugars to an `at:put:` chain (works tree-walker +
bytecode). `map_has` disambiguates stored-nil vs absent (no-NULL). REPL renders
`{k: v}` with keyword keys by name (new `symbol_name` IdApi); symbols print
`#name`. **Seq-unification deferred to Tier D** — strings/ranges answering
`do:`/`reduce:` need string-char iteration + a range type (both Tier D); query
results already materialize as RRB vectors.

### Tiers A+B SHIPPED 2026-06-06 (branch phase-16-tier-a)

**Tier B (self-hosting + pipe):** `collect:`/`select:`/`count`/`,` (concat) on
RRB_VT derived in BIRNAM SOURCE over Tier A's `inject:into:`/`with:`/`size`
(installPrelude compiles birnam-source block bodies to bytecode at lang init;
skipped under --strip=malleable). `|>` pipe operator (thread-as-receiver,
left-assoc, keyword+unary; pure grammar+desugar). Verified:
`#[1 2 3] |> collect:[x*x] |> select:[x>3] |> size` => 2. `detect:` + `reduced`
early-out deferred to Tier D (cleanest with nil ergonomics).

### Tier A SHIPPED 2026-06-06 (branch phase-16-tier-a)

Vectors are live: `#[a b c]` literal, `size`, `at:` (raises OOB via ERROR_VT),
`at:ifAbsent:` (primary, no-NULL), `with:` (immutable append), `do:`,
`inject:into:`. Vectors = RRB Refs (methods on RRB_VT). The literal desugars to a
left-nested `with:` chain over an empty-RRB literal → works in tree-walker AND
bytecode with NO new AST kind / opcode / compiler change (verified inside
compiled methods). REPL renders `#[..]` recursively. boxed
`wrapRrb`/`rrbWith`/`rrbLen`/`rrbNth`; IdApi appended
`make_rrb`/`rrb_with`/`rrb_len`/`rrb_nth`. NOT merged to main yet.

Deferred from Tier A → Tier B (all derivable in birnam source over
`do:`/`inject:into:`): `reduced` early-out sentinel,
`collect:`/`select:`/`detect:`/`count`/`,`, `printString`-as-method.

### Build order (fleshed 2026-06-06 — supersedes the literal 16.1→16.5 numbering)

The roadmap-number order is NOT the build order. Two Phase-17 items (`printString`, bool/nil + `doesNotUnderstand:`) are prerequisites for *exploring* Phase 16 and are pulled forward; `|>` (numbered 16.5) is the ergonomic linchpin and moves up next to the first value-returning keyword message. **15.R ships first** (decided 2026-06-06) so reference types (Tier E) debut with full `asOf:`/`history` time-travel.

**No-NULL resolution (locked 2026-06-06, Date-exact — see VISION §No-NULL):** nil is a value, absence is not. nil-object is freely storable/returnable; absence rides a separate channel (`ifAbsent:` / empty-result / raise), never a value. This governs every `at:`-shaped API below.

**Bracket-literal family — LOCKED (all four designed now, even though set ships later):**

| form | meaning | built |
|---|---|---|
| `#( ... )` | quoted literal array (query clause lists) | exists |
| `#[a b c]` | persistent **vector** value (RRB-backed) | A (16.1) |
| `{k: v}` | keyword-keyed **map** value (HAMT) | C (16.4) |
| `#{ a b }` | **set** value (HAMT) | grammar reserved in A; proto later |

Reader discipline: `#(`=quote-array, `#[`=vector, `#{`=set, `{`=map. `{` is the only non-`#`-prefixed literal — confirm no collision in the PEG `atom`/`expression` rules before adding (today `{` is unused → safe). Set proto is a thin layer over the same HAMT the map uses, so reserving its literal now costs one grammar rule.

#### Tier A — make a collection exist and visible
- **16.1 Vector** (`VECTOR_VT` kernel-reserved) + `#[a b c]` literal (PEG rule + desugar). Persistent (RRB), value-returning. `at:` **raises** out-of-bounds (domain error, not absence); `at:ifAbsent:` = checked variant. `size`.
- **16.2a `reduce:` primitive** + `reduced` early-out sentinel. The kernel of the whole protocol; a thin Zig RRB-walk primitive (`inject:into:` is the seeded form). Everything else derives from it.
- **17.1↑ `printString` / `displayString` / `print:`** (pulled forward — can't debug a collection you can't print). Small primitive + birnam-source recursion over `do:`.

#### Tier B — derive the protocol in birnam SOURCE
- **16.2b** `do: collect: select: detect: count , (concat)` written **in birnam over `reduce:`** — the self-hosting payoff, not Zig. Element-add **naming: `with:`** (immutable augmented copy; explicitly NOT `conj`). `at:put:` returns a new collection (or transient-only).
- **16.5↑ `|>` pipe** (16.0 above) — grammar rule + desugar. `x |> with:1 |> with:2 |> select: even` → `[[[x with:1] with:2] select: even]`. Thread-as-receiver (`->`). **ONE operator** (no thread-last twin). Pure grammar, zero kernel. Moves here because the moment `with:`/`collect:` return new receivers, nested-bracket hell appears.

#### Tier C — second collection + the unification payoff
- **16.4 Map / assoc** (`ASSOC_VT` reserved) + `{k: v}` literal (keyword-keyed HAMT *value*, distinct from a heavyweight entity). Protocol: `at:ifAbsent:` (**primary**), `at:` = `at:ifAbsent:[raise]`, `includesKey:`, `with:` (assoc), `merge:`, `keys`, `values`. **Map values may be nil-object** (real value); stored-nil vs absent stays distinguishable via `ifAbsent:`.
- **16.x seq unification** — extend the one `seq`/iteration protocol to **strings + ranges + query-results** so a datalog answer and a vector answer the same `do:`/`reduce:`. Query results already exist (Phase 7). Highest-leverage glue in the arc; nearly free once 16.2 exists. (Tenet 3 proof.)

#### Tier D — ergonomics + escape hatch
- **17.5↑ bool/nil** (pulled forward) — `and:`/`or:` (short-circuit), `not`, `&`/`|`; `isNil`/`ifNil:`/`ifNotNil:`. Cheap, high-use, needed throughout collection exploration.
- **17.3 `doesNotUnderstand:`** + equality/comparison protocol across types — foundational reflection + graceful unknown-selector handling; underlies sort / `min:` / `max:` / sorted collections.
- **destructuring** in `let` + block params (`[:a :b | ...]` from a seq, map destructure) — grammar + desugar. DNA cheap-steal, biggest ergonomics-per-effort win.
- **17.4 number protocol** — `abs modulo min: max: sqrt floor ceil round even odd between:and:`. Mostly birnam-source over existing SmallInt/Float primitives.
- **16.3 transients** — locally-mutable build, frozen on publish (Clojure transients). The explicit escape hatch; persistent RRB sharing is the default. Build only when bulk-build perf bites.

#### Tier E — gated / later (post-15.R)
- **reference types** `cell`/`ref`/`atom` = thin sugar over entity+attr+tx. **Lead feature.** read = `currentValue`, update = assert (new tx). Full `@thing asOf: tx` / `[thing history]` — works day one because 15.R already shipped. Coordinated multi-entity change = one transactor `transact:` block (no STM — subsumed by single-transactor).
- **17.6 applicative *patterns*** (NOT the abstraction) — `validate:` (accumulate ALL failures, pairs with schema-as-facts 9.5.2); `zip`/`whenAll:` (independent combination → parallelizable over future transactor threads). `|>` = sequential pipe; `zip`/`whenAll:` = independent combinator.
- **transducers** — refactor `map/filter/take` onto a transducer core (eager, no intermediate allocs); expose `comp` only once channels justify the decoupling. DNA "later".
- **metadata** `withMeta:`/`meta` — free (just more facts; equality already ignores it). Anytime.

### Reasoning
Can't meaningfully explore a language without data structures + iteration. Kernel
substrate is ready; this is surface + a small set of primitives (`reduce:`, literals)
with the bulk derived in birnam source over the RRB. Order is driven by *what you
need to see/debug a collection* (printString, bool/nil, `|>`) ahead of breadth,
and by the one-seq-protocol tenet (vector/map/string/range/query-result all answer
`do:`).

---

## Phase 17 — Standard Library

### §Status — Phase 17 SUBSTANTIALLY COMPLETE 2026-06-06 (432/432)

Most of Phase 17 shipped early (pulled forward into Phase 16 Tiers D/E — build
order ≠ roadmap order). Item-by-item:

- **17.1 printing — SHIPPED 2026-06-06** (branch phase-17-stdlib): `printString`
  (renders any value to a string via the canonical `appendRef` formatter),
  `displayString` (strings unquoted), `asString` (= displayString), `print`
  (writes displayString+newline to output, returns self → intermediate values
  visible in code). On OBJECT_VT + immediates (SmallInt/True/False/nil).
- **17.2 strings — SHIPPED (full batch 2026-06-06, branch phase-17-stdlib).**
  Core `size`/`at:`/`,`/`=`/`asSymbol` (Tier-deferred) + `<`/`copyFrom:to:` (17.1
  batch) + the string batch below.
  - **Zig (native — byte-level/alloc-loops/lexer):** string-literal **escapes**
    `\n \t \r \\ \" \0` (PEG `string` rule now `'"' (('\' any)/(!'"' !'\' any))* '"'`
    + `decodeStringLiteral`; `formEnd` form-splitter skips escaped quotes);
    `indexOf:`/`indexOf:ifAbsent:`, `asInteger`/`asNumber`, `asUppercase`/
    `asLowercase`, `trim`/`trimLeft`/`trimRight`, `split:`, `join:` (on vectors),
    `reverse`. id_api appended 7 fns (string_index_of/to_int/case/trim/split/
    join/reverse).
  - **Birnam (prelude — composed over byte primitives + natives):** `isEmpty`
    (generic), `startsWith:`, `endsWith:`, `includes:`, string `<=`/`>`/`>=`,
    `replace:with:` (= split+join), `lines` (split `\n`), `words` (split+select
    non-empty).
  - **Grammar enhancement:** `binary-op` now matches **one or two** operator
    chars (`op-char op-char?`), so 2-char selectors like `<=`/`>=` tokenize as
    one. Also gave SmallInt native `<=`/`>=` (had only `<`/`>`/`=`).
- **17.3 equality + comparison — SHIPPED 2026-06-06:** default identity `=` on
  OBJECT_VT + singletons; structural `=` on RRB_VT (size + recursive elementwise);
  string lexicographic `<`. `sort` / `sortBy:` on vectors (comparison protocol =
  "any type answering `<`"; ≤1024 elems else raise). `doesNotUnderstand:` already
  shipped Tier D (SEL_DNU=198).
- **17.4 number protocol — SHIPPED** (Tier D): mod:/sqrt/floor/ceil/round/abs/
  even/odd + min:/max:/between:and:.
- **17.5 bool/nil — SHIPPED** (Tier D): and:/or:/not, isNil/ifNil:/ifNotNil:.
  **`&`/`|` still deferred** — eager `|` collides with the block-param bar;
  and:/or: cover the need.
- **17.6 applicative patterns — SHIPPED** (Tier E): validate: (accumulate),
  zip:/whenAll:.

**Remaining:** `&`/`|` (grammar collision); a general `compare:`/`<=>` three-way
+ sorted-collection type (sort over `<` covers the common case today); string
`format:`/interpolation if needed (split/join/trim/case/search/escapes all done).
None blocking.

**Goal:** The minimal surface that makes the language pleasant to explore and
self-document. Largely buildable IN birnam once collections land; some primitives
needed. Promoted to a real phase 2026-06-03 (was "improve incrementally, no phase
pinned" — ROADMAP §4.5/§Phase 6 notes).

### Work items (to flesh out)

17.1 Printing/inspection: `displayString` / `printString` / a `print:` primitive (see intermediate values inside code, not just the REPL's final auto-print).

17.2 String operations: `size`, `at:`, `,` (concat), `=`, `asSymbol`/`asString`, substring.

17.3 General equality + comparison protocol across types; `doesNotUnderstand:` (deferred since Phase 4.5 — ROADMAP §343-344) for graceful unknown-selector handling + reflection.

17.4 Number protocol fill-out (modulo, abs, min:/max:, range helpers, `sqrt`/`floor`/`ceil`/`round`, `even`/`odd`, `between:and:`).

17.5 Boolean + nil ergonomics: `and:`/`or:` (short-circuit), `not`, `&`/`|`; `isNil`/`ifNil:`/`ifNotNil:`. (Today True/False have only `ifTrue:`/`ifFalse:`; nil has nothing.)

17.6 **Applicative *patterns* — steal them, do NOT reify the abstraction.** The formal Applicative (`pure`/`<*>`) needs higher-kinded types + a typeclass mechanism birnam doesn't have and shouldn't grow; naming anything "applicative" would be cargo-culting Haskell vocab. But the three things applicatives *buy* are each worth shipping as plain APIs:
- **Error-accumulating validation** (Haskell `Validation` — the #1 real applicative use). A `validate:` / spec surface that gathers ALL failures instead of short-circuiting at the first (monadic = stop at first; applicative = collect all). Pairs directly with schema-as-facts (9.5.2).
- **Independent combination / `zip` / `whenAll:`** — combine results that DON'T depend on each other (vs the sequential dependency `|>` expresses). The independence is the point: it's what makes the stages *parallelizable* (→ over the future transactor-threaded futures/agents). Encode "these stages are independent" as a verb; don't make the user reach for a monad.
- **Parser combinators** — the PEG `seq` combinator already IS the applicative parser. Shipped without the name. Leave it.
  Net: `|>` is the *sequential* pipe (16.0); `zip`/`whenAll:` is the *independent* combinator. That pair is the honest dynamic-language version of "monad vs applicative" without the typeclass tax.

### Reasoning
These are the affordances "explore the language" actually needs. Deliberately
deferred off the infra roadmap; now first-class so they get specced + built
rather than accreting ad hoc. Stdlib design DNA (immutable-first, reduce/seq
spine, reference-types-over-tx, transducers-later, `|>` pipe) lives in the
Phase 16 §Stdlib design DNA block.

---

## Phase 18 — Native Codegen (final)

**Goal:** Lower IR facts to machine code. The capstone optimization phase — after
semantics, collections, and stdlib are settled.

### Work items

18.1 Choose lowering target: hand-rolled emitter, QBE, or LLVM. Default: hand-rolled for x86_64 + arm64. Fall back to QBE if time-constrained.

18.2 Lowering passes: AST/bytecode → SSA-ish IR → machine code. Each pass is a birnam module.

18.3 Replace bytecode VM with native code for hot methods.

### Reasoning
Performance polish, not correctness. Bytecode + inline caching gets us to
Smalltalk-VM ballpark, enough for self-hosting. Final phase — optimize once the
language + library are stable.

---

## Phase 19 — FFI Absorption Layer (shimless library wrapping)

**Goal:** Wrap a real C library (libtecla was the forcing test) directly from the
REPL — no hand-written shim dylib. Promote FFI from "scalar toy" to "can absorb a
flat C API," then toward the relational endgame where a C struct *is* an entity.

Numbered 19 because Phase 18 (native codegen) is the deliberate capstone; this is
a parallel track extending the Phase 9 / 9.5 FFI lineage, not a successor to it.

### §Status — batch 1 SHIPPED 2026-06-06 (441/444, 3 arm64-aggregate skips)

The "promote FFI to flat-C-capable" batch landed. All in `ffi.zig` +
`ffi_module.zig` + `kernel_eids.zig` (eids 207–219) + `bootstrap.zig`:

- **Arity cap removed.** The old 0..3 limit was a lone vestigial guard;
  `shimAny` is variadic and `SEND_N`→`sendN` already carry arbitrary arity. Only
  ceiling now is `SEND_N`'s u8 arg-count (>255 still rejected). 4+ arg C
  functions (`gl_get_line`) now wrap.
- **`Z` string arg.** Birnam strings marshal to NUL-terminated C `char*` via
  scratch buffers freed after the (synchronous) call.
- **`nil` → NULL** for `.pointer`/`.cstring` args (`start: nil`).
- **`Z` cstring return → birnam string** (NULL → nil).
- **Deref / peek-poke** on `FFI_PTR_VT`: `peekInt:`/`peekByte:`/`peekDouble:`/
  `peekPtr:`/`peekString:` (byte offsets), `pokeInt:at:`/`pokeByte:at:`/
  `pokeDouble:at:`/`pokePtr:at:`, plus `address`/`isNull`.
- **`Module alloc:` / `[p free]`** — libc-backed raw cells (stable address).
  Out-params fall out: alloc a cell, pass its FfiPtr as a `p` arg, `peek…` it
  back (verified end-to-end against `strcpy`).
- **Struct-by-value: parser + x86_64 marshaling.** Sig syntax `{...}` of scalar
  sigchars; natural-alignment layout; dyncall `DCaggr` arg (`dcArgAggr`) +
  return (`dcBeginCallAggr`/`dcCallAggr`, result as a libc FfiPtr). **arm64-Apple
  gated OFF** — vendored dyncall 1.4 leaves `argAggr`/`callAggr`/`beginAggr` NULL
  in its arm64-apple vtable; calling through them segfaults, so on aarch64 an
  aggregate call returns nil (see `ffi.aggregates_supported`). See 19.1.

shimless libtecla now works on arm64 *except* anything needing struct-by-value
(libtecla's line-editing core is pointer/cstring/int — fully covered).

### Work items (backlog)

19.1 **Struct-by-value on arm64.** Either vendor dyncall ≥1.5 (has arm64-apple
     aggregate support) or hand-roll AAPCS aggregate classification: ≤16-byte
     integer composites → consecutive GP regs (decompose to i64 chunks); HFA/HVA
     (≤4 homogeneous float/double) → V regs; >16 bytes → stack copy; capture
     ≤16-byte struct *returns* from x0:x1 / v0–v3 (the hard part — scalar call
     paths only expose x0). Don't ship half-correct ABI; wrong classification =
     silent corruption.

19.2 **Callbacks.** Birnam closure → C function pointer via dyncallback
     trampolines. Unlocks `qsort` comparators, event loops, libtecla's own
     completion/word callbacks. Hardest item; needs a closure→thunk registry +
     lifetime story.

19.3 **`char**` / string arrays** — argv-style in (build from a birnam vector of
     strings) and out (NULL-terminated `char**` → birnam vector).

19.4 **Byte-buffer ergonomics** — bulk `peekBytes:len:` / `pokeBytes:at:` between
     an FfiPtr and a birnam string/byte-vector (memcpy), so crypto/image/net
     buffers don't need per-byte poke loops.

19.5 **Struct-as-entity (the birnam-flavored absorb).** Describe a C struct's
     layout once; mint an entity whose attrs mirror the fields, backed by an
     FfiPtr. Field access becomes ordinary message sends (`[pt x]` / `[pt x: 3]`)
     that lower to typed peek/poke at the field offset. This is the idst-style
     move — a struct stops being an opaque byte blob and joins the fact store —
     and is the real reason FFI lives next to a relational substrate. Depends on
     19.1 for by-value cases; pointer-to-struct works on 19's shipped deref today.

19.6 **Remaining scalar types** — unsigned variants (`uint`/`ulong`/`uchar`),
     explicit-width ints, wide chars. Mostly sig-parser + marshal additions.

19.7 **Out-param sugar** — a one-call helper (`[m call:… outPtr:]` or a `Cell`
     object) that allocs + passes + peeks + frees, so the common
     `open(path, &handle)` shape isn't four manual steps.

### Reasoning
The shipped batch is "table stakes" — arity, strings-in/out, NULL, deref,
out-params cover the majority of flat C libraries. 19.1–19.4 cover the long tail
of *calling* C; 19.5 is the part that's actually about birnam — folding foreign
data into the entity/fact model rather than leaving it as opaque pointers. Build
calling-completeness first (it's verifiable against real libs), then 19.5 once
the marshaling primitives are stable.

---

## ADR-perf-1 — Performance model & the projection boundary (2026-06-07)

**Status:** accepted (analysis); B + A shipped; the boundary decision (items 3–5)
is the open architectural work this ADR exists to flag.

### Context
A perf review of the dispatch/state path (triggered by the FFI instance-vt
churn). Cost model read from the code, **then measured** (ReleaseFast, arm64,
GeneralPurposeAllocator + index epoch arena = production config; harnesses
`src/kernel/kernel_bench.zig` + `kernel_write_bench.zig`, commits eb32d3e /
d6f65b8):

| Op | What it does | Native baseline | **Measured** | Ratio |
|---|---|---|---|---|
| Warm dispatch | `bind` = vtableEidOf (object-table HAMT get) + mcache hit + indirect call | indirect call ~1.3 ns | **~8 ns/op** | **~6.4×** |
| Field read | `currentValue(e,a)` = nested-HAMT gets | hashmap get ~2 ns | **~20 ns/op** | **~10.3×** |
| Field write (add) | `assert` = path-copy 5 indexes + history + hooks + mcache flush | store ~5–20 ns | **~1.8 µs/op** | **~100–360×** |
| Field write (`:=`) | retract + assert (lang's mutateBinding) | — | **~3.0 µs/op** | — |

**Course correction from measurement (don't repeat the wrong conclusion):**
Dispatch was first estimated at 2–5× and then mis-measured at ~22× — the latter
was an ARTIFACT of benching `SEL_VTABLE`, whose body re-resolves the vtable via
`currentValue`. Benching a trivial method (`SEL_INIT`, `return self`) shows true
warm dispatch is **~8 ns / 6.4× — NOT a bottleneck** (Smalltalk-ballpark). Of
that 8 ns, ~4.6 ns is the object-table HAMT lookup and mcache is ~free. **So
dispatch inline caches (the original perf-1.6) target a non-problem.**

The real lever is the **field read**: `currentValue` ~20 ns / 10.3×, paid by
*every method that touches entity state* (it's what inflated the SEL_VTABLE
number). Cutting it needs slot access at a fixed offset against a known shape —
which requires the receiver to be a materialized struct with a stable layout,
i.e. it couples to the **tier-2 representation (perf-1.4)**, not to a standalone
dispatch cache. A write costs **~100× a read** (~1800 ns vs ~20 ns).

**Sustained mutation is bounded only with prune-in-loop + the index epoch arena**
(churn+prune holds ~114 MiB at both 100k and 500k iters); raw mutation grows
without bound (finding #1 — what crashed the machine in the first, un-fenced
benchmarking attempt).

### The foundational tension
The fact store is **both** the durable/queryable/time-traveled projection **and**
the runtime object representation (source of truth; promotion is a read-through
cache; writes are write-through). Consequences:

- **Compute / immutable-data** (arithmetic, functional transforms over HAMT/RRB):
  ceiling ≈ good Smalltalk / slow-JIT. Fine — this is Clojure's proven profile.
- **Mutation-heavy base-level OO** (objects updating fields in loops): every
  assignment is a 5-index persistent transaction. **No amount of JIT/IC removes
  "every write updates five indexes + history."** This is the real, absolute,
  unacceptable ceiling for write-heavy code.

The meta-level (method tables, AST, bytecode) is write-once-read-many → promotion
warms up → amortized; the cost lands on base-level *mutable* state.

Datomic analogy (already our north star) actually warns against the conflation:
Datomic is a **database, not a runtime** — apps hold working values in memory and
transact deltas at boundaries; they don't run hot loops through the indexes.

### Decision
Adopt the contract: **fact store = projection of durable / queryable / shared /
time-traveled state. Transient and working state lives off-store.** The fact-store
write cost moves to the *persistence/query boundary* instead of every assignment;
the runtime ceiling rises to Smalltalk/JIT range. The alternative ("all state is
always a write-through fact") permanently caps mutable workloads at DB-write speed
and is rejected.

### Inherent vs fixable
- Dispatch (2–5×) — fixable: call-site inline caches + Phase-18 JIT. Not a ceiling.
- Field read (10–50×) — fixable **iff materialization sticks**: shape/hidden-class
  + inline slot caches (the V8 "objects-are-dicts → hidden classes" move).
- Field write (100–1000×) — **only** fixable by not making write-through mandatory
  (transients / escape analysis / write-back). This is the load-bearing item.

### Work items
- **perf-1.B (SHIPPED 2026-06-07):** `demote` no longer resets hotness — a
  shape-changed hot entity re-promotes on the next dispatch instead of re-earning
  the whole threshold (~`threshold` slow cold dispatches saved per shape change).
- **perf-1.A (SHIPPED 2026-06-07):** the materialized struct is now a *mutable*
  read cache — `onEntityMutate` patches the slot in place for a same-shape value
  write (attr already in the layout) and only demotes on a true shape change
  (attr added / retracted-to-absent / vtable / whole-entity). Stops hot+mutable
  entities (cells, counters) from being evicted on every assignment, which made
  promotion a net loss for them. FFI-pin-guarded; binary-searches the sorted
  layout for the slot index. Down payment on write-back (A makes the struct
  writable; write-back adds dirty-tracking + deferred reconcile).
- **perf-1.3 (DESIGN RESOLVED 2026-06-07; implementation gated on 1.4):** the
  projection-vs-representation boundary. Decided: **time-travel is opt-in.** Full
  tier model + opt-in API in the subsection below. This unblocks 1.4/1.5.
- **perf-1.4 (OPEN): write-back tier.** Hot mutable slots live in the struct;
  writes patch + mark dirty; facts reconciled lazily at tx/query/save boundaries.
  Per-assignment DB cost → per-commit DB cost.
- **perf-1.5 (OPEN): escape analysis / transients.** Locals and non-escaping
  objects never become facts (Clojure transients / V8 stack objects).
- **perf-1.6 (REVISED by measurement 2026-06-07):** dispatch inline caches were
  the original plan — **dropped.** Measurement shows warm dispatch is ~8 ns /
  6.4×, not a bottleneck; mcache already makes the method probe free. What
  remains worth doing is **shape/hidden-class slot access for the field READ**
  (the ~20 ns `currentValue`): when the receiver is a materialized struct of
  known layout, a `self attr` read becomes a slot load at a fixed offset (~1–2
  ns) instead of a nested-HAMT walk. This is a READ optimization, and it only
  pays off once the receiver is reliably a struct pointer with a stable shape —
  i.e. it rides on the **tier-2 representation (perf-1.4)**, not on a standalone
  cache. Net: fold perf-1.6 into 1.4 (shape) + Phase-18 (JIT slot inlining); no
  separate dispatch-IC work item.
- **micro-win available now (optional, ~4.6 ns):** an eid→vt direct-mapped cache
  (mirroring mcache) to skip the object-table HAMT lookup in `vtableEidOf`.
  Low-risk, but small (8 ns → ~4 ns dispatch) and dispatch isn't the bottleneck,
  so deprioritized.

### perf-1.3 deliverable — tier model + opt-in API (RESOLVED 2026-06-07)

**Decision: time-travel is opt-in.** The fact store is the truth for *structure*
and *temporal* data, and a *projection* for working object values. This is the
one inversion the whole performance story rests on; it is accepted.

#### Three tiers + two triggers

- **Tier 1 — heap / transient (default for new values + locals).** No eid, no
  facts. Collections (HAMT/RRB), boxed scalars, method args/temps (already frame
  slots), non-escaping objects. Pure speed. (Note: object *instantiation* today
  mints a fact-entity eagerly — 1.5 makes that lazy/escape-driven.)
- **Tier 2 — materialized entity (default for escaped user objects).** Has an
  eid; **runtime truth is the materialized struct (write-back); facts are a
  projection reconciled at boundaries.** No per-mutation history. Most user
  objects. This is the new tier 1.4 builds.
- **Tier 3 — temporal entity (opt-in).** Write-through to the historied 5-index
  store (today's `assert` behavior). Gets `asOf:`/`history`. Reference cells
  (Tier E) are the shipped instance of this.

Triggers: **1→2 = escape** (stored into another entity's attr, returned past its
frame, or queried — perf-1.5). **2↔3 = declaration** (default 2; opt into 3).

Current fact categories → tiers: **meta-level** (vtables, AST, bytecode, methods,
grammar) stays **write-through** — it is write-once-at-definition so the cost is
moot, AND `bind` dispatches by reading those facts, so it cannot be naively
write-back. "Default = tier-2" is a statement about **base-level user objects**,
not kernel structure. **User objects = tier-2. Reference cells / audited entities
= tier-3. Locals / non-escaping = tier-1.**

#### Opt-in API — one bit, three granularities

Tier-3 is a single per-entity flag, `:temporal?`. The write router reads it:
set → write-through + historied; unset (default) → write-back. Everything below is
just *how you set the bit*.

1. **Per-instance:** `[obj beTemporal]` → asserts `[obj :temporal? true]`.
2. **Per-kind (prototype):** `[Account beTemporal]` marks the prototype;
   instances **stamp the bit at `new`** (copy proto's `:temporal?` into the
   instance — not a delegation walk per write, so the hot-path check stays a
   single local bit read).
3. **Per-field:** use a reference cell (`db cell: v`) — a built-in always-temporal
   micro-entity (its prototype `CELL_VT` has `:temporal?` set). The object slot
   holds the cell's eid (stable; doesn't churn on value change), so temporal
   write cost lands only on the small cell, leaving the rest of the object tier-2.

**Unification:** there is one mechanism. A cell is just an entity whose prototype
carries `:temporal?` by default; `beTemporal` is the general form. Want a few
audited fields → cells; a whole object/kind → `beTemporal`; speed → do nothing.

#### Implementation notes

- **Bit placement:** stamp `:temporal?` on the materialized-struct header (a
  reserved slot/bit) at promote/`new`, so the write router never does a fact
  lookup to choose write-through vs write-back.
- **Inheritance:** `new` reads the prototype's `:temporal?` and stamps the
  instance.
- **Reconcile (tier-2 only):** dirty-set flushed to facts at **query /
  tx-commit / save**. Tier-3 needs no reconcile (always current in facts).
  Queries become a flush barrier over the dirty set (bounded).

#### Semantics locked

- **History begins at opt-in.** `beTemporal` after writes → no retroactive
  timeline; audit starts now.
- **`asOf:` / `history` on a tier-2 entity is empty/error** — no opt-in, no
  timeline. This is the explicit capability given up in exchange for native-speed
  mutation; accepted.
- **Cost is local** — only opted-in entities pay write-through.

#### Tenet alignment

Datomic shape (peers hold working values, transact deltas at boundaries) = tier-2
write-back turned inward — consistent with the locked threading target (ADR-11).
Storage-vs-materialization ([[feedback_verify_tenets]]): the tier-2 struct is
*materialization*; facts stay 1NF at the *storage* layer. idst/COLA malleability:
untouched — shape lives in facts.

### Consequences
Promotion's current sweet spot is immutable-after-setup entities (vtables,
classes) — explicitly the design target; B/A widen it to stable-shape mutable
entities. The deeper ceiling is not lifted until the projection boundary (3) and
write-back (4)/transients (5) exist. This ADR exists so that boundary is decided
deliberately rather than accreted.

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
         Phase 15 (heap GC) — required
              ↓
         Phase 16 (collections) ──┐
              ↓                    │
         Phase 17 (stdlib) ←───────┘
              ↓
         Phase 18 (native codegen) — final
```

## Rough size estimate

Frozen kernel (Zig) at completion of Phase 4: ~1500 LOC.
Frozen kernel at completion of Phase 14: ~3000 LOC (adds VM interpreter, AOT emitter, Mach-O/ELF readers).
Birnam-side (fact-shaped, mutable): unbounded — that's the whole point.
