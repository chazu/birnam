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

## Source closure lifetime and collected binding republication

The next diagnostic uses ordinary source-created factories and shared Cells,
with separate REPL evaluations for materialization, warm calls, cache eviction,
continued use and final release. Weak host observations follow the actual
registered descriptors, preparation inputs, candidates, closure plans and
constructor-summary owners. They keep no language objects or strong plan owners
alive. Snapshotting TinyCC storage is separate from observing these Rust owners.

The first run proved actual closure substitution (eight measured hits and 24
completed calls) and retained factory ownership through eviction. Invoking the
same factory afterward aborted with `resumable binding addition accounting
failed`. The exact replay and optimizer-off probe reproduced it. A six-line
ordinary Id program also reproduced it on the existing normal installed floor,
with optimization both off and on. Removing eviction allowed continued calls;
the final-release assertion then correctly failed because the cache still owned
the target. Removing explicit collection did not prevent the abort because the
generation-retirement attempt itself performs collection. These observations are
retained in `74af-closure-lifetime-first.json` and `74af-lifetime-diagnosis.json`.

The collector had removed the last direct binding of an unreachable nested
Block vtable and irreversibly retired its entry, although a factory in the same
admitted generation was still live. The candidate fix identifies BlockBody
registrations through the existing admitted manifest and returns a collected,
unbound entry to `Unpublished` after its final reader/acquisition/Frame/
Continuation/explicit lease releases. Ordinary dispatch rejects an unpublished
entry; a real reconstructed vtable binding can publish it again. Explicit MOP
removal and replacement still retire an entry irreversibly, and an entirely
quiescent generation can unregister its unpublished entries without rebuilding
them. Named implementation anchors do not receive this treatment. No public
IdApi field, Method convention, canonical descriptor, image or registration
carrier changes are involved.

Two focused execution tests pass, covering all six holder cases, explicit removal
after republication, stale dispatch rejection and final generation removal.
Normal-floor source regression passes eight cases per profile (16 processes),
and both complete runtime suites pass 220 tests with 15 ignored.
`74af-block-republication-checks.json` binds all six completed steps to the
current source inventory; the corrected audit verifies unchanged ABI authorities
and the exact exported symbol set with matching Cargo features. These are normal
floors loaded with the existing published module prefix, not a fresh installation.
The source regression repeats each factory lifecycle three times,
checks ordinary results and shared Cell identity before/after eviction, and keeps
an escaped closure usable after its factory is released.

The fixed diagnostic gets past the abort, but does **not** pass complete release:
`74af-closure-lifetime-fixed-debug.json` retains the failure. Rebuilt Codegen
vtable-cache rows outlive the already-evicted materialization-cache entry. A later
`evictMaterializations` enumerates ready materializations and therefore misses
those rebuilt rows, which still root the generation and its optimizer metadata.
The second warm interval also falls back (zero closure hits) after eviction.
These are unresolved source-lifetime/admission findings, not accepted release or
speedup evidence. The next implementation must make rebuilt cache ownership
follow the generation's real lifetime, preserve observable vtable identities and
mutations, and release the final generation without counter resets, dropping live
roots, or treating process exit as collection.

### Next cache-ownership implementation

Move materialized JIT vtable-cache values under their actual TCC Unit owner.
A live factory or escaped closure already retains that Unit. The Unit should
trace its cache values, and an unreachable Unit/cache/vtable cycle should be
collectable normally. Repository materialization eviction can then remove its
own lease while preserving the exact cached vtable identities and mutations of
still-live factories. This avoids replacing the leak with another permanent
process-level generation set or a weak address cache vulnerable to address reuse.

Bind materialization-specific cache shims when constructing the JIT Unit; keep
mapped-candidate cache ownership on its existing path. Use the floor's existing
object layout/trace facilities and an explicit native Unit cache contract, with
checked owner/type/lifecycle access. No extra IdApi field is required. Verify
that cache lookup/set introduces no unaccounted collecting boundary in closure
construction, that all referenced values are rooted through writes, and that
malformed or released owners fail safely. Re-run the same failed complete-release
probe and assert vtable identity and method mutation across eviction, then extend
coverage to numeric/loop/aggregate materializations and native retirement.
This is the next implementation design, not a claim that the cache fix exists.


### Unit-owned cache candidate

The next candidate binds materialization-specific cache functions through
`TccUnit bindVtableCache`. Both synchronous and resumable materialization use
them; mapped artifacts and other generated-code paths keep their existing
cache contract. The emitter and generated C signatures are unchanged.

The TCC module owns a native map indexed by actual Unit identity. Its keys are
bookkeeping, never GC roots. The existing Unit trace hook follows only that
Unit's values. The production release path removes its map before deleting the
TCC state, including failed compilation. The Unit payload layout is unchanged.
Binding validates exact heap extent and Unit ancestry before state access, and
released or unbound owners reject cache access. No lookup/write calls IdApi or performs a
language allocation, dispatch or collection; normal Rust map allocation does
not change logical Id allocation accounting. Diagnostic storage queries release
the native map lock before boxing their answer.

