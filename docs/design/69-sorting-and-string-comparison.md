# 69 — Sorting and String comparison

Status: investigation and proposed plan recorded, 2026-09-05; semantic approval
pending. Ticket: `birnam-sorting-plan-byd`. No production changes.

## Evidence

Rust `vector_sort_by` computes keys once in input order, roots elements/keys,
then performs stable insertion sort over an index vector. Each comparison is an
ordinary `<` send. Only canonical true moves the element; false, nil and Error
all stop that insertion. Rust null aborts key extraction with nil. Vectors with
fewer than two elements return the receiver without evaluating the key block.

Source-authored String `<` folds over the shorter length. It stops byte reads
after the first unequal byte but continues iteration and accumulator checks.
The predicate is not a native byte comparison. Documentation and project-policy
sorting are real consumers; tests cover basic String order and GC during key
extraction, but do not establish arbitrary effectful comparator behavior.

`bench/sorting-investigation.birnam` was run in three fresh release processes
with default GC; all sorted-output checks passed. Representative second run,
microseconds (diagnostic only, no host gating or speedup claim):

| Elements | Already sorted | Reverse sorted | Exact reverse comparisons |
| ---: | ---: | ---: | ---: |
| 32 | 22 | 93 | 496 |
| 128 | 47 | 1,393 | 8,128 |
| 512 | 217 | 22,457 | 130,816 |

For String suffix lengths 8/64/256, early-difference times were 342/1,406/5,748
us; late-difference 277/1,762/7,813 us; equal 247/1,669/65,598 us. Single-operation
samples include GC and warmup effects; the large equal case is not a stable
per-comparison estimate. Source inspection establishes exhaustive folding even
for an early difference. Separate these cases in future controlled benchmarks.

## Decision required

Proposed contract: sorting requires a consistent strict ordering; it preserves
stable ties and once-per-element key extraction but does not promise comparator
call count/order. String ordering may stop lower-level sends once its answer is
known. Ordinary replaceable `<` dispatch remains. Effectful comparators can
observe a changed sequence. The user has been asked to approve this explicitly.

Without approval, do not silently substitute mergesort or bypass derived String
sends. An opt-in new sorting selector is an alternative, but adds language
surface and also requires a decision. Exact insertion-sort callback transcripts
cannot in general coexist with O(n log n) comparison counts.

## Plan after approval

1. Characterize empty/singleton behavior, equal-key stability, key extraction
   order/count, non-true results, Errors and null. Add callback re-entry and
   forced-GC tests during comparisons, not just key extraction.
2. Replace only index-vector sorting with iterative stable bottom-up mergesort.
   Pick the right element only when `right < left` returns canonical true;
   otherwise pick left. Retain live sends, rooted input/key carriers and output
   ownership. Do not use Rust's generic sort with an inconsistent comparator or
   pretend two `<` sends equal one tri-state comparison. Keep key evaluation
   complete before comparison. Include O(n) scratch memory in measurements.
3. Investigate adaptive runs or a small insertion-sort cutoff only if the
   already-sorted/small-input benchmark regresses. Do not hardcode a cutoff
   without measurement; the baseline is linear on sorted input.
4. Separately implement an early-exit source-authored String comparison,
   preserving byte ordering, prefix behavior and ordinary byte/size messages
   on the visited path. Any recursive helper that scales with string length
   must be selected for resumable compilation: stdlib itself is synchronous.
   Prove stack-flat operation; do not introduce native recursion proportional
   to string length. A native scanner is a separate contract/design choice.
5. Benchmark sorted/reverse/random/equal keys at 0/1/32/128/512/4096, numeric
   and String keys, early/late differences and long equal prefixes. Measure
   comparisons, allocation and host-gated end-to-end docgen and project builds.
   Run focused tests, `mise run verify`, `mise run test` and stackless-control
   smoke. Preserve release thresholds.

Do sorting and String traversal as separate commits with separate before/after
evidence. No IdApi expansion, target caching or comparator specialization.
