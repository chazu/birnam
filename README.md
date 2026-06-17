# Birnam

> If you will permit a small philosophical digression ... Only things exist, and their relations.
>
> The first thing a society did upon entering an era of technological acceleration was to disturb the living environment...This did not have to mean stagnation: the environment was an obedient guardian, and to the extent that it was able, it could change itself according to wishes or demands. Could it change so that “progress” would take place? We had no answer for that...Could one call [progress] a situation in which intellectual, creative, cognitive, and constructional activities were so specialized that in each profession one dug deeper in an ever-narrower plot of ground? If machines counted faster and better than a living being, why should the living being count?...Such an artificial environment would be no one, like a meadow, forest, or steppe—with the difference that it would grow and blossom not on its own, not for itself, but for someone. For beings. Would they become stupid from this, turning into dull-witted gluttons that whiled away their hours with toys provided by the planetary guardian? Not necessarily. It depended on the point of view.
>
> Stanislaw Lem, _Fiasco_

A weird, late-bound, dynamic language intended to be completely malleable, in the style of Ian Piumarta's `idst`. Probably does not fulfill this or any other goal.

## Why

I've wanted to craft a personal computing environment like the one described in Piumarta's COLA/ALBERT papers for a long time. I thought `idst` was brilliant, and thought in particular that the ability to absorb C-ABI compliant dynamic libraries, wrap them in a lightweight dispatching shell and use their full power was a cool idea.
Claude showed up. I took a few stabs at realizing the idea, and spent a lot of time playing with idst. I also wrote two smalltalks. I played with cuelang, implemented datalog poorly twice, played with tuplespaces and "wrote" agentic workflow systems using combinations of the aforementioned technologies.
Then this guy incepted me with the meme of `extensional object identity` and I ended up reading The Third Manifesto.
I asked Claude about ents - still haven't found a reason to really grok those, might this do? Claude said no dawg u just need hash-mapped array tries and relaxed radix balanced trees and you can do all kinds of cool shit.
I said "Can i still do the idst thing? vtable at -1?"
Claude said "yeah dude sure I don't care do whatever you want"...No actually it probably said "THATS A BRILLIANT IDEA" or some shit.
Then I said something like "LFG"
Then I had this thing, which Claude says is:

```
A tiny Smalltalk-ish language running on a **relational fact-store substrate**, implemented in Zig.

Everything above the kernel is fact-shaped: grammar rules, AST nodes, method tables, bytecode, dispatch state, even error diagnostics live as facts in a HAMT/RRB-backed store. There are no bespoke data structures above the substrate — the language is built out of the same relational primitives it exposes. The result is a live, malleable image (define methods, mutate state, `:save` it, reload it, ship it as a standalone binary) in the Smalltalk tradition, but with a Datomic-style fact store underneath.

> Heritage: tagged pointers + the vtable-at-offset-`-1` trick from Ian Piumarta's [idst](https://www.piumarta.com/software/cola/); persistent collections (HAMT + RRB) à la Clojure; a fact store + time/history à la Datomic.
```

## What

Its all clanker-made right now. Only as of the writing of this commit am I attempting to use it - thats not true, I used it before we built the `lang` module from the repl, but since then I've been [balling the jack](https://en.wikipedia.org/wiki/Ballin%27_the_Jack) to get it done-except-AOT-compilation before I really mess with it.

So a lot of shit isn't done:

* Any documentation
* Really any standard library - there's strings, and combinators and some other stuff but we still need:
  - http
  - files outside the kernel
  - i/o streams
  - external process management
  - literally everything else
* emacs mode
* lsp (what the fuck does this even look like? i have no idea)


## Get started in 60 seconds

**Option A — download a release** (arm64 macOS only, for now):

```sh
# grab the latest tarball from https://github.com/chazu/birnam/releases
curl -L <tarball-url> | tar xz
xattr -dr com.apple.quarantine birnam-*-aarch64-macos   # clear Gatekeeper
birnam-*-aarch64-macos/bin/birnam                        # zero-config REPL
```

The bundle is self-contained: `bin/birnam` finds its modules in the sibling
`lib/`, so it runs from wherever you unpack it — no install step, no env var.

**Option B — build from source** (any platform with **Zig 0.15.1**):

```sh
zig build install --prefix ~/.local      # or: mise run install
export PATH="$HOME/.local/bin:$PATH"
birnam                                     # zero-config REPL
```

Either way you land at a `birnam>` prompt with `peg` + `lang` loaded. Poke at it:

```
birnam> [3 + 4]
=> 7
birnam> [6 squared]
=> 36
birnam> let x := 10
=> 10
birnam> [x + 5]
=> 15
```

