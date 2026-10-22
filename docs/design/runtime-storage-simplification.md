# Runtime storage simplification with unchanged malleability

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
| Shared immutable method layout data (proposal 5) | Pending |
| Reused collector workspace (proposal 4) | Pending |

## 1. Argument normalization

The experiment borrowed an invocation's immutable ABI argument packet when it
had no nullable Object to translate. Source calls containing null still copied
and normalized it to canonical nil. The required owned Oop buffer for frame
entry and all message protocols remained unchanged. The new production fixture
checks mixed carriers, nullable/native semantics, collection and reentrant
invocation without overwriting the outer packet; it is retained as a regression
check even though the optimization is rejected.

[Evidence](../measurements/runtime-storage-arguments-20261022.json). Serial ABBA,
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
