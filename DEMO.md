# birnam — demo

End-to-end demonstration of the relational-fact-store substrate running a tiny
Smalltalk-ish language built on top of itself — one that you can mutate live,
snapshot to disk, reload in a fresh process, wrap C libraries with, and freeze
into a standalone binary.

## What this demo proves

Birnam parses, desugars, compiles, and runs source code through a stack that is
*entirely fact-shaped* — grammar rules, AST nodes, bytecode, method tables,
dispatch state, even error diagnostics live as facts in a HAMT/RRB-backed
store. No bespoke data structures above the kernel.

The pipeline:

```
source string
  → PEG engine (fact-shaped patterns + packrat memo)
  → concrete syntax tree (entities: :ast-rule-name, byte spans, linked children)
  → desugar pass (CST → typed AST with per-kind vtables)
  → compile block bodies → bytecode (stored as facts)
  → eval: top-level AST is tree-walked; user methods run on the stack VM
  → result Ref
```

Every stage above the kernel substrate is malleable — patterns, grammar rules,
AST kind-vt methods, SmallInt primitives, user methods are all asserted as facts
and can be retracted + replaced live without a kernel restart. And because the
whole image *is* the fact store, it persists: `:save` writes it, `:load` brings
it back in a fresh process, `:freeze` compiles a wrapper into a dylib.

## Build

```
zig build
```

Produces `zig-out/bin/birnam` (the shell) and module dylibs in `zig-out/lib/`:

- `libbirnam_peg.dylib`   — PEG engine façade
- `libbirnam_lang.dylib`  — birnam grammar + desugar + bytecode compiler + eval + REPL
- `libbirnam_query.dylib` — datalog query module (Phase 7)
- `libbirnam_hello.dylib` — smoke-test module

## Run the demo

```
./zig-out/bin/birnam \
    ./zig-out/lib/libbirnam_peg.dylib \
    ./zig-out/lib/libbirnam_lang.dylib
```

A `birnam>` prompt. Each line is parsed by the PEG engine, desugared to a typed
AST, and evaluated; results print as `=> value`. Lines starting with `:` are
commands (see below). Ctrl-D exits. (Load `libbirnam_query.dylib` too for
collection accessors like `count`/`at:`.)

### 1. The language

```
birnam> 42
=> 42
birnam> [3 squared]
=> 9
birnam> [[3 succ] squared]
=> 16
birnam> "hello"
=> "hello"
```

`let` bindings, `:=` assignment, binary ops, control flow:

```
birnam> let x := 42
=> 42
birnam> [x squared]
=> 1764
birnam> x := 99
=> 99
birnam> [3 + 4]
=> 7
birnam> [[3 < 5] ifTrue: [| 999] ifFalse: [| 0]]
=> 999
```

Closures over the enclosing scope:

```
birnam> let makeAdder := [:a | [:b | [a + b]]]
birnam> let add10 := [makeAdder value: 10]
birnam> [add10 value: 5]
=> 15
```

Iteration + multi-statement blocks + comments:

```
birnam> let sum := 0
birnam> [1 to: 10 do: [:i | sum := [sum + i]]]
birnam> sum
=> 55
birnam> let i := 0
birnam> [[| [i < 5]] whileTrue: [| i := [i + 1]]]
birnam> i
=> 5
birnam> 42 // line comment
=> 42
birnam> [3 /* block comment */ + 4]
=> 7
```

Floats (with mixed-type coercion) + non-local return `^`:

```
birnam> [3.14 + 2.0]
=> 5.140000000000001
birnam> [3.14 asInt]
=> 3
birnam> [[3 vtable] method: #early do: [| ^99; 100]]
birnam> [7 early]
=> 99
birnam> [[3 vtable] method: #viablock do: [| [[| ^88] value]]]
birnam> [5 viablock]
=> 88
```

`early`'s body `^99; 100` is two statements; `^99` sets the unwind sentinel and
the method-frame catches it — `100` never runs. `viablock` invokes an inner
block whose body is `^88`; the value-frame *propagates* the sentinel, the
enclosing method-frame catches — classic Smalltalk non-local return.

### 2. Defining methods live

`[<recv> vtable]` is the receiver's vtable; install a method on it. Live
immediately for every value of that type — the install is just asserting facts,
and the body compiles to bytecode:

```
birnam> [[3 vtable] method: #neg do: [| [self negate]]]
birnam> [7 neg]
=> -7
birnam> [[3 vtable] method: #pick:or: do: [:a :b | b]]
birnam> [99 pick: 10 or: 20]
=> 20
```