(Top-level message sends are bracketed — `[3 + 4]`, not `3 + 4`.) Then grow and
keep a world:

```
birnam> :in playground.birnam    # evaluate a file (multi-line, atomic)
birnam> :save mywork.bim         # snapshot the whole image
birnam> :load mywork.bim         # ...and restore it later
```

Drop a `birnam.toml` beside your work to pin the module set and auto-resume an
image (see [Project config](#project-config--birnamtoml) below), and a bare
`birnam` in that directory starts up right where you left off. The full
language tour lives in [`DEMO.md`](DEMO.md).

> Heads up: only arm64 macOS has a prebuilt binary today. On Linux / Intel
> macs, build from source (Option B) — the cross-platform port is tracked
> separately.


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

### Install

```
zig build install --prefix ~/.local
```

Installs exactly `bin/birnam` (the shell) and `lib/libbirnam_*.dylib` (the
modules) under the prefix — nothing else. Because the shell probes its own
`../lib`, a prefix install is **zero-config**: `~/.local/bin/birnam` finds its
modules in `~/.local/lib` with no environment variable set. (`$BIRNAM_PATH`
remains useful to point at an install prefix elsewhere.)

## Run the REPL

Modules are named by **bare name** — `lang` resolves to `libbirnam_lang.dylib`
along the dylib search path. With no arguments, `birnam` loads the default set
(`peg lang`):

```
birnam                       # default: peg + lang, then the REPL
birnam peg lang query        # explicit module set (full dylib paths still work)
birnam --bare                # load nothing (bare kernel)
birnam init                  # scaffold birnam.toml + playground.birnam here
birnam playground.birnam     # run a script (file-in, then exit)
birnam -i playground.birnam  # run a script, then drop into the REPL
birnam -e '[3 + 4]'          # evaluate one expression, then exit
```

You get a `birnam>` prompt. Each line is parsed by the PEG engine, desugared to a typed AST, compiled to bytecode, and run on the VM. Results print as `=> value`. Type `:help` for the command list. Ctrl-D exits.

Any argument ending in `.birnam` is a script (filed in, statements separated by `;`); `-e EXPR` evaluates one expression. Both run after the modules load and then exit, unless `-i` keeps the session open afterward. `birnam init` writes a starter `birnam.toml` + `playground.birnam` (never clobbering existing files).

### Resolving modules

A module name is resolved to a dylib by searching, in order:

1. any `dylib_paths` from `birnam.toml` (relative to the config file)
2. directories in `$BIRNAM_DYLIB_PATH` (`:`-separated) — dirs that *directly* hold dylibs
3. install roots in `$BIRNAM_PATH` (`:`-separated, Go-style) — each probed at `<root>/lib` then `<root>`
4. the current directory and everything beneath it (recursive)
5. the directory holding the `birnam` binary, then its sibling `../lib` — the standard `<prefix>/bin/birnam` + `<prefix>/lib/` install layout, so a packaged install resolves modules with **no env var set**

For each directory the name is tried as-is, then decorated: `lang` → `liblang.dylib` → `libbirnam_lang.dylib` (the convention the bundled modules follow). A name that already points at a file is used verbatim, so explicit paths keep working.

`$BIRNAM_DYLIB_PATH` lists exact dylib directories; `$BIRNAM_PATH` lists install *roots* and expects the dylibs under `<root>/lib` — point it at an install prefix (or a `zig-out`) and modules resolve without naming the `lib/` dir.

### Project config — `birnam.toml`

A `birnam.toml` found by walking from the current directory up to the
filesystem root (git-style; first match wins) pins a project's module set,
extra search directories, and a snapshot to auto-load — so a bare `birnam`
inside the project tree starts up ready to go:

```toml
modules     = ["peg", "lang", "query"]   # ordered load list (bare names or paths)
dylib_paths = ["zig-out/lib"]            # extra search dirs, relative to this file
image       = "session.bim"              # snapshot :load'd after the modules
```

It's a deliberately small TOML subset: string and string-array values, `#`
comments, `[section]` headers ignored. Relative `dylib_paths` and `image` are
resolved against the config file's own directory, so a project is
location-independent.

Precedence for the module set: **explicit arguments > `birnam.toml` > the
default (`peg lang`)**. `--bare` overrides everything and loads nothing. The
config `image` is auto-loaded only on a bare (no-module-argument) invocation —
passing modules on the command line is treated as a custom session and won't
drag the project snapshot in.

## Tests

```
zig build test
```

379 tests across the kernel and modules.

## Benchmarks

Two microbenchmark harnesses measure the kernel's hot paths — dispatch, field
reads, and fact writes — so the performance model is grounded in numbers rather
than guesses. They are **safe by construction**: an earlier ad-hoc attempt to
benchmark by running million-iteration mutation loops in the live REPL exhausted
memory and crashed the machine (every `assert`/`retract` path-copies five HAMT
indexes and appends never-freed history, so unbounded mutation grows memory
without bound). The harnesses cap themselves so that can't happen again.

Both build to `zig-out/bin/` and are **not** auto-run by `zig build bench` (which
only runs the data-structure microbenchmarks). Run them explicitly, always under
`timeout`, starting with a small iteration count and watching the printed peak
before scaling up.

### `kernel_bench` — reads + dispatch (non-growing)

```
zig build kernel-bench
timeout 30 ./zig-out/bin/kernel_bench 1000000      # argv[1] = iterations
```

Boots the kernel and times warm dispatch and field reads against native
baselines. Its hot loops allocate nothing (results are immediates / already
stored Refs), so memory stays flat at any iteration count. Safety: all
allocation goes through a `FixedBufferAllocator` over a single 256 MiB slice —
physically bounded, so a runaway hits `OutOfMemory` and exits clean. (macOS
ignores `ulimit -v`, so the cap must live in-process.) It also prints a
breakdown (`table.get`, `vtableEidOf`, `mcache.lookup`, `bind`) to attribute
dispatch cost.

### `kernel_write_bench` — fact writes (the memory-growing class)

```
zig build write-bench
timeout 30 ./zig-out/bin/kernel_write_bench distinct 10000   # argv[1]=mode argv[2]=count
```

Modes: `distinct` (assert fresh facts — adding data), `churn` (re-assert one
slot — history grows), `assign` (retract+assert, the `:=` pattern),
`churn-pruned` (churn with periodic `compact` to prove mutation stays bounded).

Because writes grow memory, this harness uses a **capped allocator** — a real
allocator (forwards `free`, so `compact` reclamation works) wrapping a
`GeneralPurposeAllocator` (matching the shell/aot binaries) with a hard 512 MiB
outstanding-bytes ceiling. Past the ceiling, allocation fails and the bench
aborts cleanly with `[HIT CAP]` instead of taking the machine down. Each run
prints peak bytes; run one mode per process on a fresh boot.

### Measured (ReleaseFast, arm64; see ADR-perf-1 in `ROADMAP.md`)

| Operation | Cost | vs native |
|---|---|---|
| warm dispatch (`send0`) | ~8 ns/op | ~6.4× |
| field read (`currentValue`) | ~20 ns/op | ~10.3× |
| field write (`assert`) | ~1.8 µs/op | ~100–360× |

A write costs roughly 100× a read. Dispatch is not a bottleneck; the levers are
the field read and the write path. The full analysis, including the
projection-vs-representation decision it drives, is **ADR-perf-1** in
`ROADMAP.md`.

> Benchmarking discipline: always run under `timeout`, start small and watch the
> printed peak, run one bench per process, and never run an un-pruned mutation
> loop. Isolate the primitive from method-body work — benchmarking a method
> whose body itself reads state once produced a dispatch number ~4× too high.

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

**Errors are values (Result model).** A failed operation returns a first-class error value instead of crashing or silently yielding `nil`. An error **propagates**: sending it any message returns the error, so a chain short-circuits. Unhandled errors surface as a `! error:` line:

```
birnam> [#[1 2 3] at: 99]
=> error: vector index out of bounds
birnam> nonesuch
[lang] ! error: undefined variable: nonesuch
birnam> [10 / 0]
[lang] ! error: divide by zero
```

Catch with `on:do:`, run cleanup with `ensure:`, raise with `error`/`error:`. Inspect with `message`/`kind`/`isError`:

```
birnam> [[| [10 / 0]] on: #zeroDivide do: [:e | 0]]
=> 0
birnam> [[| [10 / 0]] on: #Error do: [:e | [e message]]]    // #Error = catch any
=> "divide by zero"
birnam> [[| risky] ensure: [| [stream close]]]              // cleanup runs either way
birnam> ["disk full" error: #io]                            // raise your own
```

> ⚠️ **Eager send vs. block.** A bracketed `[recv msg]` is a **message send and runs immediately** — `[10 / 0]` evaluates *now* and yields the divide-by-zero error. A **block** is `[| … ]` (no params) or `[:x | … ]` (params): the leading `|` makes it a *deferred* value. So anything that should run *under* a handler — `on:do:`, `ensure:`, `ifTrue:`, `whileTrue:` — must be a block: write `[[| [10 / 0]] on: #zeroDivide do: …]`, **not** `[[10 / 0] on: …]` (the latter sends `on:` to the already-computed error). Every send needs brackets; `10 / 0` on its own is a parse error.

**I/O.** `File` does data read/write (returns an error value on failure — catch it or let it propagate); `System` does clean stdout/stderr, stdin, and command-line arguments:

```
birnam> [File write: "hi\n" to: "out.txt"]
birnam> [File read: "out.txt"]
=> "hi\n"
birnam> [File exists: "out.txt"]
=> true
birnam> [[| [File read: "missing"]] on: #fileNotFound do: [:e | "default"]]
=> "default"
birnam> [System println: "clean stdout, no prefix"]      // also print: (no newline), error: (stderr)
birnam> [System readLine]                                 // next line from stdin, or nil at EOF
birnam> [System arguments]                                // tokens after `--` on the command line
=> #[]
```

`print` (any value) goes to the prefixed REPL log on **stderr**; `System print:`/`println:` are **clean stdout** for program output. Pass program arguments after `--`: `birnam peg.dylib lang.dylib -- alpha beta` makes `[System arguments]` yield `#["alpha" "beta"]`.

## Saving and loading the image

REPL state — your bindings and installed methods — persists to a single self-describing `.bim` file and survives a process restart:

```
birnam> let answer := 42
birnam> [[3 vtable] method: #double do: [| [self + self]]]
birnam> :save app.bim
[birnam] saved app.bim (… facts, 2 modules)
```

```
# fresh process
birnam> :load app.bim
[birnam] loaded app.bim (… facts, … methods rebuilt)
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
| `:eval-mode [bytecode\|treewalk]` | switch how top-level statements + method installs evaluate (default `bytecode`); bare `:eval-mode` prints the current mode |

By default the REPL compiles each top-level line (and filed-in form) to bytecode and runs it on the VM, so hot top-level loops don't pay the per-statement scope-fact cost; cross-line `let` bindings and globals still resolve normally. `:eval-mode treewalk` forces the simpler AST tree-walker for both top-level statements and user-method installs — an inspectable fallback when debugging a suspected miscompile or comparing the two evaluators. The setting lasts for the session only (it is not saved into images).

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

Compile any image into a standalone executable (kernel + modules + data linked in, no dylibs at runtime) with the `birnam --aot` driver:

```
# save an image, then freeze it into a binary
birnam … :save app.bim
birnam --aot=static  app.bim -o myapp                 # full tower: REPL over baked state
birnam --aot=static  --strip=malleable app.bim --entry=double:5 -o myapp
                                                         # pure-VM one-shot: parser/compiler/AST stripped
birnam --aot=static  --strip=history  app.bim -o myapp   # read-only frozen store (no tx-writer)
birnam --aot=dynamic app.bim -o dist/myapp            # portable bundle: dlopen modules at startup
```

The driver copies the image to a per-app embed path, runs the parameterized `zig build aot-build` (so it needs the repo + `zig` on PATH), and writes the binary to `-o`.

| flag | effect |
|---|---|
| `--aot=static` | kernel + modules + data linked in; no dylibs at runtime |
| `--aot=dynamic` | kernel + data linked in; modules `dlopen`'d at startup from the image manifest. The driver **stages the dylibs beside the binary** so the bundle is portable; the exe resolves them by basename via `$BIRNAM_DYLIB_PATH` (PATH-like; defaults to the cwd + below), then exe-relative |
| `--strip=malleable` | drop the parser/AST/compiler — a frozen app that runs bytecode but can't compile new source |
| `--strip=history` | freeze the fact store read-only (no tx-writer / history); the app reads its baked facts and never mutates them |
| `--entry=<sel>[:<recv>]` | send `<sel>` to `<recv>` and exit with the result. `<recv>` is a SmallInt (`double:5`) or a **top-level binding name** (`run:app` → send `#run` to whatever `app` is bound to) |

The only deferred slice is the mmap-relocation RO-*segment* optimization (load-time perf, Phase 12.5) — not an AOT capability. The fixed demo artifacts (`zig build aot` → `birnam-aot` / `birnam-aot-malleable`, embedding `src/aot_embed.bim`) still exist for the test fixtures.

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
| `ROADMAP.md` | phase-by-phase work plan + status (Phases 0–14 done; Phase 15 GC next) |
| `DEMO.md` | end-to-end REPL walkthrough |
| `DATALOG.md` | the Datalog query system |

## References

- Ian Piumarta, [Open, Extensible Object Models](https://www.piumarta.com/software/cola/)
- Rich Hickey, Clojure persistent data structures (HAMT) + Datomic (fact store, history)
- Adele Goldberg, David Robson, *Smalltalk-80: The Language and its Implementation*
