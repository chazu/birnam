# Runtime storage simplification with unchanged malleability

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

> **Historical scope:** These are the earlier storage experiments, including rejected candidates. They are not measurements or design instructions for the corrected Oop-only branch. See the
> [complete work summary and current checkpoint](logue-runtime-work-summary.md).

Approved 2026-10-22 after the first [runtime simplification batch](runtime-simplification-results.md).
This batch changes internal storage only. It preserves current selectors, Id
lookup, allocation/representation hooks, logical frames, selected code ownership,
image formats, callback order and typed failures. Selector caching and boxed
Machine-index protocols are outside this batch.

Implement and measure one slice at a time, committing its evidence separately.
No new Rust-stack work buffers; retained workspace belongs to explicit owners.
Native allocation savings are distinct from language-object allocation counts.

| Slice | Status |
| --- | --- |
| Argument normalization without redundant copying (proposal 3) | Measured; not retained |
| Shared immutable method layout data (proposal 5) | Complete |
| Reused collector workspace (proposal 4) | Measured; not retained |

## 1. Argument normalization

The experiment borrowed an invocation's immutable ABI argument packet when it
had no nullable Object to translate. Source calls containing null still copied
and normalized it to canonical nil. The required owned Oop buffer for frame
entry and all message protocols remained unchanged. The new production fixture
checks mixed carriers, nullable/native semantics, collection and reentrant
invocation without overwriting the outer packet; it is retained as a regression
check even though the optimization is rejected.

[Evidence](../../measurements/runtime-storage-arguments-20261022.json). Serial ABBA,
two fresh processes per variant, five 200,000-call samples per focused case:

| Owned native-ABI invocation | Before | Candidate |
| --- | ---: | ---: |
| Authored implementation, one non-null argument | 65.76 ns | 60.69 ns |
| Authored implementation, eight non-null arguments | 85.37 ns | 70.24 ns |
| Authored implementation, eight arguments ending in null | 78.95 ns | 86.99 ns |
| Native implementation, eight arguments | 66.68 ns | 67.91 ns |

The ordinary cases remove one host allocation and improve 7.7%/17.7%, but the
nullable case regresses 10.2%. Trials combining normalization with root admission
or copying after finding the first null did not resolve the tradeoff. The
existing implementation is retained; no root, normalization or argument-storage
change from this experiment is installed or committed.

The complete Logue phase sequence changes 11.37 → 11.28 seconds (-0.79%), redraw
22.44 → 22.36 ms (-0.36%) and floor roundtrip 3,032.51 → 3,020.92 ms (-0.38%). These
do not establish an application gain. All four PTY outputs match exactly.
The candidate passed 27 Execution unit checks, the new production argument test,
compiler and method-invocation suites, and 177 task-control checks. Rejection is
for the measured cost tradeoff, not a semantic failure.


## 2. Shared immutable method layouts

Each binding retains its existing descriptor, identity, executable owner, step
entry and derived entry state. Its immutable slot kinds, call sites/plans, root
bitmaps, edge-copy tables and capture layout are shared through immutable array
references. Rebinding no longer deep-copies these tables. Scalar fields and array
lengths remain directly available in the descriptor, preserving the existing
hot frame-access shape.

The first trial grouped all layout fields behind a shared layout object. It
improved binding construction but added pointer indirection to frame access;
paired Logue runs were 3.7% slower overall and generation 7.1% slower. That
representation was rejected. The retained design shares arrays directly and
introduces no additional layout abstraction.

Call-plan admission assigns a new immutable table to the receiving descriptor,
so it cannot modify another binding's table. No method lookup, source message,
logical frame or serialized format changes. The unit fixture proves shared
storage with independent owners, generations and derived-entry caches. Production
binding/replacement and mapped/rebuilt image checks cover the public behavior.

[Evidence](../../measurements/runtime-storage-layout-20261022.json). The binding
workload takes Darwin live-malloc statistics around each 10,000-binding batch,
with collection and compilation outside measurement. Each binding adds four
live host allocations instead of eight. Eight- and sixteen-argument templates
use 576 bytes/binding instead of 800/864: 224/288 bytes saved. Zero-argument byte
counts also include variable registry growth; the allocation-count reduction is
stable there too.

External compiler/linker load roughly doubled even unchanged baseline timings.
The final paired runs request the same foreground thread QoS for both variants;
these are not quiet-host measurements. Binding medians are 2.52 → 2.22 µs,
2.54 → 2.34 µs and 2.74 → 2.39 µs (0/8/16 arguments). The complete Logue sequence
is 25.99 → 24.86 seconds, but no application-speedup claim is made under this
load. The deterministic host allocation/storage reduction is the primary result.
All four terminal outputs match exactly.

Checks: 26 Execution unit tests; production compiler, Machine, method binding,
method invocation and candidate-retirement suites; source-free mapped/rebuilt
image checks with method replacement and collection. No expectation was relaxed.
An initial timing run overlapped a correctness test and was discarded. Other
uncontrolled runs under the external build load were not used for a speed claim.


## 3. Collector-owned workspace reuse

The experimental collector retained empty capacity for the allocation inventory, address
index, marks, pending queue and core-edge snapshot. Activation and marking shared one preparation function; activation no longer cloned the whole inventory
just to iterate it. Between cycles, all logical contents, marks, errors and
visitor-admission state were cleared. Fresh `allocationsDo:`, root enumeration,
owner/identity queries, tracing and ordered retirement still run each time.
No answer or permission was cached, and no borrow spans an owner callback.

Cleanup recycled storage after successful collection, marking errors and
activation failure. The existing terminal failure behavior after partial
retirement is unchanged. A deterministic unit test checks pointer/capacity reuse,
stale-address removal, reset marks/failure/admission, duplicate-address refusal
and successful preparation after that refusal. Eight production collection,
activation, accounting, object-model and whole-heap suites pass.

The tradeoff was retained high-water scratch capacity in the collector context;
it holds no logical objects between cycles. Id's own census snapshot and the
dead-object/retirement lists remain separate allocations. This is workspace
reuse, not incremental collection or removal of the full-heap traversal.

[Evidence and candidate patch](../../measurements/runtime-storage-workspace-20261022.json).
Two processes per variant, serial ABBA with the same foreground QoS; the external
build remained active. Five precise collections per case used 200,000 live
objects, then the same live set plus 200,000 newly dead objects per collection.
Live-only medians were 314.61 → 313.75 ms (-0.27%); garbage medians were
499.61 → 514.59 ms (+3.0%). Logue's complete sequence was 26.27 → 26.50 seconds
(+0.91%), ordinary redraw 52.14 → 53.41 ms (+2.44%), and floor roundtrip essentially
unchanged. All terminal outputs match.

These samples do not establish a speedup. Four unit checks and eight production
GC suites passed, but buffer reuse by itself is not acceptance evidence for a
performance improvement. The candidate was removed before final delivery rather
than retaining extra workspace lifetime/state and peak memory without a clear
benefit. Its exact patch is retained in the evidence for a future quiet-host trial.
The production collector is unchanged by this follow-up batch.
