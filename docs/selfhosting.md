# Native compiler bootstrap

The native bootstrap pipeline builds PEG, Lang, Lang's syntax translator, the compiler IR, and the C emitter from their Birnam sources. It then uses those native modules to rebuild the same five modules and checks that the generated C is stable. It retains the Odin siphon and seed runtime services; self-hosting here concerns the authored parser/compiler toolchain, not removal of the runtime or native C toolchain.

```sh
make verify-selfhost
```

Each target rebuilds the native bootstrap floor first. `selfhost` builds the generations, checks C stability, and runs the compiled `42` example. `verify-selfhost` performs that pipeline plus the native semantic/grammar/replacement and error tests. They are separate from the shorter `make verify` regression gate.

The generated artifacts and receipt live under `build/selfhost/`. To compile another program with the second native generation:

```sh
python3 tools/compile.py examples/compiler/answer.birnam \
  --native-dir build/selfhost/stage2 -o build/native-answer.dylib
./build/siphon ./build/seed.dylib ./build/native-answer.dylib
```

The result is `42`. `--native-dir` is explicit: missing libraries fail instead of loading authored sources as a fallback. Without it, `compile.py` retains the interpreted bootstrap path. `--emit-c` and `--name` work with either path.

## Stages and evidence

1. The seed interpreter loads authored PEG, Lang, the translator, and compiler. That compiler emits C for the five module sources, and Clang builds **Stage 1** libraries.
2. A new host loads the seed's runtime services and those five native libraries. The native compiler emits the same five modules, and Clang builds **Stage 2**. Each C file must match its Stage 1 counterpart byte for byte before compilation continues.
3. Another host loads Stage 2 and emits all five modules again into **stage3-c**. These files must match Stage 2 exactly. A third set of identical-C libraries is unnecessary; Stage 2's compiler is exercised by this emission and by compiling/running a separate application.

The comparisons establish a generated-C fixed point for these sources, toolchain, and runtime. They are not a formal proof of compiler correctness or a binary reproducibility claim; debug paths differ between library generations. The semantic tests provide separate observations of behavior.

Stage 2 and stage3-c run with a test guard installed before any authored native library loads. It disables the seed's `parse:`, `eval:`, `eval:in:`, and `evalFile:` entries and all seven seed syntax-family `evalIn:` entries. The guard directly probes all eleven sentinels, resets their counters, and checks for zero calls when loaded again after successful compilation or execution. The general Oop primitives, environments, Block/Method invocation, conservative collector, loader, and I/O remain available.

`tests/selfhost_test.py` compiles the compiler semantic corpus, PEG semantic/replacement corpora, Lang grammar corpus, and compiler-emission replacement corpus with Stage 2, then runs the resulting native tests under the guard. It also checks rejected syntax, integer range, duplicate parameters, invalid UTF-8, missing-library failures, and the public native CLI. `--existing` can reuse a completed build only after checking its module sources, generated C, library hashes, host/seed/adapter hashes, and pinned toolchain against the receipt.

## Driver boundary

[compile_request.c](../src/tools/compile_request.c) is a small native CLI adapter built as `build/compile-request.dylib`. It reads raw stdin bytes, obtains the export name from `BIRNAM_COMPILE_NAME`, sends `Compiler compile:as:`, and sends the resulting text to `System print:`. It contains no grammar, lowering, IR, or C-emission logic. Unlike the bootstrap path's source wrapper, it needs no interpreted request script and preserves arbitrary input bytes for Lang's own UTF-8 diagnostics.

[compile.py](../tools/compile.py) chooses the interpreted or native module-loading path and invokes Clang. [selfhost.py](../tools/selfhost.py) orchestrates the stages and comparisons. Parsing, translation, and C emission remain Birnam-authored, using ordinary Id messages and the existing C-compatible Oop boundary.

The pipeline rewrites its receipt to `building` before starting. A failure cannot leave an old `passed` receipt at that location. A successful receipt records input and artifact hashes, observed toolchain, per-module stage times, C equality, guard use, and example output. Qualification adds its acceptance results. These local generated artifacts and logs are not checked-in bootstrap seeds.

## Limits

This uses the pinned Mac toolchain and the existing ABI. It does not add runtime installation, source dependency resolution, identity-preserving image/module migration, library unloading, fibers, Alien, optimized dispatch, or a native-speed claim. The C backend still emits native Methods for individual code objects with message-based child execution. Build timings describe the observed workload, not a controlled performance comparison. CBPV remains a separate proposed experiment.

The Odin interpreter remains necessary to start from a source-only checkout. The native stages do not use its parsing/evaluation entries, but they still use its general runtime services and native Block/Method frame machinery. No old Rust implementation, installed Birnam, or checked-in generated C participates. See [the validation receipt](selfhosting-validation.md) for the actual results and their boundaries.
