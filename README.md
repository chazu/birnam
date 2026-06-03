# Birnam

A tiny Smalltalk-ish language running on a **relational fact-store substrate**, implemented in Zig.

Everything above the kernel is fact-shaped: grammar rules, AST nodes, method tables, bytecode, dispatch state, even error diagnostics live as facts in a HAMT/RRB-backed store. There are no bespoke data structures above the substrate — the language is built out of the same relational primitives it exposes. The result is a live, malleable image (define methods, mutate state, `:save` it, reload it, ship it as a standalone binary) in the Smalltalk tradition, but with a Datomic-style fact store underneath.

> Heritage: tagged pointers + the vtable-at-offset-`-1` trick from Ian Piumarta's [idst](https://www.piumarta.com/software/cola/); persistent collections (HAMT + RRB) à la Clojure; a fact store + time/history à la Datomic.

## Build

Requires **Zig 0.15.1**.

```
zig build
```

Produces `zig-out/bin/birnam` (the shell) and the module dylibs in `zig-out/lib/`:

| dylib | role |
|---|---|
| `libbirnam_peg.dylib` | PEG parsing engine (fact-shaped patterns + packrat memo) |
| `libbirnam_lang.dylib` | birnam grammar + desugar + bytecode compiler + eval + REPL |
| `libbirnam_query.dylib` | Datalog query module |
| `libbirnam_repl.dylib` | minimal REPL module (superseded by lang's REPL) |
| `libbirnam_hello.dylib` | smoke-test module |

The shell `dlopen`s modules from argv; each exports `birnam_module_init` and talks to the kernel through a C-ABI `IdApi` table. Order matters — load `peg` before `lang`.

## Run the REPL

```
./zig-out/bin/birnam \
    ./zig-out/lib/libbirnam_peg.dylib \
    ./zig-out/lib/libbirnam_lang.dylib
```

You get a `birnam>` prompt. Each line is parsed by the PEG engine, desugared to a typed AST, compiled to bytecode, and run on the VM. Results print as `=> value`. Ctrl-D exits.

## Tests

```
zig build test
```

376 tests across the kernel and modules.

## Language tour

Bare literals and identifiers evaluate directly; **message sends are wrapped in `[ ]`**.

```
birnam> 42
=> 42
birnam> "hello"
=> "hello"
birnam> 3.14
=> 3.14
birnam> [3 + 4]
=> 7
birnam> [3 squared]
=> 9
birnam> [[3 succ] squared]
=> 16
```

**Bindings** (`let`) and **assignment** (`:=`):

```
birnam> let x := 42
=> 42
birnam> [x squared]
=> 1764
birnam> x := 99
=> 99
```

**Blocks** are closures — `[:a | …]` takes params, `[| …]` takes none. They close over their enclosing scope:

```
birnam> let makeAdder := [:a | [:b | [a + b]]]
birnam> let add10 := [makeAdder value: 10]
birnam> [add10 value: 5]
=> 15
```

**Control flow** is message-based:

```
birnam> [[3 < 5] ifTrue: [| 999] ifFalse: [| 0]]
=> 999
birnam> let i := 0
birnam> [[| [i < 5]] whileTrue: [| i := [i + 1]]]
birnam> i
=> 5
birnam> let sum := 0
birnam> [1 to: 10 do: [:i | sum := [sum + i]]]
birnam> sum
=> 55
```

**Defining methods.** `[<recv> vtable]` returns a receiver's vtable; install a method on it with `method:do:`. The new method is live immediately for all values of that type:

```
birnam> [[3 vtable] method: #neg do: [| [self negate]]]
birnam> [7 neg]
=> -7
birnam> [[3 vtable] method: #pick:or: do: [:a :b | b]]
birnam> [99 pick: 10 or: 20]
=> 20
```

**Non-local return** `^` returns from the enclosing method (not just the block); `;` separates statements:

```
birnam> [[3 vtable] method: #early do: [| ^99; 100]]
birnam> [7 early]
=> 99
```

**Floats, comparisons, comments:**

```
birnam> [3.14 + 2.0]
=> 5.140000000000001
birnam> [3.0 < 5.0]
=> true
birnam> 42 // line comment
=> 42
birnam> [3 /* block comment */ + 4]
=> 7
```

Also: integers, floats (with mixed-type coercion via `asFloat`/`asInt`), strings, symbols (`#foo`, `#kw1:kw2:`), `true`/`false`/`nil`, iteration (`timesRepeat:`, `to:do:`, `whileTrue:`/`whileFalse:`).

## Saving and loading the image

REPL state — your bindings and installed methods — persists to a single self-describing `.brnimg` file and survives a process restart:

```
birnam> let answer := 42
birnam> [[3 vtable] method: #double do: [| [self + self]]]
birnam> :save app.brnimg
[birnam] saved app.brnimg (… facts, 2 modules)
```

```
# fresh process
birnam> :load app.brnimg
[birnam] loaded app.brnimg (… facts, … methods rebuilt)
birnam> answer
=> 42
birnam> [5 double]
=> 10
```

Pure data write — no compiler/toolchain needed to save. On load, the fact store is rebuilt and method closures are re-linked against the running kernel.

### Other REPL commands

| Command | Effect |
|---|---|
| `:save <path>` | snapshot the image to `<path>` |
| `:load <path>` | load an image into the running session |
| `:prune` | reachability GC: drop unreachable entities + collapse history |
| `:reset` | clear the top-level scope |
| `:in <path>` | file in a birnam source unit (atomic: aborts on first error, applies nothing) |
| `:in! <path>` | file in best-effort (evaluate every good form, report errors) |
| `:freeze <base>` | freeze the image's FFI wraps into a standalone wrapper module dylib |

## Calling C libraries (FFI)

Wrap an existing shared object and call into it — interactively or from a filed-in unit. `Module` is a prototype; sending `load:` mints a module object; `wrap:as:signature:` declares a method bound to a C symbol (dyncall signature string, e.g. `"d)d"` = `double → double`):

```
birnam> let m := [Module load: "/usr/lib/libSystem.B.dylib"]
birnam> [m wrap: #sqrt: as: "sqrt" signature: "d)d"]
birnam> [m sqrt: 144.0]
=> 12
birnam> [m symbols]                 // enumerate the lib's symbol table (load query module)
```

A **wrapper unit** is just a source file of such sends; file it in to install the whole binding:

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

Wraps are **declarative facts** (symbol name + signature + selector), so they persist across `:save`/`:load` and **re-link by symbol name** on load — the `dlopen`/`dlsym` is deferred to a commit step, never a stale cached address.

### Freezing a wrapper into a dylib

`:freeze` turns the live wraps into a standalone **wrapper module dylib**:

```
birnam> let lib := [Module load: "/usr/lib/libSystem.B.dylib"]
birnam> [lib wrap: #sqrt: as: "sqrt" signature: "d)d"]
birnam> :freeze foo
[birnam] froze 1 module(s), 1 wrap(s) -> foo.birnam + foo.zig
[birnam] compiled libfoo.dylib
```

It emits `foo.birnam` (canonical, re-editable source), `foo.zig` (a module that embeds it), and `birnam_module_abi.zig`, then `zig build-lib`s `libfoo.dylib`. Load that dylib after `peg`/`lang` and its `birnam_module_init` re-files-in the wraps — `dlopen`ing the alien lib and binding the symbols:

```
./zig-out/bin/birnam …/libbirnam_peg.dylib …/libbirnam_lang.dylib ./libfoo.dylib
birnam> [lib sqrt: 49.0]
=> 7
```

No new kernel ABI — the frozen module just replays the same `load:`/`wrap:` sends. Because wraps relink by name, the frozen dylib is portable across processes.

## Ahead-of-time compilation

`zig build aot` produces standalone executables with the image + modules linked in (no dylibs at runtime):

```
zig build aot
./zig-out/bin/birnam-aot              # full tower: REPL over the baked state
./zig-out/bin/birnam-aot-malleable    # pure-VM: parser/compiler stripped, runs baked bytecode
```

Both embed `src/aot_embed.brnimg`. The malleable build drops the parser, AST, and compiler (a frozen end-user app that runs bytecode but can't compile new source). This is an early slice of the full AOT surface — see `ROADMAP.md` Phase 14 for the planned `birnam --aot=<static|dynamic> [--strip=malleable] [--strip=history]` driver.

## Architecture

```
birnam (shell)              dlopen modules, drive stdin
  ├─ kernel (frozen Zig)    fact store (EAVT/AEVT/AVET/VAET + history),
  │                         tagged-pointer Refs, object table, HAMT + RRB,
  │                         interner, bytecode VM + dispatch, image, prune
  ├─ libbirnam_peg          PEG engine
  └─ libbirnam_lang         grammar → desugar → bytecode compiler → VM
```

- **Everything is a fact.** `[entity attribute value]` triples in a persistent, time-indexed store. The materialized object table (structs with vtable at offset `-1`) is a cache over the canonical facts.
- **Tagged-pointer Refs** distinguish small integers, eids, keywords, pointers, and strings in a single 64-bit word.
- **Bytecode VM.** User methods compile to bytecode (stored as facts) and run on a stack VM with an inline method cache; the language switched off the tree-walker to the VM in Phase 11.
- **Module ABI is append-only.** Each module carries its own mirror of the C-ABI `IdApi` struct and talks to the kernel through function pointers (Zig has no cross-dylib type imports).

## Documentation

| Doc | Contents |
|---|---|
| `VISION.md` | the locked design vision (relational substrate, tenets) |
| `ROADMAP.md` | phase-by-phase work plan + status (Phases 0–13 done, 14 in progress) |
| `DEMO.md` | end-to-end REPL walkthrough |
| `DATALOG.md` | the Datalog query system |

## References

- Ian Piumarta, [Open, Extensible Object Models](https://www.piumarta.com/software/cola/)
- Rich Hickey, Clojure persistent data structures (HAMT) + Datomic (fact store, history)
- Adele Goldberg, David Robson, *Smalltalk-80: The Language and its Implementation*
