# Lang validation

5 November 2026. This receipt qualifies the authored grammar, CST-to-syntax translation, and explicit parser takeover for the existing seed subset. The grammar slice was committed as `36e153c`; this receipt includes the subsequent takeover layer. See [Lang](lang.md) for the public protocol and remaining limits.

## Build and dependency evidence

The pinned environment remains macOS 27.2 ARM64, Odin `dev-2026-10:84bc3fc21`, Apple Clang 16.0.0 (`clang-1600.0.26.3`), SDK 15.0, and Boehm 8.2.12. The build verifies the versions and library/SDK checksums in [toolchain.json](../toolchain.json). Odin vetting and the existing C warnings-as-errors gate remain enabled.

The final source/build-input SHA-256 is `74c313011407630c10d6278cbb87ed75cb1643935739ceea1e63aa72c7b85042`. It covers sorted relative paths, a NUL, file bytes, and another NUL for `Makefile`, `toolchain.json`, `tools/build.py`, and every non-`__pycache__` file under `src`, `include`, `tests`, `examples`, and `modules`. Documentation is excluded.

The grammar slice passed `make verify` in the working checkout before its commit. The final complete `make verify` passed from a fresh source directory, `/private/tmp/birnam-lang-clean-gy6yocd3`, in 136.0 seconds. That directory contained only the listed source/build inputs, without Git metadata or previous build products. The generated local log is `build/lang-clean-verification.log`. The build used the declared installed native toolchain and collector; it did not consume the old checkout, Cargo/Rust artifacts, or an installed Birnam.

## Acceptance results

- Native dispatch/handover/loader tests and the original seed/PEG gates passed. The existing GC witnesses finalized 4,096 native objects, 600 seed closures, 401 PEG contexts, 400 PEG trees, and 400 PEG grammars.
- Lang's grammar accepted the supported seed forms and rejected malformed grouping, token boundaries, missing selectors/delimiters, invalid escapes, and deferred forms. CST assertions checked labels, source order, and exact byte spans. Both PEG's and Lang's complete source parsed through the authored grammar.
- `tests/seed_test.py --authored` passed the existing sample, semantic and collection corpora, all 26 failure cases, multi-file namespace behavior, source failure ordering, and diagnostic checks through the authored parser. Native-parser mode remains tested separately.
- Translation preserved inspected syntax fields and original source, Integer range behavior (including the signed minimum and leading zeros), duplicate Block parameter errors, and UTF-8 String escape decoding. An invalid UTF-8 input after a multibyte comment reported byte 7, line 2, byte column 2.
- Replacing the visitor's `identifier` Method changed subsequent translation. Replacing the syntax owner's `literal:bytes:` Method observed three construction sends and collected during each, without losing partially built code. Constructors rejected invalid argument, parameter, element, and form Vectors.
- A distinctive Integer literal's factory was not invoked while translating it; its code remained executable afterward. The authored parser's own normal Integer/String operations are still ordinary language behavior, not promised to be effect-free.

## No native-parser fallback

`tests/lang_guard.c` captures the original native `Seed parse:` Method and replaces that Method's entry with a sentinel returning `nativeParserForbidden`. It directly enters the Method once to verify the sentinel, resets its counter, then installs Lang through `installOn:`. The original entry remains disabled, including for callers retaining the original Method.

With that guard active, the harness re-evaluated `modules/peg.birnam`, `modules/lang.birnam`, and `modules/lang-syntax.birnam` through the authored parser, explicitly installed the newly loaded Lang, then passed the seed semantics, PEG semantics, and PEG replacement/reentrancy corpora. The final witness checked zero calls to the disabled entry. Source loading was explicit and ordered; this is not a claim of identity-preserving hot reload.

## Collection

`tests/lang_contract.c` created and discarded 100 parsed syntax roots and 100 returned lexical closures through authored parsing. The clean run finalized **100/100 syntax roots and 100/100 closures**. Retained syntax evaluated to `3`, a retained closure answered `42`, and the native-parser guard remained untouched. Heap size stayed at **6,144,000 bytes** across the bounded workload.

Assertions require at least 90 finalizations in each category and permit up to 16 MiB heap growth to account for conservative retention. This qualifies survival and reclamation for the exercised workload, not universal retention, arbitrary source depth, or long-running performance.

## Delivery boundary

Grammar and translation policy are Birnam-authored. Native additions are owner-provided constructors for the existing syntax representation and a general UTF-8 error-offset query. Evaluation still runs on the Odin seed; initial loading still uses its bootstrap parser. After explicit takeover, the tested authored path parses and reconstructs its own modules without calling that parser.

This is an authored front end for the seed subset. Full language coverage, native compilation, compiler IR/CBPV experiments, images, fibers, source dependency resolution, coordinated module reload, and native-speed performance remain unqualified or undelivered. Matching and translation use the interpreter/native stack, and PEG has no memoization or linear-time guarantee.
