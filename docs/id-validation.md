# Authored Id validation

Recorded 7 November 2026 on macOS 27.2 ARM64 with the pinned toolchain in [toolchain.json](../toolchain.json). The final source/build-input SHA-256 is `bb4f7c583e7a048a237a14d14109685f750ef52531d791510d8e4d6a3c108818`. It is SHA-256 over sorted relative paths, a NUL, file bytes, and another NUL for `Makefile`, `toolchain.json`, and every non-`__pycache__` file under `src`, `include`, `tests`, `examples`, `modules`, and `tools`. Documentation is excluded.

## Regression gate

`make verify` passed in 4m46s; the run before this change took 2m45s. The added time is mostly [id_test.py](../tests/id_test.py), which compiles the module and two corpora with the interpreted compiler, plus the toolchain now loading Id first. All earlier siphon, seed, PEG, Lang, and compiler acceptance passed unchanged on the new siphon.

The Id test reported:

- Source takeover, loaded twice: the Id corpus, seed semantics and collections, and PEG semantics and replacement passed. The 9 replaced siphon Methods were captured before takeover, confirmed unselected and disabled after it, and never entered.
- Captured pre-takeover siphon Methods still ran their own implementation: the native `define:as:` added a layer, and the native vtable `clone` copied entries.
- The C contract kept 1000 definitions on one object in one layer and finalized 4095 of 4096 discarded specialized objects; the heap stayed at 1916928 bytes.
- The natively compiled module, Id corpus, and compiler corpus passed under the compiler guard, with the native parser and seed evaluation disabled. The compiler contract finalized 200 of 200 closures and reported no interpreter fallback.
- The authored compiler produced byte-identical C for the compiler semantics corpus with and without Id loaded.
- Loading the native module without the seed failed with `unknownName`.

## Native bootstrap

`make verify-selfhost` passed in 26m04s, with Id as the first of six modules. Stage 1, Stage 2, and stage3-c C matched byte for byte for every module, and the Stage 2 compiler built and ran the `42` example. The guarded acceptance corpora, five rejected inputs, missing-module failure, and the public CLI all passed. Both native generations and the acceptance runs loaded `id.dylib` before the rest of the toolchain. Stage 1 `id.c` has SHA-256 `3e5c6c580a9bca1794fe7c6b5320c7f84e4d5ee9bd45a385f6bb4bb3a84403d8`.

Observed per-module times, including emission and Clang, compared with the [previous receipt](selfhosting-validation.md):

| Module | Stage 1 seconds | Previous | Stage 2 seconds | Previous |
|---|---:|---:|---:|---:|
| id | 30.009 | — | 78.550 | — |
| peg | 64.619 | 52.845 | 173.710 | 142.311 |
| lang | 23.298 | 19.434 | 64.636 | 54.081 |
| lang-syntax | 33.713 | 27.993 | 96.945 | 82.277 |
| compiler-ir | 17.076 | 14.097 | 46.124 | 37.701 |
| compiler-c | 41.208 | 35.613 | 105.754 | 90.978 |

The existing modules took 16–23% longer in each stage, and the pipeline took 1357s against the previous 15.7 minutes. The previous run partly overlapped the regression gate; this one ran alone. Neither is a controlled comparison. The likely cost is authored policy on paths the toolchain uses heavily. Every generated code node is created with the authored Namespace `new` and specialized with the authored `define:as:`, and IR and CST nodes are copied with the authored Namespace `clone`. Profiling would be needed before attributing or optimizing it.

## Boundary

This qualifies the authored definition, cloning, and reflection policy and its takeover on this Mac. It does not make default lookup authored, add slot/accessor factories, compact pre-takeover layer chains, provide thread-safe publication or unloading, or qualify another platform. Generated artifacts and receipts remain local build products.
