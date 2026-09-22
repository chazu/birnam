# 73 — Scalable sorting and String ordering

**Status: ACCEPTED 2026-09-09. The user accepted both section 1 decisions (the
sorting contract and the String traversal contract), including the section 3
input/result behavior and the section 5 send/boot behavior. Implementation
proceeds as two separate stories, `birnam-epic-as7.5` (stable scalable sorting)
and `birnam-epic-as7.6` (early-exit String ordering), under section 7.**

Epic: `birnam-epic-as7`. Design gate: `birnam-sorting-plan-byd`.
Implementation: `birnam-epic-as7.5` (sorting) and `birnam-epic-as7.6` (String).
This is the requested review stop after the initial implementation stories.

## 1. Recommended decisions

Accept the following separately; accepting one does not implicitly accept the
other. The implementation tickets remain blocked by the open design gate.

| Decision | Recommendation | Observable change |
| --- | --- | --- |
| Sorting contract | Require a consistent strict weak ordering for sorted-result guarantees. Preserve stable ties and once-per-element key evaluation. Comparator call count/order is not guaranteed. | Effectful `<` implementations see a different comparison transcript. |
| String traversal contract | Compare bytes through ordinary sends and stop at the first unequal pair. Read each input length once at entry. | Fewer size/fold/control sends; no byte reads or traversal after the answer is known. Overrides on the visited path still run. |

The proposed implementation is stable bottom-up mergesort with a linear
already-ordered check, followed in a separate change by resumable String byte
traversal. Neither decision permits cached method targets, native bypasses of
ordinary comparison/byte access, boxed-Integer canonicalization, or a changed
IdApi/image format.

## 2. Current behavior and evidence

The current native [Vector sortBy:](../../rust/birnam-collection/src/lib.rs)
copies elements, evaluates all keys in input order, then insertion-sorts an
index vector. It sends `candidate < previous`; only canonical true moves the
candidate. It constructs a new Vector for input sizes of at least two.

Current [String <](../../src/id/stdlib.birnam) folds over the shorter length.
Once unequal bytes decide the answer, subsequent iterations still execute
accumulator/control checks. It stops byte reads, but does not stop traversal.
The proposed change is therefore broader than adding a missing byte-read guard.

[Design 69](69-sorting-and-string-comparison.md) retains the investigation and
its diagnostic provenance. Reverse-sorted numeric inputs at 32/128/512 elements
require exactly 496/8,128/130,816 insertion comparisons. Its String fixtures
show why early differences and long common prefixes need separate measurements.
Those timings are diagnostic, not host-gated acceptance or speedup promises.

Documentation sorts module/display/file names, and project policy sorts
dependencies and roots. These are current consumers, before any further
implementation is moved into Birnam. A future compiler also needs predictable
collection costs and bounded native-stack use.

The initial CodeProgram/global/method-index work does not change either ordering
implementation. Its correctness evidence is separate in
[the implementation receipt](../measurements/selfhost-initial-implementation.json).
Its timing acceptance remains pending by user instruction.

## 3. Sorting contract in detail

The key block must return normally for the operation to complete. For normal
sorted-result guarantees, key comparison must induce a consistent strict weak
ordering for the duration of this operation: irreflexive, transitive, and with
transitive equivalence when neither key is less than the other.

This is a precondition for the result guarantee, not a runtime ordering validator.
The implementation does not add speculative comparisons to prove consistency.

| Case | Proposed behavior |
| --- | --- |
| Empty/singleton Vector | Return the receiver; do not invoke the key block, as today. |
| Two or more elements | Snapshot elements and compute keys exactly once, in input order, before comparisons. Produce a new Vector, including when already sorted. |
| Equivalent keys | Preserve their input order under a consistent ordering. |
| Key callback returns Rust null | Abort key extraction and answer language nil, as today. Do not compare incomplete keys. |
| Key callback returns a language Error or nil | Retain that ordinary key value, as today. Do not introduce automatic Error propagation. |
| Comparison returns canonical true | Treat the left operand of that send as less than its right operand. |
| Comparison returns false, nil, Error, or Rust null | Treat it as non-true. Do not add a second comparison or propagate it as a sort failure. |
| Inconsistent/effectful comparisons | Keep a bounded algorithm and preserve all input elements, but do not promise a sorted result or the old insertion-sort transcript. |

Comparator mutation remains visible: every comparison makes a fresh ordinary
`<` send against the current lookup rules. Key extraction is already a snapshot;
mutating a key object may still affect its later comparison behavior. A callback
that changes ordering while sorting cannot expect the strict-ordering guarantee.

For illustration, the current insertion sort of `[3, 1, 2]` compares `(1,3)`,
`(2,3)`, `(2,1)`. The proposed check-and-merge algorithm may compare `(1,3)`
twice and compare `(2,1)` before `(2,3)`. This transcript is derived from the
algorithms, not a new measured fixture. Both yield `[1,2,3]` for numeric keys.

## 4. Sorting algorithm and lifetime boundary

1. Keep the existing element/key snapshot and scoped root provider. Finish key
   evaluation before making any comparison.
2. Initialize the index vector to input order. Check adjacent keys by sending
   `next < previous`; stop that check at the first canonical true. If none is
   true, construct the result from the original order. This preserves linear
   comparison work for already-sorted/equivalent input, an existing strength of
   insertion sort that plain bottom-up mergesort would lose.
3. Otherwise merge adjacent runs of widths 1, 2, 4, and so on, alternating two
   index buffers. Send `right < left` once for each choice. Select right only
   for canonical true; otherwise select left, preserving stable ties.
4. Build the result through the existing Vector construction path. Do not
   return the original receiver as an additional identity optimization.

