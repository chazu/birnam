# First authored C compiler

Implemented first slice, 5 November 2026. This slice builds on authored Lang/PEG and covers the existing seed forms. It is a first native backend. The subsequent [native bootstrap](selfhosting.md) qualifies rebuilding the authored toolchain; this remains an unoptimized compiler. CBPV remains the separate experiment described in `cbpv-experiment.md`; this slice does not adopt that IR or claim a CBPV preservation result.

The compiler uses an authored IR whose syntax families are ordinary objects with `evalIn:` and `emitIn:` behavior. Lang's translator constructs these through an injectable syntax-owner protocol. A reference IR evaluator is compared against the existing seed evaluator before native execution is qualified. Errors remain values; only discarded non-final Errors stop a sequence. Sends evaluate receiver and arguments once in order before binding and directly entering the selected Method.

Generated C supplies native `evalIn:` Methods for individual code objects. Child execution sends `evalIn:` through current lookup; primitive operations, name resolution, constructors, and source sends likewise retain their message boundaries. This deliberately conservative backend has per-node dispatch and no inlining, selector cache, native payload decoding, or performance claim. It executes generated native Methods rather than asking Seed to parse/evaluate embedded source.

The artifact reconstructs code objects with inspectable source and child references, resolves standard literal factories and runtime services in the load namespace, publishes a named program, then executes its top level. The program offers `code`, `source`, and `runIn:`. It still needs the general data/environment/Block services, which are in [runtime.dylib](runtime.md) since 9 November 2026. It does not need PEG, Lang, the compiler, or the interpreter at execution time. This is native module packaging, not an image of arbitrary compile-time state or identity-preserving module reload.

Qualification covers source/IR/native agreement, independent expected results and traces, late lookup after argument mutation, Errors/recovery, source Block identity and capture survival, Method retention, replacement of code/runtime behavior, GC, and explicit rejection of unsupported compiler inputs. Native runs must rule out interpreter fallback; since 9 November 2026 they load no interpreter, and a guard fails the run if `Seed` is bound. The build driver may orchestrate source loading, C output, and Clang, but compilation policy and C emission must reside in Birnam.

## Usage and APIs

```sh
make build
python3 tools/compile.py examples/compiler/answer.birnam -o build/answer.dylib
./build/siphon ./build/runtime.dylib ./build/answer.dylib
```

The example prints `42`. `--emit-c` writes C without invoking Clang; `--name Example` chooses the program's namespace export (default `NativeProgram`). The Python driver loads the [authored Id](id.md), PEG, Lang, the translator, and the two compiler modules, installs authored parsing, passes the input as String data, and captures the compiler's C output. It never translates source or emits program logic itself. Clang uses the existing pinned local configuration. Source compilation Errors cause a failing exit and no new C output.

- [compiler-ir.birnam](../modules/compiler-ir.birnam) publishes `Compiler`. `lower:` answers the IR, `evaluate:in:` runs the reference IR evaluator, `ir` exposes the construction service/prototypes, and `frontend` exposes its Lang instance.
- [compiler-c.birnam](../modules/compiler-c.birnam) adds `compile:as:`, `emit:as:`, and `emitter`. The first parses/lowers/emits; the second accepts an IR root and export name. Both answer C text or an Error.
- Lang's visitor now sends `syntax` to obtain its construction service. Its default still constructs seed syntax. The compiler clones that visitor and supplies the IR construction service, reusing Lang's grammar, escape decoding, range checks, and duplicate-parameter checks.
- `Reflect send:to:with:` is a general dynamic-send primitive added for authored evaluators. It takes a selector, receiver, and Oop argument Vector, then uses Id selection/direct entry. It does not send an implicit `valueOn:with:`, `apply`, or `invoke` message to the selected Method.

The IR represents Literal, Name, Declaration, Send, Block, Vector, and Sequence objects. Their own `evalIn:` and `emitIn:` Methods supply behavior. There is no native IR variant switch. IR construction, reference evaluation, and emission remain replaceable independently. Changing an IR evaluator does not automatically rewrite its separate emitter or an already loaded native artifact.

## Native representation and execution

Generated code objects are Namespace-backed objects with native `evalIn:` Methods. Each entry performs the operations for that particular source node. The backend emits distinct C statements for receiver and successive operands, avoiding C's unspecified function-argument evaluation order. A source send then calls `birnam_bind` followed immediately by `birnam_enter` on that actual selection. Vector transport storage is GC-managed, including when custom behavior retains it.

A declaration stores an Error value normally. A sequence tests `isError` only when discarding a non-final result. Blocks are constructed through the captured Block factory's current `code:environment:parameters:` Method; their bodies are generated native code objects. The existing Block/environment services supply lexical frames and capture lifetime. A Method made from such a Block retains the native code object and its defining environment.

The program exposes `code`, `source`, and `runIn:`. Sequence code objects expose `source`, and all bodies share the original source Bytes. Internal links use named slots: `c0`, `c1`, etc. for ordered children, and `receiver`, `expression`, `body`, `parameters`, `selector`, `name`, `factory`, and `bytes` where applicable. The number and meaning of links are part of each generated entry's representation. Editing a child or replacing its `evalIn:` is observed by a later send; changing the number of operations requires a replacement implementation. These are compiled code objects, not the seed's syntax payload layout.

Each native Method owns a small capture payload containing Oop references to canonical Booleans/nil and the load namespace's Block, Vector, and Environment services. Its C implementation alone interprets that capture. There are no Odin-specific fields or generic payload inspection. Service Methods remain live; rebinding an unrelated namespace slot does not retarget an already captured service object.

Integer/String/Symbol literal factories are linked by name at module initialization. Unknown literal factory objects answer `unsupportedLiteralFactory` at emission. This is a contextual native module, not serialization of arbitrary compiler-process objects. The program is published before top-level execution, so an initialization Error preserves completed publication/effects, matching the loader's existing policy. Native libraries remain mapped under that loader's current lifetime policy.

C string constants use fixed-width octal escapes and explicit byte lengths, preserving UTF-8, embedded NUL, quotes, and backslashes without embedding source text as C syntax.

## Acceptance and limits

[Compiler validation](compiler-validation.md) records the complete gate. The shared direct/IR/native corpus checks returned values, alias relationships, captured locals versus late globals, effects before method selection, explicit Block execution, constructor/literal behavior, Error propagation/recovery, and code-source retention. Separate witnesses check effectful receiver-aware lookup and poison implicit Method invocation messages. The native acceptance process loads neither Lang nor PEG nor `seed.dylib`. Its guard fails to load, with `interpreterPresent`, if `Seed` is bound; it replaced the earlier guard that disabled native parsing and all seed syntax-family evaluation entries. It also wraps current constructors and Environment lookup, collecting during their callbacks to verify those boundaries remain live.

The reference IR evaluator is itself ordinary authored code. Mutating global infrastructure such as Integer arithmetic, canonical Boolean control, or Vector construction can change its own bookkeeping, just as such edits can change the authored parser/compiler. The tested agreement is for the documented witnesses, including receiver-local arithmetic replacement and source operation/Method replacement. It is not a formal equivalence claim under arbitrary mutation of the evaluator, compiler, or runtime services.

No optimization, deoptimization, packrat parser, CBPV adoption, effect handler, new fiber behavior, image format, or code unloading is introduced. Per-node message dispatch and many object allocations remain. The subsequent [native bootstrap qualification](selfhosting.md) rebuilds PEG, Lang, and the compiler with native generations. Independent performance and broader malleability evidence are still required.
