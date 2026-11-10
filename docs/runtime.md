# Runtime services

Implemented 9 November 2026. `build/runtime.dylib` is an Odin native module that supplies the runtime services every Birnam program uses: Integer, String, Symbol, Vector and Bytes behaviour, Cell, Environment, Block, interpreted-Method frames, Reflect, System, and the Object, nil, Boolean, and Error protocol. Until this date these services lived in `seed.dylib` together with the bootstrap parser and evaluator. `seed.dylib` now holds only the interpreter.

The split separates two things that were bundled. Natively compiled programs and the native toolchain need the services but not the interpreter. Before the split they had to load the whole seed library, and a test guard disabled the interpreter's entries to show they were not used. Now they load `runtime.dylib` alone, so no interpreter code is loaded in the process, and the absence of `Seed` replaces the disabled entries as the evidence.

## Load order

Source runs load the runtime, then the interpreter, then the sources:

```sh
./build/siphon ./build/runtime.dylib ./build/seed.dylib --eval examples/seed/answer.birnam
```

Compiled programs load the runtime and then their own libraries:

```sh
python3 tools/compile.py examples/compiler/answer.birnam -o build/answer.dylib
./build/siphon ./build/runtime.dylib ./build/answer.dylib
```

Both print `42`. Without `--native-dir`, [compile.py](../tools/compile.py) still uses the interpreter to compile; only the compiled program runs without it. With `--native-dir`, it also compiles without the interpreter. It runs the siphon on `runtime.dylib`, the six native toolchain modules, and the request adapter, and passes the source on stdin:

```sh
./build/siphon ./build/runtime.dylib \
  build/selfhost/stage2/id.dylib build/selfhost/stage2/peg.dylib \
  build/selfhost/stage2/lang.dylib build/selfhost/stage2/lang-syntax.dylib \
  build/selfhost/stage2/compiler-ir.dylib build/selfhost/stage2/compiler-c.dylib \
  ./build/compile-request.dylib < examples/compiler/answer.birnam > build/answer.c
```

The adapter takes the export name from `BIRNAM_COMPILE_NAME` and uses `NativeProgram` if it is unset. The siphon processes arguments left to right in one world and has no dependency resolution, so the order on the command line is the load order. `siphon --help` states the source order.

## Published names

The runtime's init attaches to the world, installs its behaviours, publishes eight names, and returns the world.

| Name | Provides |
|---|---|
| `Integer` | Boxed signed 64-bit Integers: `fromBytes:`, checked `+ - * / %`, comparisons, `printString` |
| `String` | UTF-8 text: `fromBytes:`, `asBytes`, `intern`, `,`, `=`, byte operations, `utf8ErrorOffset` |
| `Symbol` | A factory whose `fromBytes:` interns through the siphon |
| `Block` | The prototype Block: `code:environment:parameters:`, `value` through four arguments, `valueWithArguments:`, `whileTrue:`, `code`, `parameters`, `environment` |
| `Environment` | Lexical environments: `forNamespace:`, `lookup:`, `define:as:`, `namespace`, `parent`, `names` |
| `Reflect` | `methodFrom:`, `methodFor:on:`, `send:to:with:`, `defineMethod:on:as:` |
| `System` | `print:`, `println:` |
| `Cell` | `with:`, `value`, `value:` |

