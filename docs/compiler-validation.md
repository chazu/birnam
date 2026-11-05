# Compiler validation

5 November 2026. Scope: the first authored IR/reference evaluator and C backend for the seed subset, producing loadable native modules. This is not self-hosting, a CBPV trial, an optimization result, or a formal preservation theorem. See [compiler implementation](compiler.md) for its protocols and explicit limits.

## Build evidence

The pinned environment is unchanged: macOS 27.2 ARM64, Odin `dev-2026-10:84bc3fc21`, Apple Clang 16.0.0 (`clang-1600.0.26.3`), SDK 15.0, and Boehm 8.2.12. The build verifies versions and library/SDK digests from [toolchain.json](../toolchain.json). Odin vetting and the C warnings-as-errors gate remain enabled; generated modules compile as C11 with `-O2 -g` through the same SDK and collector configuration.

The complete `make verify` passed in **158.7 seconds** in a fresh source directory, `/private/tmp/birnam-compiler-clean-e0fswvgc`. It contained only source/build inputs, without Git metadata or previous build products. That gate covered native siphon, seed, PEG, Lang takeover, and compiler acceptance. The local generated log is `build/compiler-clean-verification.log`.

After that gate, two C test fixtures were strengthened to wrap the current Integer, Block, Vector, and Environment methods and collect during their callbacks. Those two fixtures were copied into the same fresh directory and rebuilt; the complete compiler suite passed again there. No production code changed between these runs. Its log is `build/compiler-services-verification.log`.

The final checked source/build-input digest is `f06943da140f6025f3ce42723343073382d9f4dc012407982fa35c49dba548a0`. This is SHA-256 over sorted relative paths, a NUL, file bytes, and another NUL for `Makefile`, `toolchain.json`, and all non-`__pycache__` files under `src`, `include`, `tests`, `examples`, `modules`, and `tools`. Unlike earlier receipts, the complete tools directory is included because it now contains the compiler driver. Documentation is excluded. The original full-gate digest, before the two harness additions, was `5ecad6c6bcddb9e8ad7705b9d3fff47d040bf3bc519b78d86e852cde2fa5cdf0`.

Build inputs and commands used the declared installed native dependencies, without the old checkout, Cargo/Rust artifacts, or an installed Birnam. The driver passed source as data to authored Lang/Compiler, captured its C text, then invoked Clang. The driver did not generate program logic.

## Semantic witnesses

`tests/compiler_test.py` ran from an unrelated working directory and exercised three paths: direct seed evaluation, the authored IR evaluator, and generated native code. The shared corpus supplied independent expected results and identity/effect assertions, rather than relying only on matching output.

- Receiver and argument effects occurred once in order. An argument replaced the pending Method, and the selected replacement produced the expected `RAM` trace.
- Captured Methods retained their selected implementation; later sends saw replacement. Inherited Methods received the original receiver, and local overrides left the parent unchanged.
- Captured locals survived return and collection, globals resolved late in the defining namespace, source Blocks and Vectors retained required identity distinctions, and returned Blocks remained deferred until explicitly invoked.
- Receiver-local arithmetic replacement remained visible. Signed-minimum literals, checked overflow, byte-oriented strings, persistent Vectors, empty bodies, and retained code source passed.
- Error values could be stored and explicitly recovered. An Error receiver still evaluated its argument; a discarded Error stopped its enclosing body and preserved preceding effects. Duplicate local bindings and arity mismatches reported their kinds.
- A separate effectful receiver-aware lookup fixture observed exactly two lookups and two entries in all three execution paths. The selected Method's `valueOn:with:`, `apply`, and `invoke` were poisoned, ruling out an implicit second invocation lookup.
- Replacing IR emission and the emitter's quoting Method affected subsequent compiler behavior. An unsupported literal factory returned `unsupportedLiteralFactory` rather than an invented serialization.
- Generated C preserved UTF-8, NUL, quote/backslash bytes, and Symbol spelling. A native top-level failure printed only its preceding output and returned `zeroDivide`. Empty source produced a valid empty module.
- Malformed source, deferred float syntax, Integer range overflow, and duplicate Block parameters failed compilation without writing a new output file.

## Native execution and malleability

The native acceptance host loaded only `seed.dylib`, the test guard, and the generated module/fixtures. It did not load PEG, Lang, or Compiler for execution.

`tests/compiler_guard.c` replaced the native parser Method entry and all seven seed syntax-family `evalIn:` entries with Error sentinels. Each sentinel was directly exercised to prove it worked, then its counters were reset. The compiled corpus completed with zero sentinel calls. Block/environment/value primitives remained available as the declared runtime services; none of these tests claims their removal.

The strengthened guard replaced the current Integer literal constructor, Block constructor, Vector constructor, and Environment lookup Methods with observing native wrappers that delegate to their captured originals. All four were called by compiled execution. Every seventeenth call collected, testing live intermediate references during ordinary runtime callbacks.

The native contract replaced the root code object's `evalIn:` and observed the replacement through the program's `runIn:`. Direct entry of a captured original Method still executed the native body. This distinguishes live sends from retained entry; it does not freeze the code object's mutable links or imply cross-artifact migration.

## Collection

The native lifecycle witness created and discarded **200 closures**, all of which were finalized in the final strengthened run. A retained compiled-body closure still returned its captured marker. Heap size stayed at **1,064,960 bytes** over the bounded workload. The test allows at least 180 finalizations and up to 16 MiB growth for conservative retention; these observations are not universal retention guarantees.

The existing full-gate GC results also passed: 4,096 native objects, 600 seed closures, 401 PEG contexts/400 trees/400 grammars, and 100 Lang syntax roots/100 closures were finalized in their respective workloads.

## Boundaries

The native output uses per-node message dispatch and GC-managed argument carriers, with no optimization or native-speed claim. The IR reference evaluator uses authored bookkeeping that itself depends on mutable language services; agreement is bounded to the documented witnesses, not arbitrary mutations of that infrastructure. Independent changes to `evalIn:` and `emitIn:` need their own consistency evidence.

Native artifacts reconstruct their own code objects and link named standard factories/services in the load namespace. They do not save arbitrary compiler-process objects, migrate identities, unload code, or persist active execution. Compiling and rebuilding PEG, Lang, and Compiler into a self-hosted toolchain remains the next bootstrap milestone. Broader performance, images, fibers, concurrency, Alien, and CBPV/effect experiments remain separate work.