### 3. The image is persistent — `:save` / `:load`

Your bindings and installed methods are facts; the whole image snapshots to a
single self-describing `.brnimg` file and survives a process restart.

```
birnam> let answer := 42
birnam> [[3 vtable] method: #double do: [| [self + self]]]
birnam> :save app.brnimg
[birnam] saved app.brnimg (56 facts, 2 modules)
```

```
# fresh process
birnam> :load app.brnimg
[birnam] loaded app.brnimg (56 facts, 1 methods rebuilt)
birnam> answer
=> 42
birnam> [5 double]
=> 10
```

Pure data write — no compiler/toolchain to save. On load the fact store is
rebuilt and method closures are re-linked against the running kernel.

### 4. Wrapping a C library — FFI from the REPL

`Module` is a prototype (no classes). `load:` mints a module object; `wrap:as:
signature:` declares a method bound to a C symbol (dyncall signature string,
`"d)d"` = `double → double`):

```
birnam> let m := [Module load: "/usr/lib/libSystem.B.dylib"]
birnam> [m wrap: #sqrt: as: "sqrt" signature: "d)d"]
birnam> [m sqrt: 144.0]
=> 12
birnam> [m symbols]            // RRB of symbol entities (needs query module)
```

Wraps are **declarative facts** (symbol name + signature + selector). The
`dlopen`/`dlsym` is deferred to a commit step, so they persist across `:save`/
`:load` and **re-link by symbol name** in the new process — never a stale
cached address.

### 5. File-in — `:in` / `:in!`

A wrapper (or any birnam unit) is a file of `;`-separated statements. File it in:

```
// libfoo.birnam
let foo := [Module load: "libfoo.dylib"];
[foo wrap: #open:  as: "foo_open"  signature: "p)p"];
[foo wrap: #close: as: "foo_close" signature: "p)i"]
```

```
birnam> :in libfoo.birnam
[lang] filed in 3 forms (0 errors)
```

`:in` is atomic — a parse/eval error aborts the unit and applies nothing (the
deferred `dlopen` never fires, so no half-open state). `:in!` is best-effort:
evaluate every good form, report the rest.

### 6. Freeze — compile a wrapper into a dylib

`:freeze` turns the live wraps into a standalone wrapper **module dylib**:

```
birnam> let lib := [Module load: "/usr/lib/libSystem.B.dylib"]
birnam> [lib wrap: #sqrt: as: "sqrt" signature: "d)d"]
birnam> :freeze foo
[birnam] froze 1 module(s), 1 wrap(s) -> foo.birnam + foo.zig
[birnam] compiled libfoo.dylib
```

It reconstructs canonical source from the wrap facts (`foo.birnam`), emits a
wrapper module (`foo.zig`) + the ABI mirror, and `zig build-lib`s `libfoo.dylib`.
Load that dylib after peg/lang and its `birnam_module_init` re-files-in the
wraps — `dlopen`ing the alien lib and binding the symbols:

```
./zig-out/bin/birnam …/libbirnam_peg.dylib …/libbirnam_lang.dylib ./libfoo.dylib
birnam> [lib sqrt: 49.0]
=> 7
```

No new kernel ABI — the frozen module replays the same `load:`/`wrap:` sends.

### REPL commands

| Command | Effect |
|---|---|
| `:save <path>` / `:load <path>` | snapshot / restore the image |
| `:prune` | reachability GC: drop unreachable entities + collapse history |
| `:reset` | clear the top-level scope (globals like `Module` survive) |
| `:in <path>` / `:in! <path>` | file in a source unit (atomic / best-effort) |
| `:freeze <base>` | freeze FFI wraps into a wrapper module dylib |

### Test-runner mode (CI)

```
zig build test
```

Should end `376/376 tests passed`. The DEMO-labelled cases exercise the same
path as the REPL:

```
zig build test --summary all 2>&1 | grep "DEMO"
```

## What to look at (suggested walk)

### 1. The vision

`VISION.md`. Two non-negotiable claims: **the image is a database** (code,
methods, vtables, history — all in one HAMT/RRB fact store, no bolt-on log) and
**everything is an object via prototypes** (vtables ARE prototypes, Piumarta/Self
lineage — no class/instance split).

### 2. The kernel — `src/kernel/`

