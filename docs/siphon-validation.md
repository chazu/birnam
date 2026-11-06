# Native siphon validation

5 November 2026. Historical receipt for native siphon commit `e13b252`: the first single-threaded native siphon and C module ABI. The design checkpoint preceding implementation is `74ebd20`. Later source-module changes are qualified separately in [seed interpreter validation](seed-interpreter-validation.md).

## Build inputs

| Input | Observed value |
|---|---|
| Host | macOS 27.2, ARM64 |
| Odin | `dev-2026-10:84bc3fc21` |
| C compiler | Apple Clang 16.0.0, `clang-1600.0.26.3` |
| SDK | macOS 15.0 from Xcode |
| Collector | Boehm 8.2.12, runtime version `0008020c` |
| GC library SHA-256 | `357ca358cf58b0e23bcbea6389d53030ce62209ca3236c081e8c63af64da3a84` |
| SDK settings SHA-256 | `1f8d9531b992145bd1338075321a2c46e22ac83cb6d25a5cb9cf9e1efd9b9d5b` |

The checked implementation/build-input digest is `c40e7f64716076d445b36a984415ba31b66e61769335db439d0b67d0bb8f4fe3`. It is SHA-256 over sorted relative file paths, a NUL, file bytes, and another NUL for `Makefile`, `toolchain.json`, `tools/build.py`, and all files under `src`, `include`, `tests`, and `examples`. Documentation is excluded.

## Results

`make verify` passed with Odin `-o:speed -debug -vet` and C fixtures compiled with `-O2 -Wall -Wextra -Werror` (unused entry parameters exempted). The SDK is explicit; the first attempt using the ambient Command Line Tools SDK failed to link, and the build now checks the matching Xcode SDK settings.

The C contract fixture passed receiver-sensitive binding, direct entry without implicit application, local method overrides, parent updates, shallow object cloning, namespace copy independence, symbol protocol replacement, root lookup replacement, and entered/captured Method retention. It also passed kinded failure and Error-propagation cases, including an inherited Object method that must not consume an Error, invalid argument-carrier extent, wrong arity, and null receiver rejection.

The GC workload finalized **4,096 of 4,096** unreachable registered objects across four bounded batches, within its 8 MiB heap-growth allowance. Live arguments, object slots, and captured Methods survived explicit collection inside native calls and after replacement. Finalization counts are reclamation evidence for this workload; they are not a universal guarantee about conservative retention.

Three separately compiled C modules demonstrated compatible handover in one world: the first created live objects and captured a Method; the second replaced root lookup and shared behavior; the third confirmed the same namespace identity, new behavior on the existing object, and continued execution of captured old code after collection.

An initializer that published native code and then failed returned `unknownName`; its already-published Method remained callable. Missing libraries and missing entrypoints produced `moduleOpenFailed` and `moduleEntryMissing` with nonzero CLI exit status. A native hello module ran successfully from `/private/tmp` using absolute executable/module paths.

A second verification copied only `src`, `include`, `tests`, `tools`, `examples`, `Makefile`, and `toolchain.json` into a fresh temporary directory and ran `make verify` there. It passed without previous build products or Git metadata, using only the declared installed native toolchain/dependencies. The run's log is available locally at `build/clean-source-verification.log`; it is generated evidence, not a required input. No Rust checkout, Cargo command, installed Birnam, or old Birnam artifact was used.

## Limits

This is not qualification of source evaluation, authored PEG, fibers, multicore publication, incremental GC, `GC_do_blocking`, arbitrary foreign stacks, image persistence, or an optimized calling ABI. The collector is exposed as a native bootstrap service with `collect`; a separately loadable collector and allocation-policy handover remain future work. The fuller-Id takeover is demonstrated by a C fixture, not a production authored module. Native DSOs remain mapped until exit.

The exact APIs and delivery boundary are in [the siphon implementation note](siphon.md). No native-speed or production-release claim is made by these results.
