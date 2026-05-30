# birnam — demo

End-to-end demonstration of the relational-fact-store substrate
running a tiny Smalltalk-ish language built on top of itself.

## What this demo proves

Birnam parses, desugars, and evaluates source code through a stack
that is *entirely fact-shaped* — grammar rules, AST nodes, method
tables, dispatch state, even error diagnostics live as facts in a
HAMT-backed store. No bespoke data structures above the kernel.

The pipeline:

```
source string
  → PEG engine (fact-shaped patterns + packrat memo)
  → concrete syntax tree (entities with :ast-rule-name, byte spans,
    linked-list children)
  → desugar pass (CST → typed AST with per-kind vtables)
  → eval (send-dispatched per AST kind-vt, primitives on SmallInt)
  → result Ref
```

Every stage above the kernel substrate is malleable — patterns,
grammar rules, AST kind-vt methods, SmallInt primitives are all
asserted as facts at module-init and can be retracted + replaced
live without a kernel restart.

## Build

```
zig build
```

Produces:

- `zig-out/lib/libbirnam_peg.dylib`   — PEG engine façade
- `zig-out/lib/libbirnam_lang.dylib`  — birnam grammar + desugar + eval
- `zig-out/lib/libbirnam_query.dylib` — datalog query module (Phase 7)
- `zig-out/lib/libbirnam_repl.dylib`  — Phase 6 REPL (Phase 10.5 replaces)
- `zig-out/lib/libbirnam_hello.dylib` — smoke-test module

## Run the demo

### Interactive REPL (preferred — Phase 10.5)

```
./zig-out/bin/birnam \
    ./zig-out/lib/libbirnam_peg.dylib \
    ./zig-out/lib/libbirnam_lang.dylib
```

You get a `birnam>` prompt. Each line is parsed by the PEG engine,
desugared into a typed AST, and evaluated by the send-dispatched
eval methods. Type expressions; results print as `=> value`. Lines
starting with `:` are debug commands (`:reset` clears the scope
stack — others land in follow-ups). Ctrl+D exits.

Try these in order:

```
birnam> 42
=> 42

birnam> [3 squared]
=> 9

birnam> [5 succ]
=> 6

birnam> [[3 succ] squared]
=> 16

birnam> [[3 vtable] method: #neg do: [| [self negate]]]
=> <eid:68>

birnam> [7 neg]
=> -7

birnam> [12 neg]
=> -12

birnam> [[3 vtable] method: #pick:or: do: [:a :b | b]]
=> <eid:68>

birnam> [99 pick: 10 or: 20]
=> 20

birnam> nil
=> nil

birnam> true
=> true

birnam> "hello"
=> "hello"
```

Phase 10.4.5 — `let` bindings, `:=` assignment, closure capture,
control flow, binary ops:

```
birnam> let x := 42
=> 42

birnam> [x squared]
=> 1764

birnam> x := 99
=> 99

birnam> [x + 1]
=> 100

birnam> let counter := 0
=> 0

birnam> counter := [counter + 1]
=> 1

birnam> counter := [counter + 1]
=> 2

birnam> counter := [counter + 1]
=> 3

birnam> counter
=> 3

birnam> [3 + 4]
=> 7

birnam> [[3 < 5] ifTrue: [| 999] ifFalse: [| 0]]
=> 999

birnam> let makeAdder := [:a | [:b | [a + b]]]
=> <eid:NNN>

birnam> let add10 := [makeAdder value: 10]
=> <eid:NNN>

birnam> [add10 value: 5]
=> 15
```

That last sequence is the classic closure-over-enclosing-scope demo:
`makeAdder` is a 1-param block that returns a 1-param block. The
returned block closes over `a` from the outer activation. When
`makeAdder value: 10` runs, the inner block captures the scope where
`a = 10`. Subsequent `add10 value: 5` finds `a = 10` two scopes up
the chain via `:scope-parent` linkage, returns `10 + 5 = 15`.

Everything above runs through the same pipeline: PEG parse → CST →
typed AST → eval, with SCOPE_VT and BINDING_VT entities materialized
as facts in the store at each block invocation.

What just happened:

- The `[[3 vtable] method: #neg do: [| [self negate]]]` line
  *installed a user-defined method* on the kernel-fixed SmallInt
  vtable from inside the running image. The block `[| [self
  negate]]` evaluated to a kernel-shaped closure whose body is the
  AST entity for `[self negate]`.
- `[7 neg]` then dispatched against the installed method. The
  block-invocation shim bound `self` to `7`, evaluated the body,
  and returned `-7`.
- `[[3 vtable] method: #pick:or: do: [:a :b | b]]` installed a
  2-parameter block as a compound-keyword method. `[99 pick: 10
  or: 20]` invoked it with `a = 10`, `b = 20`, returned `b`.

The image you're typing into is the same fact store the test runner
uses; you're just driving it via stdin instead of a Zig test.

### Test-runner mode (CI)

```
zig build test
```

