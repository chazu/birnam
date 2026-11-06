# Native bootstrap validation

5 November 2026. This receipt qualifies native rebuilding of the five authored toolchain modules: PEG, Lang, Lang's syntax translator, compiler IR, and C emitter. It retains the Odin siphon/seed runtime services and the pinned C toolchain. See [native bootstrap](selfhosting.md) for commands and boundaries.

## Inputs and runs

The pinned environment remains macOS 27.2 ARM64, Odin `dev-2026-10:84bc3fc21`, Apple Clang 16.0.0 (`clang-1600.0.26.3`), SDK 15.0, and Boehm 8.2.12. `toolchain.json` versions and SDK/collector checksums were verified by the build tools.

The final source/build-input SHA-256 is `5e7529050fbe920001eb0627279044a5db8f779db875bdcc0d4793b7e2ad4e1d`. It covers sorted relative paths, a NUL, file bytes, and another NUL for `Makefile`, `toolchain.json`, and every non-`__pycache__` file under `src`, `include`, `tests`, `examples`, `modules`, and `tools`. Documentation and generated build products are excluded.

The native floor and new request adapter were rebuilt in the working checkout. `python3 tools/selfhost.py` then built both native generations from the five module sources, checked all three C emissions, and compiled/ran the separate answer example. The pipeline passed in **939.528 seconds**. It regenerated Stage 1 from the interpreted compiler; prior native authored libraries were not bootstrap inputs.

`python3 tests/selfhost_test.py --existing` validated that receipt's sources, C files, libraries, host, seed, adapter, and toolchain, then passed the current guarded acceptance tests. It rebuilt its guard from source. The generated receipt is `build/selfhost/receipt.json`, with both `status` and `acceptance.status` equal to `passed`. Local logs are `build/selfhost-progress.log` and `build/selfhost-acceptance.log`.

Separately, the complete existing `make verify` gate passed in **168.8 seconds** in `/private/tmp/birnam-bootstrap-clean-l3d7agbh`, containing only source/build inputs without Git metadata or prior build products. Its log is `build/selfhost-regression.log`. That fresh-directory run covered the native floor, seed, PEG, Lang, and first-compiler gates; the multi-generation native bootstrap and additional acceptance ran in the working checkout as described above. No Rust/Cargo artifact, old checkout, installed Birnam, or checked-in generated C supplied a bootstrap dependency.

## Generated-C fixed point

For every row, Stage 1 C (emitted by the interpreted compiler), Stage 2 C (emitted by the first native compiler), and stage3-c (emitted by the rebuilt native compiler) were byte-identical:

| Module | C bytes | SHA-256 for all three emissions |
|---|---:|---|
| peg | 1,026,604 | `ba3d5d22480b107077f4a6a9fd375d706a83db1a72d655834b233273e5db6d08` |
| lang | 426,743 | `ed5774597b242d789674839060f443c36e7517c95bf120f03725ffd887acb475` |
| lang-syntax | 574,373 | `bf97abb5c9dfb1c9c594d4e09340411209bdbe5bf10f56a5be4b6bbc763b86da` |
| compiler-ir | 307,960 | `ccb45b0cb29c8339990ef8e07ae52014a2032629c1cb6bcee2ad396049206328` |
| compiler-c | 630,529 | `ef955d97e907d06d822300000720eb4402e1be671c47ffa3e30866254f919319` |

All ten Stage 1/Stage 2 libraries compiled with the pinned C11 `-O2 -g` warnings-as-errors configuration. Stage 2 also compiled `examples/compiler/answer.birnam`; its native module printed exactly `42` with interpreter entries disabled.

This is output stability for the tested bootstrap, not a formal correctness proof or a claim that the library binaries are identical. Debug paths differ across stages; library hashes are recorded individually in the generated receipt.

## Guard and native acceptance

The guard was loaded before any authored native library. It replaced the seed's source parsing/evaluation entries and all seven syntax-family evaluation entries with failing sentinels, directly tested all eleven, and reset their counters. Reloading the guard after each successful native compilation/execution asserted zero calls. Native Block/Method frame machinery, environments, data primitives, Id dispatch, collection, loading, and I/O remained available.

The second native compiler compiled these source corpora into native libraries. They executed with native toolchain modules and the guard, producing their expected PASS outputs:

- Compiler semantics: captures/identity, code source, late globals, receiver/argument order, replacement before selection, captured Method entry, arithmetic replacement, Errors/recovery, and collection.
- PEG semantics: labelled CST spans, prefix/EOF, ordered choice, repetition/lookahead, structural errors, UTF-8 byte positions, and the deep finite rule chain.
- PEG replacement/reentrancy: current Methods, captured entry, rule-binding snapshots, nested parses, and independent diagnostic state.
- Lang grammar: accepted/rejected seed forms, labels, and byte spans.
- Compiler emission: replaceable IR emission/quoting and explicit unsupported-factory errors.

The native compiler rejected deferred float syntax, malformed sends, Integer range overflow, duplicate Block parameters, and invalid UTF-8 with the expected Error kinds and no new C output file. An absent native module directory failed with `moduleOpenFailed`, without source fallback. The public `compile.py --native-dir ... --emit-c` invocation reproduced the example C byte for byte from an unrelated working directory.

## Timing and remaining limits

Observed per-module times include emission and Clang, not just compiler execution:

| Module | Stage 1 seconds | Stage 2 seconds |
|---|---:|---:|
| peg | 52.845 | 142.311 |
| lang | 19.434 | 54.081 |
| lang-syntax | 27.993 | 82.277 |
| compiler-ir | 14.097 | 37.701 |
| compiler-c | 35.613 | 90.978 |

Stage 2 took longer in this run. Some work overlapped with the fresh regression gate, so this is not a controlled performance comparison. The current backend's per-node dispatch, Namespace-backed code links, and allocation costs remain; self-hosting is not evidence of native-speed execution. Performance work needs separate profiling and representative witnesses.

This establishes the authored toolchain's native bootstrap on this Mac. It does not eliminate the initial Odin interpreter or its runtime services, qualify another platform, adopt CBPV, introduce images/fibers/Alien, support identity-preserving module migration, or install a system-wide compiler. Generated artifacts and receipts remain local build products.