It also installs Methods on objects the siphon already created: Vector construction and access, Bytes conversion and byte operations, Symbol `printString`, Method `valueOn:with:`, the Object defaults, nil, the two Booleans, and the Error recovery messages. The surfaces are listed in the [seed interpreter](seed-interpreter.md#native-primitives-exposed-through-messages) table, which predates the split. A Method made from a Block by `Reflect` gets the runtime's interpreted-Method behaviour. Its entry binds the receiver to the first parameter and sends `evalIn:` to the captured code in a fresh environment. Compiled code uses the same Block and Method machinery with generated code objects.

Loading `runtime.dylib` again is a no-op: init returns the world, every binding keeps its identity, and the selected Methods are unchanged.

## The interpreter module

`seed.dylib` publishes only `Seed`, with `parse:`, `eval:`, `eval:in:`, `evalFile:`, and `syntax`. It keeps the Odin parser, the seven syntax families with their `evalIn:` Methods, and the syntax-construction namespace that `Seed syntax` answers.

It does not link against `runtime.dylib`. At load it reads `Integer`, `String`, `Symbol`, `Block`, `Vector`, `Environment`, and `Object` from the world with `at:`. It then reaches them only through messages such as `fromBytes:`, `code:environment:parameters:`, `fromElements:`, and `forNamespace:`. It captures these objects once, as the seed did before the split. Replacing their Methods later changes interpretation; rebinding the names does not. Loading `seed.dylib` again answers the existing `Seed`.

The siphon's `--eval` option looks up `Seed` in the world when it reaches that argument and sends it `evalFile:`. The siphon contains no parser or evaluator.

Lang's default translator now sends `[Seed syntax]` each time it needs its syntax owner, rather than capturing it when [lang-syntax.birnam](../modules/lang-syntax.birnam) loads ([Lang](lang.md)). Lang can therefore load where `Seed` is unbound, as it does in the native toolchain.

## Shared helpers and state

Both libraries import [src/abi](../src/abi/abi.odin). It holds the Oop and header layout, the siphon's exported C functions, allocation, argument Vectors, Symbol interning, and the send, Error, publish, and install helpers. Odin compiles the package statically into each library. Each library therefore has its own copy of the package's state: the world, nil, true, false, the Error prototype, and an interned-Symbol cache. `attach` fills that state from the world the first time the library loads and registers it as a GC root. Each library also roots its own `state` struct. The `abi_aliases.odin` files in [src/runtime](../src/runtime) and [src/seed](../src/seed) only give the helpers short local names.

## The prototype Block's code

The prototype `Block` needs a code object. Before the split it was an empty seed Sequence, which answered `""` to `source` and `#[]` to `forms`. The runtime owns no syntax, so the code is now a private runtime object whose only Method is `evalIn:`, which answers nil. `[Block value]` is still nil. The new object does not respond to `source` or `forms`. Nothing in the tests, corpora, or modules reads them.

## Failure modes

- `siphon seed.dylib` without the runtime fails with `unknownName` (namespace has no such binding), because the interpreter's first service lookup finds nothing. The siphon exits 1.
- `--eval` with no `Seed` bound fails with `unknownName`: the lookup answers an Error, and `evalFile:` propagates it. The tests check this with neither library loaded.
- A natively compiled module loaded without the runtime fails when it resolves a missing service. The tests check this for the native Id module, which fails with `unknownName`.
- The interpreter-absence witness and the compiler guard fail with `interpreterPresent` when `Seed` is bound (next section).

## Evidence that the interpreter is absent

Native command lines do not list `seed.dylib`. In addition, [no_interpreter.c](../tests/no_interpreter.c) is a witness module that sends `at: #Seed` to the world each time it loads. If the answer is an `unknownName` Error, init returns the world and prints nothing. Otherwise it returns an Error of kind `interpreterPresent`, and the siphon exits 1. It keeps no state, so it can be loaded first and last in one run:

```sh
./build/siphon ./build/runtime.dylib ./build/no_interpreter.dylib \
  ./build/answer.dylib ./build/no_interpreter.dylib
```

`make verify` builds the witness. It is used as follows:

- `make verify` runs the compiled `answer.dylib` this way and expects `42`. It then checks that `runtime.dylib seed.dylib no_interpreter.dylib` exits 1 with `interpreterPresent`.
- [runtime_contract.c](../tests/runtime_contract.c), loaded after `runtime.dylib` alone, checks that the eight names are bound, that `Seed` is unbound, that Integer arithmetic works, and that the prototype Block's value is nil. It also loads `runtime.dylib` a second time and checks that nothing changed.
- [compiler_guard.c](../tests/compiler_guard.c) no longer disables Seed entries. Its init fails with `interpreterPresent` unless `Seed` is unbound. Its `untouched` message checks again each time it is sent. [compiler_test.py](../tests/compiler_test.py) and [id_test.py](../tests/id_test.py) run every natively compiled program under it on `runtime.dylib` alone. `compiler_test.py` also checks that the guard rejects a process with the interpreter loaded.
- [selfhost.py](../tools/selfhost.py) loads the witness before and after the native compiler for Stage 2, stage3-c, and the compiled application. [selfhost_test.py](../tests/selfhost_test.py) does the same for the acceptance corpora and failure cases, and checks the witness's failure path. The receipt fields are described in the [bootstrap guide](selfhosting.md#driver-boundary).

Results are recorded in [runtime validation](runtime-validation.md).

## Limits

The runtime services are still Odin. No service has been rewritten in Birnam, and the siphon still supplies dispatch, allocation, loading, Errors, and the collector interface. The interpreter is still required to build Stage 1 from a source-only checkout, and `compile.py` without `--native-dir` loads it. Nothing here is a performance claim, and timings recorded for the split are observations only.

The witness observes the `Seed` binding, not mapped code. A process that loaded `seed.dylib` and then removed the `Seed` binding could pass it. The stronger evidence is that the native command lines do not load the library. There is no module unloading: libraries stay mapped until exit, and a second load is a no-op, not a reload. Load order is not resolved automatically; a module loaded before the names it needs fails with `unknownName`. The interpreter captures its service objects at load, as before. The prototype Block's code no longer answers `source` or `forms`. The build remains pinned to the local Mac toolchain.