Final line should read `Build Summary: 73/73 steps succeeded;
277/277 tests passed`. The DEMO-labelled cases in the test output
exercise the same path as the REPL — useful for CI regression
checks. List them with:

```
zig build test --summary all 2>&1 | grep "DEMO"
```

## What to look at (suggested walk)

For a recorded or live demo, the punchy story arc:

### 1. The vision (5 minutes)

Read `VISION.md`. The two non-negotiable claims:

- Tenet 0: "The image is a database." Everything — code, methods,
  vtables, history — lives in a HAMT+RRB-backed relational fact
  store. No bolt-on log, no separate metadata. (`VISION.md §6`.)
- Tenet 1: "Everything is an object." Vtables ARE prototypes
  (Piumarta/Self lineage). No class/instance split. (`VISION.md
  §94-105`.)

### 2. The kernel (10 minutes)

Walk `src/kernel/` top to bottom:

- `hamt.zig`, `rrb.zig` — two persistent collections, ~500 LOC each
- `ref.zig` — 3-bit tagged pointers (pointer / SmallInt / EID /
  keyword / string / reserved)
- `fact_store.zig` — five indexes (EAVT/AEVT/AVET/VAET/MEAT),
  6-column tuples with module-provenance
- `object_table.zig` — EID → Ref HAMT with hotness counters
- `send.zig` — variadic dispatch (`Imp = fn(*Send, [*]const Ref,
  usize) Ref`) shipped in Phase 10.2.5 via β refactor of 37 method
  impls. mcache hit on hot path, fact-store lookup on cold.
- `bootstrap.zig` — brings up vtables, asserts seed facts, installs
  ~5 native kernel methods. Stage 17 reserves the Phase 10.1 EID
  block.

Show the line where the kernel becomes alive:

```
send_mod.initDispatch(store, table, allocator);
```

After that one line, `[obj vtable]` works via the full dispatch path.

### 3. The PEG engine (5 minutes)

Walk `src/kernel/peg.zig`:

- Patterns are entities. `:pattern-kind` is an interned keyword
  (`:literal`, `:seq`, `:choice`, `:star`, …). 12 combinators.
- `matchPattern` is a recursive Zig walker that reads `:pattern-kind`
  off the receiver and dispatches.
- Packrat memo lives in an `AutoHashMap(MemoKey, MemoEntry)` —
  hot-path cache, NOT fact-shaped. (Phase 8 derived-state separation.)
- CST nodes get `:ast-rule-name`, `:ast-byte-start/end`,
  `:ast-first-child` + `:ast-next-sibling` linked-list children.
- Failure emits an `ERROR_VT` entity with `:peg-position` +
  `:peg-expected :many`.

User-facing surface via send-dispatch on `PEG_VT` / `GRAMMAR_VT`:

```
[PEG newGrammar]              → grammar entity EID
[PEG literal: "true"]         → pattern entity EID
[PEG seq: a b c …]            → variadic pattern entity EID
[PEG charset: 'a' 'z' '0' '9'] → range-list pattern entity EID
[grammar defineRule: "ident" as: pattern]  → rule entity EID
[grammar parse: source from: "start"]      → AST root EID | ERROR_VT EID
```

### 4. The grammar (3 minutes)

Walk `src/modules/lang/lang.zig` `buildGrammar()`. The grammar is
~20 PEG rules, asserted from Zig via send-dispatched PEG calls. No
DSL file — `[grammar defineRule: ...]` sends do the assembly.

This is the same surface a user would use to mutate the grammar at
runtime, which is the point: there's no privileged "the grammar"
that lives somewhere other than the fact store.

Show how a single rule like `bracket-keyword` decomposes into a
sequence of literal + ws + expression + kw-arg+ + ws + literal
patterns, each of which is itself a fact.

### 5. The pipeline (5 minutes)

Walk through `parseExprImpl` in `src/modules/lang/lang.zig`:

1. Receive source string Ref.
2. `[grammar parse: src from: "start"]` returns CST root EID.
3. `desugar(cst_eid)` walks the CST, builds typed AST with kind-vts.
4. Returns typed AST root EID.

Then `[ast eval]` dispatches per kind-vt:

- `AST_LITERAL_VT eval` → return `:ast-value`
- `AST_UNARY_SEND_VT eval` → eval receiver, send selector
- `AST_KEYWORD_SEND_VT eval` → eval receiver, eval args, sendN
- `AST_SELF_VT eval` → return `s.state`

Each eval method is a regular fact installed at lang-dylib init —
user code can override any of them via `[<kind-vt> method: #eval
do: [...]]` once Phase 10.4 method-install ships.

### 6. The substrate moment (2 minutes) — drive it live

Open a terminal, run `./zig-out/bin/birnam ./zig-out/lib/
libbirnam_peg.dylib ./zig-out/lib/libbirnam_lang.dylib`, and type at
the prompt. The session in the "Run the demo" section above is the
walk script. Show the install line, then dispatch through the user-
defined method, then add a second method with parameters and
dispatch it. Each prompt-and-result pair is one full trip through:

