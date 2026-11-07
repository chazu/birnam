# Birnam

An Odin reboot of Birnam: a malleable `id` object universe with native compilation, stackful fibers, and source-bearing native images.

The native siphon boots one Id world and loads modules through an Oop-only C interface. A separately loaded Odin seed interpreter now evaluates the initial Birnam source subset, including lexical Blocks and source-defined Methods. A Birnam-authored PEG module now constructs grammars and labelled syntax trees through ordinary messages. Authored Lang now parses the seed subset and can replace the bootstrap parser. The first authored compiler emits loadable C modules for that subset; the native toolchain can rebuild its authored modules, while optimization remains later work.

```sh
make doctor
make build
./build/siphon ./build/seed.dylib --eval examples/seed/answer.birnam
./build/siphon ./build/seed.dylib --eval modules/peg.birnam --eval examples/peg/word.birnam
make verify
./build/siphon ./build/hello.dylib
```

The build is pinned to the initial Mac toolchain. See [the siphon implementation](docs/siphon.md) for prerequisites, protocols, module loading, acceptance evidence, and limitations.

## Compilation and bootstrapping walkthrough

The delivered toolchain has reached native self-hosting: the Birnam-authored parser and compiler compile themselves into native modules, then rebuild themselves with identical generated C. The Odin runtime remains underneath. Compiling a program and bootstrapping the compiler are separate sequences.

### 1. Build the native foundation

```sh
make doctor
make build
```

`doctor` checks the pinned Mac toolchain. `build` uses Odin and Clang to produce:

- **`build/siphon`** — the executable hosting the Id object universe: objects, Methods, lookup/dispatch, namespaces, GC integration, and native-module loading.
- **`build/seed.dylib`** — the small Odin parser/interpreter, plus runtime services such as Integers, Strings, collections, lexical environments, Blocks, and reflection.
- **`build/compile-request.dylib`** — a small C adapter that passes source bytes to the authored compiler.

The seed loads into the siphon's existing object world. To run source directly:

```sh
./build/siphon ./build/seed.dylib \
  --eval examples/seed/answer.birnam
```

This prints `42`. The seed parser constructs syntax objects; evaluation sends `evalIn:` to those objects. Arithmetic, branching, Block invocation, and source sends use ordinary message behavior.

### 2. Let Birnam take over parsing

Five authored modules form the toolchain, loaded in this order:

| Module | Responsibility |
|---|---|
| `modules/peg.birnam` | Parser combinators, matching, and labelled concrete syntax trees |
| `modules/lang.birnam` | Birnam's grammar, expressed using PEG |
| `modules/lang-syntax.birnam` | Translation from concrete syntax trees into executable syntax or compiler IR |
| `modules/compiler-ir.birnam` | Compiler IR objects and their reference evaluator |
| `modules/compiler-c.birnam` | C generation |

Initially, the Odin seed interprets these modules so their definitions exist. Parser takeover is explicit: `[Lang installOn: Seed]` replaces the seed's parsing behavior. Subsequent source evaluation uses authored PEG and Lang to parse, while the seed still evaluates the resulting syntax objects.

To exercise that transition:

```sh
./build/siphon ./build/seed.dylib \
  --eval modules/peg.birnam \
  --eval modules/lang.birnam \
  --eval modules/lang-syntax.birnam \
  --eval examples/lang/takeover.birnam \
  --eval examples/seed/answer.birnam
```

This also prints `42`. The takeover example installs Lang; the final source file is parsed through the authored front end.

### 3. Compile an individual program

```text
Birnam source
    ↓ Lang grammar + PEG matching
Concrete syntax tree
    ↓ Lang translator, configured with the compiler's IR constructors
IR objects: Literal, Name, Declaration, Send, Block, Vector, Sequence
    ↓ authored emitIn: behavior
C source
    ↓ Clang
Native .dylib
    ↓ siphon loads and initializes it
Program executes
```

The translator is shared: its configurable construction interface lets it create either seed syntax or compiler IR. The backend generates native `evalIn:` Methods for individual code objects. Those Methods execute children through messages and preserve live lookup. For a source send, the receiver and arguments execute in order, then the selected Method is bound and entered.

Native code is delivered, but this backend is still conservative and dispatch-heavy. It does not yet flatten a whole program into aggressively optimized C.

The default compilation command bootstraps the authored compiler through the interpreter:

```sh
python3 tools/compile.py examples/compiler/answer.birnam \
  -o build/answer.dylib

./build/siphon ./build/seed.dylib ./build/answer.dylib
```

The program prints `42`. Use `--emit-c` to stop at C output, or `--name Example` to choose the program's namespace export. Python orchestrates module loading and Clang; parsing, lowering, and C generation live in Birnam. See [compiler details](docs/compiler.md).

### 4. Bootstrap the compiler itself

The same compiler can compile all five toolchain modules:

```text
Odin seed interprets the authored toolchain
    ↓ compile the five Birnam modules → C → Clang
Stage 1 native toolchain
    ↓ compile the same five sources → C → Clang
Stage 2 native toolchain
    ↓ compile the same five sources → C
Stage 3 C output
```

