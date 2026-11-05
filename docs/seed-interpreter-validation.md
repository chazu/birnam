# Seed interpreter validation

5 November 2026. Scope: the separately loaded Odin seed interpreter, its initial source subset, and integration with the existing native siphon. Authored PEG and the complete first milestone remain future work.

## Inputs and commands

The pinned environment is unchanged from the native siphon baseline: macOS 27.2 ARM64, Odin `dev-2026-10:84bc3fc21`, Apple Clang 16.0.0 (`clang-1600.0.26.3`), SDK 15.0, and Boehm 8.2.12. Exact library/SDK checksums are enforced by [toolchain.json](../toolchain.json).

The checked source/build-input digest is `17ea09cfef74d026dea13114f6d38c3ff567f6dda8e63c18d765c84e70b098b4`. It is SHA-256 over sorted relative paths, a NUL, file bytes, and another NUL for `Makefile`, `toolchain.json`, `tools/build.py`, and every non-`__pycache__` file under `src`, `include`, `tests`, and `examples`. Documentation is excluded.

`make verify` passed with Odin vetting for both host and module, and C fixtures compiled with warnings-as-errors. The standalone command below printed exactly `42`:

```sh
./build/siphon ./build/seed.dylib --eval examples/seed/answer.birnam
```

## Observed results

- The original native contract, compatible handover, module failure, and different-working-directory checks still pass. Its native GC workload finalized 4,096 of 4,096 registered unreachable objects.
- The source semantic corpus passed lexical captures after return/GC, defining-namespace behavior, global rebinding, local overrides, captured Method identity/entry, inspection of interpreted Method code/captures, and absence of implicit application lookup.
- Receiver/argument effects occurred once in the expected order, and replacement during argument evaluation affected final target selection. Vector elements preserved order and fresh/persistent collection behavior.
- Replacing arithmetic, Boolean conditional, Block invocation, literal construction, parser, and syntax evaluation behavior affected execution. The direct evaluator did not bypass those message boundaries.
- Error values were retained/recovered explicitly; a discarded runtime Error stopped later expressions and retained earlier effects. Twenty-six negative source cases passed, including malformed grouping, arity errors, range/overflow, unsupported syntax, missing names, type/index failures, and invalid UTF-8 with byte-position diagnostics.
- Multi-file evaluation preserved the namespace and late global lookup. Missing seed/source inputs and incomplete `--eval` usage failed explicitly.
- The C witnesses verified effectful lookup on repeated source execution, effects before failed evaluation, and correct results while GC ran during interning of partially built syntax.
- A bounded workload created and discarded 600 returned lexical closures; **600 of 600** were finalized, within the 16 MiB heap-growth allowance. The allowance accounts for conservative retention; this is evidence for the tested workload, not a universal retention guarantee.

The complete gate also passed in a fresh temporary source directory containing only the implementation/test/example directories, `Makefile`, and `toolchain.json`, with no prior build products or Git metadata. That build used the declared installed native dependencies, without the old Rust tree, Cargo, or an installed Birnam. The local generated log is `build/seed-clean-verification.log`.

## Limits

This receipt does not qualify a native Birnam compiler, full language compatibility, authored PEG, source module dependencies, image persistence, fibers, concurrent publication, incremental GC, Alien callback/thread handling, or CBPV transformations. The native module remains mapped until process exit and is not an implementation of rebuilt-code hot reload. Runtime stack depth and long-running workloads beyond the bounded tests are not characterized. See [the implementation note](seed-interpreter.md) for the exact source/API surface and small seed policies.