```
src_ref = mkString("[3 squared]")
ast_ref = send1(<lang_module>, parseExpr:, src_ref)
        → grammar.parse:from: → CST entities asserted under M_PEG
        → desugar() walks CST, asserts typed AST under M_LANG
val     = send0(ast_ref, eval)
        → AST_UNARY_SEND_VT.eval
            → recurse send0(receiver_ast, eval)
                → AST_LITERAL_VT.eval → returns SmallInt(3)
            → send0(SmallInt(3), squared)
                → SMALLINT_VT.squared → returns SmallInt(9)
val == SmallInt(9)
```

Every arrow above is a fact-store query + a method dispatch. Every
intermediate result is an entity (or an immediate). The CST nodes
and AST nodes are *still in the store* after eval completes — you
can query history to see what was parsed, what shape it had, what
got asserted under which module's provenance.

### 7. The roadmap (2 minutes)

Open `ROADMAP.md`. Show:

- ✓ Phases 0–9.5 (substrate + datalog + alien-code absorption)
- ✓ Phase 10.0 (PEG engine)
- ✓ Phase 10.1 (grammar + slot block)
- ✓ Phase 10.2.5 (variadic dispatch — kernel-deep edit per
  ADR-10-Send-Arity)
- ✓ Phase 10.2 (parser/desugar) + 10.3 minimal (eval)
- ✓ Phase 10.4 (method install via `[vt method:#sel do:[block]]`)
- ✓ Phase 10.4 follow-up (block params `[:x :y | body]` + compound
  symbols `#kw1:kw2:`)
- ✓ Phase 10.5 (REPL — lang dylib drives the stdin loop)
- ✓ Phase 10.x (binary ops `+ - * / < > =`, `ifTrue:ifFalse:`,
  block `[block value: x]`)
- ✓ Phase 10.4.5 S1-S4 (fact-shaped scope chain via SCOPE_VT +
  BINDING_VT entities, closure capture by reference, `let`,
  `:=` assignment, REPL persistence across lines)
- 🗓 Phase 10.4.5 S5 — parse-time slot resolution (speed layer,
  optional perf optimization)
- 🗓 Phase 10.3 rest — `whileTrue:`, non-local return `^`
  (ADR-10-E4 `g_unwinding`), `;` statement separator, comments,
  float literals
- 🗓 Phase 11 — bytecode VM
- 🗓 Phase 12 — image snapshot
- 🗓 Phase 13 — pruning + reachability GC
- 🗓 Phase 14 — AOT

## What's NOT in the demo yet

(So the audience knows what's coming.)

- **`whileTrue:` + iteration primitives.** Eval methods on Block
  for value-loops. Demo-able once landed.
- **Non-local return `^`.** ADR-10-E4: `g_unwinding` side-channel +
  `:ast-method-frame` back-pointer. Spec-locked, not implemented.
- **`;` statement separator + multi-stmt block body.** Eval
  AST_STMT_SEQ_VT walks linked-list of statements.
- **Comments + float literals.** Grammar extensions; eval reuses
  existing literal path.
- **Parse-time slot resolution (Phase 10.4.5 S5).** Speed-layer
  optimization. Truth-layer (S1-S4) shipped — every binding lives
  as a BINDING_VT entity. S5 annotates AST_VAR_REF_VT nodes with
  (depth, slot) for fast lookup; eval falls back to fact-query
  walk otherwise (current behavior).
- **Multi-line REPL input.** Currently one expression per line.
  T2-locked path: PEG furthest-failure-at-EOF detection. Land
  alongside scope-chain work.
- **Image snapshot / load.** Phase 12 — fact-store roots + mmappable
  segment files. Without it, every demo invocation rebuilds the
  whole tower from scratch. (For the demo, this is actually a *
  feature* — every birnam invocation comes up identical.)

## What's interesting beyond the demo

- The fact store is queryable as Datalog. `[db findAll: ?vt where:
  #([?vt :name 'Point'])]` (Phase 7).
- History is free and as-of-able. Every assert carries a `tx`
  column; querying as-of an older `tx` shows the world at that
  point.
- Dyncall-backed FFI (Phase 9). `[Module load: "/usr/lib/…"]` mints
  a Module entity with `[mod wrap: 'name' as: 'sel' signature:
  'd)d']` installs FFI shims. Stale-pointer detection on unload.
- Predicates + functions as fact entities (Phase 7.4). Override-able
  bind via `:bind` attribute (Phase 7.5).
- Schema-driven multi-cardinality attributes (Phase 9.5.2,
  ADR-9.5-D2 polymorphic currentValue).

## Files to skim if you have 30 seconds

- `VISION.md` — design rationale
- `ROADMAP.md` — phase-by-phase plan + locked decisions + ADRs
- `src/kernel/peg.zig` — engine + send-dispatched constructors
- `src/modules/lang/lang.zig` — grammar build + desugar + eval +
  SmallInt primitives, all in one file. ~900 LOC. The whole
  malleable tower above the substrate.
- `src/kernel/lang_module_test.zig` — DEMO cases