The pipeline requires `Stage 1 C == Stage 2 C == Stage 3 C` for each module. There is no third library generation: the third emission checks that Stage 2 reproduces the same C. This establishes generated-C stability for these inputs, not binary reproducibility or a formal correctness proof.

Stage 2 construction and Stage 3 emission run with guards disabling the seed's parser/evaluator entry points and syntax evaluation Methods. This checks that native compilation does not fall back to interpreted source. General runtime services remain available.

```sh
make selfhost
```

This rebuilds the native foundation, runs the generations and comparisons, and uses Stage 2 to compile and run a separate application that prints `42`. Artifacts and the receipt go into `build/selfhost/`.

To use that native compiler for another program:

```sh
python3 tools/compile.py examples/compiler/answer.birnam \
  --native-dir build/selfhost/stage2 \
  -o build/native-answer.dylib

./build/siphon ./build/seed.dylib ./build/native-answer.dylib
```

Without `--native-dir`, compilation uses the interpreted bootstrap path. With it, the specified native libraries are required; missing libraries cause failure. The native request adapter reads source bytes from stdin and sends `Compiler compile:as:`, so this path needs no interpreted request script.

### 5. Start a compiled program

The siphon processes command-line arguments left to right, in one object world:

1. Bootstrap the core Id world.
2. Load `seed.dylib` to install runtime services.
3. Load the application library and call its `birnam_module_init_v1` entry.
4. The application constructs its code objects, connects runtime services and literal factories, publishes its program object, and executes its top level.

Running a compiled application requires neither PEG nor Lang nor the compiler. It still requires the runtime services currently packaged inside `seed.dylib`. This is native-module loading; saved-image loading remains future work.

### 6. Validate the stages and understand the boundary

```sh
make verify             # Runtime, seed, PEG, Lang, and compiler regression gates
make verify-selfhost    # Rebuild generations, compare C, run guarded acceptance
```

These are separate gates. The [recorded bootstrap validation](docs/selfhosting-validation.md) reports passing generation and acceptance checks, including identical C for all five modules. That bootstrap run took about 15.7 minutes; this is an observed build duration, not a performance guarantee. A successful local run writes its hashes and results to `build/selfhost/receipt.json`.

The delivered language remains the seed subset: bracketed sends, immutable bindings, signed Integer literals, String and Symbol literals, Vectors, lexical Blocks, comments, and message-based control. Full language coverage, fibers, image persistence, identity-preserving reload, and optimizing compilation remain unfinished. CBPV remains an experiment.

Self-hosting currently covers the authored parser/compiler toolchain. The Odin interpreter is still needed to bootstrap a source-only checkout; the native generations retain Odin runtime services and Clang. No old Rust implementation, installed Birnam, or checked-in generated C participates. See the [bootstrap guide](docs/selfhosting.md) for the detailed boundary.

## Documentation

- [Architecture](ARCHITECTURE.md) is the consolidated record of the agreed requirements and directions.
- [Native siphon](docs/siphon.md) describes the delivered first slice; [validation](docs/siphon-validation.md) records its evidence.
- [Loadable seed interpreter](docs/seed-interpreter.md) documents source execution and the supported subset; [validation](docs/seed-interpreter-validation.md) records the checks.
- [Authored PEG](docs/peg.md) documents grammar construction, matching, replacement, and limitations; [validation](docs/peg-validation.md) records acceptance.
- [Authored Lang](docs/lang.md) covers the grammar, CST translation, and parser takeover; [validation](docs/lang-validation.md) records acceptance.
- [Authored C compiler](docs/compiler.md) documents native module generation and the reference IR; [validation](docs/compiler-validation.md) records acceptance.
- [Native bootstrap](docs/selfhosting.md) describes the self-hosting stages, guarded acceptance, and remaining runtime dependencies; [validation](docs/selfhosting-validation.md) records evidence.
- [First milestone](docs/first-milestone.md) records the seed-language scope and acceptance requirements: an Odin interpreter running Birnam-authored PEG.
- [Seed invocation contract and ABI proposal](docs/seed-invocation.md) records the selected receiver-aware lookup design and separates the still-proposed native calling convention and recursion details.
- [Dispatch equivalence analysis](docs/dispatch-equivalence.md) examines whether replaceable lookup can replace the proposed extra send/bind layers, with explicit assumptions and a counterexample.
- [Lookup, binding, and sending review](docs/dispatch-design-review.md) is a standalone treatment for external language-design review, comparing plain Id, receiver-aware lookup, and independent bind/send hooks.
- [CBPV compiler core experiment](docs/cbpv-experiment.md) records a direction to try, with invariant constraints, a bounded trial, and adoption/rejection criteria. CBPV is not an adopted architectural requirement.
- [Review notes](Birnam%20Runtime%20Architecture%20%E2%80%94%20Review%20Notes.md) preserve the discussion and decisions.
- [Collector research](Collector%20Research.md) and [image-loading prior art](Image%20Loading%20Prior%20Art.md) distinguish external evidence from implementation proposals.

The earlier [architecture record](Birnam%20Runtime%20Architecture%282%29.md) is retained as discussion history. Use the consolidated architecture for current requirements. Historical sources in `/Users/chazu/dev/zig/birnam` inform syntax and public behavior; they are not build dependencies or an implementation plan. The reboot must not use the previous Rust implementation, compiler, or Lang seed.
