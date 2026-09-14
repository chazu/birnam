# 74AF — Complete optimization-consumer teardown

Status: focused compiler-storage teardown verification passed in both profiles,
isolated from the queued 74AE source checkpoint. No optimization family is
complete on this evidence.

74AE established the private compiler's allocation accounting and individual
Unit release. A consumer also owns candidate code, numeric kernels, recovery
plans, selected entries and caches. Its complete lifetime must release those
owners while preserving every ordinary Id effect and active native call.

## Implementation

The explicit `birnam-id/allocation-metrics` diagnostic feature enables the
existing private compiler accounting. Normal and existing resource-only builds
retain their original allocation path. No public API, execution, registration,
canonical descriptor, image or compiler eligibility rule changes.

Four isolated test entry points reuse the actual production-driver fixtures:

- Unboxed numeric results, aliases and forced exits at each original boundary,
  including the longer 16- and 32-operation chains.
- Loop invariants with input, method, lookup, declaration and GC changes, across
  boxed/unboxed and tail-transition modes; fresh results and ordered effects.
- Discarded clones with identity, allocation, escape and late-binding controls.
- Virtual fields across short/long regions, every supported forced call exit,
  collecting observers, aliases, reentry, capture and suspension.

Each entry point runs its full fixture matrix three times in one process. Every
matrix returns normally, destroying its Runtime registries, selected owners,
baseline Units and candidate kernels. Live compiler heap blocks/bytes and private
Unit/relocation totals must equal the original zero baseline after every matrix.
There is no counter reset, forced registry clearing or process-exit substitute.
The underlying fixtures retain their positive optimizer-activity and semantic
assertions; the lifecycle wrapper does not replace them with a trivial compile.

Compiler totals cover the linked instance, so these tests are explicitly ignored
by ordinary concurrent test runs. `scripts/consumer-teardown-smoke.py` compiles
both profiles with the diagnostic feature, obtains the exact test executable
from Cargo's artifact message, and runs each exact test in a separate process
with one test thread. It retains source hashes, binary hashes, commands, raw
output, exit status and all three cycle observations. It rejects missing tests,
missing/reordered cycles and any nonzero retained compiler resource.

## Remaining resource acceptance

This is compiler-storage evidence after complete Runtime destruction, not total
Rust allocation accounting. Plans or caches that own no TinyCC Unit can retain
Rust memory without appearing in these counters. Independently observe release
of those owners and bound any deliberately persistent pools during a live
Runtime. Heap and relocated-byte counters overlap on Darwin and must not be
added to estimate process footprint.

The closure consumer depends on the ordinary Codegen native declaration/context.
Pure metadata/preparation unit tests cannot replace its installed-source lifetime.
Its next test must materialize a fresh source CodeObject, warm and prove actual
closure substitution, retain shared Cells and returned identity across cache
eviction and GC, release the final target, then establish generation/candidate
retirement. Extend that same live-runtime exercise to the numeric, loop and
aggregate consumers and the combined numeric-closure implementation. Preserve
outer native caller roots; exclude only the probe's own temporary stack values.

Keep source-driver, ABI/image, installed application, cold/preparation/steady
cost and whole-process resource acceptance separate. Continue unqualified
measurements under the user's waived quiet-host requirement. This test changes
neither optimization defaults nor the four-family completion contract.

## Completed focused evidence

`74af-consumer-teardown-corrected.json` passes all eight exact test processes:
four consumers in debug and release, each repeating its complete semantic
fixture matrix three times. All 24 post-matrix observations report zero private
Units, relocated bytes, compiler heap blocks and requested heap bytes, after
normal destruction in the same process. The ordinary feature configuration also
passes `cargo check` (`74af-normal-check.json`).

The first numeric execution already passed all three cycles. Its initial Python
parser rejected the first observation because libtest prints it on the same line
as the test name. `74af-consumer-teardown.json` retains that failed parsing attempt
and successful raw execution. The repair accepts only the exact expected test
name prefix; eleven admission checks cover the actual output and reject failed
processes, missing/duplicate/extra cycles, mismatched test names/families and
nonzero retained resources. Runtime and fixture assertions were unchanged.

`74af-consumer-teardown-audit.json` independently reparses every complete raw
process, checks both executable hashes and profile/feature builds, validates the
normal build record and the retained failure, and proves the exact eight-path
difference from 74AE. The reused fixture bodies are byte-identical except for
making four test functions visible to their parent test module. The new wrapper
is gated by both `cfg(test)` and the explicit diagnostic feature. This evidence
does not transfer automatically to the later numeric-closure composition or
establish release of Rust-only metadata and live-runtime source consumers.
