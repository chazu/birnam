# Authored PEG validation

5 November 2026. This receipt qualifies the first milestone: the Odin seed executing Birnam-authored PEG. It covers the general collection/control support committed as `69d65e9` and the authored implementation described in [PEG](peg.md).

## Build inputs

The pinned environment is macOS 27.2 ARM64, Odin `dev-2026-10:84bc3fc21`, Apple Clang 16.0.0 (`clang-1600.0.26.3`), SDK 15.0, and Boehm 8.2.12. [toolchain.json](../toolchain.json) pins the versions and library/SDK checksums, enforced by the build tool.

The checked source/build-input SHA-256 is `96d610b245179dab1b38f7464becd9b0c395b00c321eb8e2c753e05e9fa69b2a`. The digest covers sorted relative paths, a NUL, file bytes, and another NUL for `Makefile`, `toolchain.json`, `tools/build.py`, and every non-`__pycache__` file under `src`, `include`, `tests`, `examples`, and `modules`. Documentation is excluded.

`make verify` passed in the working checkout. After adding the final within-parse replacement assertion and refining the GC report, the complete gate passed again in `/private/tmp/birnam-peg-clean-2uxuwyao`. That fresh directory contained only the listed source/build inputs, without Git metadata or existing build products. The generated local log is `build/peg-clean-verification.log`. The build used the pinned installed native toolchain and collector; no old checkout, Rust/Cargo, or installed Birnam supplied build inputs or execution.

Odin builds used `-o:speed -debug -vet`. C fixtures used `-O2 -Wall -Wextra -Werror` with the existing unused-parameter exemption. The authored module is loaded from source through `seed.dylib`.

## Observations

- The original native contract, handover, failed-module-loading, and different-working-directory checks passed; 4,096 of 4,096 registered native objects were finalized.
- Seed source semantics and 26 negative CLI cases passed. The general helper corpus covered Cell cloning, message-based loop/short-circuit control, persistent Vector append/concatenation, present-nil namespace bindings, and bulk byte-prefix comparison. The seed C witnesses finalized 600 of 600 discarded closures and passed collection during source parsing.
- `tests/peg_test.py` ran the example and authored corpora from an unrelated working directory. The example printed `#root`, `0`, `3`, `#word`; a discarded parse Error stopped the CLI before the following print.
- `tests/peg/semantics.birnam` passed nested labels and byte spans, prefix versus explicit EOF, ordered choice/backtracking and child rollback, constructor aliases, repetition/optionality/lookahead, zero-width star/plus behavior, empty combinators, and invalid byte ranges.
- Structural `undefinedRule` Errors survived choice, optionality, both lookaheads, star, and plus. Direct and indirect left recursion were distinguished from valid advancing recursion. A finite chain of 128 references produced the expected nested CST.
- Diagnostics reset per parse, remained independent across grammars, and counted UTF-8 bytes. Literal parsing also accepted arbitrary Bytes produced by splitting a multibyte character.
- `tests/peg/replacement.birnam` verified that matching Methods expose authored code; local Method edits affect later parsing without altering siblings; shared behavior edits affect existing patterns; and captured Methods retain the original implementation. Rule binding changes take effect next parse, while Method changes can affect the next matching send within the current parse.
- Reentrant parsing of the same grammar kept separate input, active-rule, and diagnostic state across collection. Each completion published its own diagnostic. Repeated attempts at one rule/position sent again, with observable effects rather than hidden memoization.

## Collection and dependency boundary

`tests/peg_contract.c` observes the authored matcher through a native test wrapper. It registers finalizers on parse contexts, result trees, and temporary grammars, periodically collects while the authored match is active, then checks retained objects through messages. The wrapper delegates to the captured authored Method; it does not implement matching or decode CST payloads.

The clean run finalized **401/401 contexts**, **400/400 discarded trees**, and **400/400 temporary grammars**. The context count includes the first parse producing a retained tree: the tree does not need to retain its parse context. A retained grammar and CST remained usable, and the grammar parsed again after collection. Reported heap size stayed at **1,064,960 bytes** across the bounded workload. The assertions allow conservative retention (at least 360 finalizations per category) and up to 16 MiB heap growth; this evidence is not a universal retention guarantee.

Production PEG behavior resides in `modules/peg.birnam`. Added native support consists of general Cells, Block/Boolean control, Vector operations, namespace fallback, and byte-prefix comparison. The dependency chain is the native siphon, the separately loaded Odin seed, then authored PEG source. No old native PEG engine or Rust artifact participates.

## Limits

This completes the scoped, single-threaded interpreted PEG milestone on this Mac. It does not qualify full Lang, a compiler, native generated code, image persistence, dependency-aware source loading or coordinated module reload, fibers, Alien, concurrent publication, incremental collection, or CBPV. There is no memoization, complexity guarantee, throughput claim, or arbitrary recursion-depth guarantee. The implementation uses copying persistent Vectors and native call-stack recursion; larger workload/performance design remains open.