The first candidate incorrectly restricted values to vtables. The unchanged
emitter also caches admitted Code objects; rejecting one returned nil into a
StepMethod and caused `invalidStepAction`. The failed replay remains in
`74af-unit-owned-first.json`. Accepting Code/object carriers under the existing unsafe generated-cache
contract fixes that regression. The final write path also removes reflection
callbacks: even an instrumented IdApi cannot introduce collection at this edge.
Owner registration and null-value checks remain; malformed non-null Oops are
outside the native caller contract. The exact original source now reaches final release with all
17 watched owner observations dead (`74af-unit-owned-values.json`). Optimization
records are used before eviction; subsequent calls still fall back safely. The
shared `fused_numeric::controls` guard rejects optimization while any stackless
generation retirement is pending. That explains the zero post-eviction hits;
this resource fix does not change that control policy.

The expanded resource driver initially compared snapshots at different GC
boundaries and counted its own uncollected REPL evaluator Units. Per-cycle
sampling in both GC modes showed constant storage rather than accumulation.
Running ordinary GC immediately before each identical query sequence returns
all six baseline TCC/cache metrics exactly to baseline after every lifecycle.
Both observations are retained in `74af-unit-storage-diagnosis.json` and
`74af-unit-storage-collected.json`; no root or counter reset was used.

The first corrected matrix passed all eight diagnostic combinations and the
normal resumable identity/mutation case before finding the separate synchronous
owner issue below. The final corrected source matrix passes in both profiles. It repeats three complete
factory lifetimes per process with optimization off/on, both GC modes and
private/escaping closures, checking weak optimizer ownership plus private and
baseline compiler storage. Normal-floor cases additionally check synchronous
and resumable vtable/Code identity and method edits across eviction. Release
builds and module suites pass. This candidate is not yet integrated
or accepted as full production/resource verification.


### Synchronous escaped-closure owner correction

The direct synchronous compiler deliberately creates fresh nested vtables and
Code objects on each invocation (`portable_c.rs`). The initial test incorrectly
expected resumable-style sharing; the corrected test explicitly requires fresh
identities and per-vtable method edits instead. The emitter keeps this behavior.

A separate real failure occurred when a synchronous nested closure outlived its
factory. The emitter passed the tagged `__birnam_code_generation_identity` to
the materialized-Block constructor as an Oop. That scalar cannot trace its Unit.
`74af-synchronous-owner-replay.json` reproduces the stack overflow on the existing
published installation after factory release/eviction/GC; the same program passes
with the corrected TCC/Codegen modules.

Synchronous generated units now carry an explicit private
`__birnam_materialization_owner` word, initialized to the actual relocated Unit
before construction. Nested closures copy that Oop into their existing traced
V2 carrier, just as resumable materialization already does. No public function
signature, IdApi slot, Block carrier, canonical descriptor or image format changes.
Only this private generated owner reference changes; the generation remains a
scalar key and nested allocation/identity behavior stays intact.

`74af-unit-owned-verification.json` binds the final 302-input candidate, normal
and diagnostic builds, both source matrices and compiler/TCC/Codegen tests. Its
status is now passed; the two earlier matrix failures remain preserved.


### Focused final acceptance

`74af-owned-completion-audit.json` validates all eight completed build/source/test
steps against the final 302 inputs and current binaries. Both profiles pass
86 compiler, 18 Codegen and one TCC test. All 24 primary source processes pass:
48 complete closure lifetimes with optimizer/private/baseline storage checks,
plus eight normal-floor identity/mutation cases. Four additional synchronous
source cases per profile add 24 complete direct/nested lifecycle cycles with
exact TCC unit, relocated storage, heap and cache return to baseline.

The supplemental debug runner completed all four processes, then erroneously
tried to hash its copied executable after deleting the temporary layout. Its
failed status remains in `74af-synchronous-storage-debug.json`. The separate
audit checks every raw result and storage snapshot plus the copied executable's
unchanged origin; the corrected release runner passes normally. This is recorded
as a verifier lifecycle error, not a runtime pass inferred from missing output.

In total the focused evidence covers 32 source processes and 72 complete
lifecycle cycles. The ABI/compatibility audit verifies the eight frozen
authorities, unchanged Unit payload layout and unchanged normal module exports.
Get/set has no IdApi or language callback. The synchronous emitter's private
owner word is the sole new generated carrier; no durable ABI changes.

This completes the focused ownership repair, not any optimization family. The
next gate is this candidate's full source/build/install/image/relocation
verification, followed by integration with the separately verified numeric,
loop, aggregate and closure consumers. Pending-retirement fallback, retained
Rust metadata accounting, broader application cases and profitability remain
explicit follow-through. All timings remain unqualified.


The focused ownership repair is committed at `e340e1da` on the isolated 74AF
branch, following collected-binding republication at `976acf64`. The full
production gate has started against its frozen 300 production inputs;
`74af-owned-production-start.json` records both live process identities,
prerequisite audits and the complete runner. Its live queue and proof are
`74af-owned-production-queue.json` and `74af-owned-production.json`.
Runtime integration remains pending that gate and subsequent installed acceptance.