- `hamt.zig`, `rrb.zig` — two persistent collections
- `ref.zig` — 3-bit tagged pointers (pointer / SmallInt / EID / keyword / string)
- `fact_store.zig` — five indexes (EAVT/AEVT/AVET/VAET/MEAT), 6-column tuples
  with module-provenance + per-fact tx/op history
- `object_table.zig` — EID → Ref HAMT with hotness counters
- `send.zig` — variadic dispatch; mcache hit on hot path, fact-store on cold
- `bytecode.zig` + `compiler.zig` — block bodies compile to bytecode (stored as
  facts); `dispatch.zig` runs the stack VM with an inline method cache
- `image.zig` — `.brnimg` snapshot/load (M_USER facts + interner + manifest)
- `prune.zig` — reachability mark-sweep + `compact`
- `ffi.zig` / `ffi_module.zig` — dyncall FFI + the `Module` prototype + declarative
  deferred-resolve wraps; `freeze.zig` — wrapper-dylib emission
- `bootstrap.zig` — brings up vtables + seed facts + native methods

The line where the kernel becomes alive: `send_mod.initDispatch(store, table, allocator)`.

### 3. The PEG engine — `src/kernel/peg.zig`

Patterns are entities (`:pattern-kind` keyword, 12 combinators). `matchPattern`
is a recursive walker; packrat memo is a hot-path cache (not fact-shaped). CST
nodes carry `:ast-rule-name`, byte spans, linked children. User surface:

```
[PEG newGrammar] / [PEG literal: "x"] / [PEG seq: …] / [PEG charset: …]
[grammar defineRule: "ident" as: pattern]
[grammar parse: source from: "start"]   → AST root EID | ERROR_VT EID
```

### 4. The grammar + pipeline — `src/modules/lang/lang.zig`

`buildGrammar()` asserts ~25 PEG rules via send-dispatched PEG calls — no DSL
file, the same surface a user mutates at runtime. `parseExprImpl` runs
parse → CST → `desugar` → typed AST; block bodies go through the compiler to
bytecode. The `language` prototype hosts file-in (`fileIn:` / `fileInSource:`).

### 5. The roadmap — `ROADMAP.md`

- ✓ Phases 0–9.5 — substrate, datalog, FFI, modules, schema
- ✓ Phase 9.5.6 — FFI reachable from the language, file-in, declarative
  relink-by-name wraps, **freeze**
- ✓ Phase 10 — PEG engine, grammar, desugar, eval, method install, scope chain
- ✓ Phase 11 — bytecode VM (user methods dispatch through the VM)
- ✓ Phase 12 — image snapshot (`:save`/`:load`)
- ✓ Phase 13 — pruning + reachability GC (`:prune`)
- 🟡 Phase 14 — AOT (static + `--strip=malleable` shipped; driver + variants pending)
- 🗓 Phase 15 — native codegen · Phase 16 — GC

## What's NOT in the demo yet

- **Multi-line REPL input.** The interactive prompt is still one expression per
  line; multi-statement units come in via file-in (`:in`), not inline.
- **Auto-generated wrappers.** `[mod symbols]` enumerates a dylib's symbols, but
  generating a wrapper from them needs a signature source (headers / sig DB /
  placeholders) — not built.
- **Native codegen (Phase 15) + GC (Phase 16).** Bytecode runs on a stack VM;
  process-lifetime allocations are currently just leaked.

## What's interesting beyond the demo

- The fact store is queryable as Datalog (Phase 7), with free, as-of-able
  history — every assert carries a `tx` column.
- Predicates + functions as fact entities (7.4); override-able `bind` (7.5).
- Schema-driven multi-cardinality attributes (9.5.2).
- AOT: `birnam --aot=static [--strip=malleable] <image.brnimg> -o <out>`
  compiles any image into a standalone executable (no dylib deps); driver
  copies the image to a per-app embed + runs `zig build aot-build`. The fixed
  `zig build aot` demo artifacts (`birnam-aot` / `birnam-aot-malleable`) remain
  for fixtures. `--aot=dynamic` + real history-strip pending.

## Files to skim if you have 30 seconds

- `VISION.md` — design rationale · `ROADMAP.md` — phases + ADRs
- `src/kernel/peg.zig` — engine + send-dispatched constructors
- `src/modules/lang/lang.zig` — grammar + desugar + eval + file-in, one file
- `src/kernel/ffi_module.zig` + `freeze.zig` — the Module prototype + freeze
- `src/kernel/lang_module_test.zig` — DEMO cases (incl. FFI / file-in / freeze)