Comparison work is O(n log n) worst-case and O(n) for the ordered check's fast
case. Scratch storage is O(n), including one additional index buffer compared
with the present implementation. Measure that retained/peak storage separately
from GC-managed result and key allocation. Use checked width/index arithmetic.

The index buffers contain positions, not Oops. The existing scoped provider must
keep receiver, key block, copied elements and all keys live across comparison
callbacks, including nested sorting and explicit GC. No Rust borrow or mutable
root-list borrow may span a send. The sort remains inside its current native
callback boundary; this proposal does not add suspension across that boundary.

Do not add a guessed small-sort cutoff, Rust generic sorting, specialized
numeric/String comparators, or descending-run reversal in this first slice.
The extra ordered check is explicit here because its comparison transcript is
part of the behavior change being reviewed, even though it is not a promised
future transcript. Measure small and random inputs before retaining it.

## 5. String ordering and stackless compilation

For ordinary Strings, the result remains lexicographic byte ordering. There is
no Unicode normalization, locale collation or case folding.

The proposed source algorithm reads left size and then right size once. It
visits indices from zero to their minimum length. At each index it sends
`byteAt:` to the left input and then the right input; unequal bytes decide the
result immediately. A completely equal visited prefix compares the captured
lengths. Empty inputs require no byte reads.

Ordinary size, byte access, byte equality and byte-order sends remain observable
on the executed path. Do not replace them with a native memcmp or retain their
method targets. The algorithm may change intermediate control/fold sends and
length-read counts. Callers overriding these protocols must not rely on the old
exhaustive fold transcript. No new non-String coercion or error policy is added;
the implementation must characterize existing failure cases before changing code.

Prefer a tail-recursive source helper carrying the index and captured lengths,
compiled as resumable together with the production String `<` entry. All its
nested blocks must share resumable selection. A synchronous recursive helper
would grow the native stack with prefix length and is unacceptable.

There is a bootstrap constraint: [the host](../../rust/birnam-host/src/lib.rs)
registers synchronous self-hosted units before `stacklessControl`, and
[the installer](../../rust/birnam-cli/src/bin/birnam-build.rs) selects only that
unit for initial-generation resumable compilation. Merely adding recursion to
stdlib would therefore be wrong.

Use the existing bootstrap/production override pattern: retain the current
stdlib comparison for the pre-control bootstrap phase, and install the
resumable production entry and helper with `stacklessControl`, as that unit
already does for iteration and collection control. Both implementations must
agree on ordinary String results. A later removal of the bootstrap fallback
requires its own proof that no pre-control caller needs it; it is not assumed
safe here. No additional toolchain or prebuilt Birnam bootstrap dependency is
introduced.

The new helper is an ordinary reflective method, not a private frame opcode.
Include its definition and ownership in the usual source/portable artifact
inventory. Rebuild initial-generation artifacts through the existing process;
do not hot-patch a saved native address or silently alter an old artifact.

## 6. Alternatives considered

| Alternative | Assessment |
| --- | --- |
| Keep exact insertion comparator transcript | Preserves arbitrary callback observations but retains quadratic comparison work. Choose this only if that transcript is intended language behavior. |
| Add opt-in sorting/comparison selectors | Preserves old behavior but duplicates the API and leaves current docgen/project consumers slow until migrated. Prefer clarifying the existing contract. |
| Plain bottom-up mergesort | Simpler, but worsens already-ordered comparison scaling from O(n) to O(n log n). The proposed precheck addresses that known case. |
| Native String byte scanner | Potentially much faster on long equal prefixes, but bypasses visited-path ordinary sends. This is a separate, broader semantic decision. |
| Synchronous recursive String helper | Rejected because input length would determine native-stack depth. |

## 7. Implementation and acceptance after review

Implement the sorting and String stories separately, with their own differential
behavior and performance evidence. The code changes below do not begin while
this document remains proposed.

**Sorting:** characterize key evaluation, result identity, stable tags on equal
keys, null/non-true/Error cases and callbacks that mutate lookup, force GC and
invoke nested sorts. Then implement the reviewed algorithm. Differentially
compare output with the existing implementation for consistent orderings; do
not assert identical callback transcripts. Verify the output is a permutation
even for inconsistent comparators that return normally.

**String:** pin empty/prefix/equal/non-ASCII byte order, early and late differences,
existing failure behavior, and visited-path overrides. Prove the exact length
and byte-read bounds with instrumentation. Check long equal-prefix execution
is stack-flat in AOT, forced-source, image-recovery and baked paths; test
replacement/rollback of the production method and bootstrap fallback behavior.

Run the full Rust verification gate and language suite, plus applicable
control/image/retirement tests. Algorithms remain target-neutral; eventual Linux
acceptance must exercise the same semantic fixtures through its own native ABI.

Retain host-gated numeric and String sorting at 0/1/32/128/512/4096 elements,
sorted/reverse/random/equivalent keys, and String early/late/equal-prefix cases.
Record comparison counts, elapsed time, allocation and peak scratch memory.
Include documentation generation and project builds. Keep existing benchmark
case bodies, names and thresholds unchanged. Pending baseline/timing work is
not waived by accepting this design.

## 8. Review checkpoint

The requested decision was whether to accept the two contracts in section 1,
including the input/result behavior in section 3 and String send/boot behavior
in section 5. **Decision: both accepted by the user on 2026-09-09**, recorded
here and on `birnam-sorting-plan-byd` (closed). The pending timing work named
in section 7 closed the same day (Design 68/71 acceptance); implementation
acceptance for the two stories still requires their own host-gated evidence.

The Map, Store, allocation and Linux designs remain separate future reviews.
No sorting or String implementation has been changed for this proposal.
