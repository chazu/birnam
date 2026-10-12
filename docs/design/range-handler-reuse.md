# Range handler reuse — 2026-10-12

`Range size` and `Range at:` now select reusable argument-taking blocks instead
of constructing branches that capture the bounds, index and computed element on
every invocation. This follows the existing authored Integer implementation.
The benefit applies to Range consumers, including the shared iteration drivers
that repeatedly send `size` and `at:`. No native implementation is added.

Selection, handler invocation, bound getters, conversion, comparison and
arithmetic remain ordinary current sends. There are no method-replacement guards
in Range, no cached bounds or lengths, and no compiler/runtime specialization to
deoptimize. Users can replace these methods normally. As with other authored
algorithm rewrites, internal send traces and temporary branch identities change;
the implementation does not promise to reproduce an older algorithm's internal
callbacks after those operations are replaced.

The constructor, overflow checks, iteration drivers and bootstrap loading order
are unchanged. Indexing still computes `start + index` on each call and returns
that result, preserving fresh default Integer identities rather than returning
the stored lower bound. `do:` still ignores Error-valued callback results;
`inject:into:` still threads Error values. Mutation during a callback is visible
to the next iteration.

## Rejected alternatives

- Replacing `Integer compareInteger:with:` followed by `> 0` with direct `>`
  passed the fully loaded source checks but failed the native cold-bootstrap
  collections check near the signed 64-bit limits. Early Integer ordering uses
  Float conversion; the exact comparison service is necessary here. The final
  change retains the original comparison sends. This is a concrete example of
  why canonical arithmetic proofs alone cannot establish bootstrap compatibility.
- An alternate size-overflow calculation did not show a convincing gain.
- Vector owner-local temporary roots instead of registry tokens did not improve
  the measured application consistently; the Rust experiment was discarded.
- Caching Vector/String iteration extent added driver state without a reliable
  application improvement. Range bounds were never cached: they are live slots.

## Validation and measurement

The quiet paired run uses two fresh processes per variant, three warmed samples
per process, and 200 elements per operation. Median thread CPU time:

| Workload | Baseline | Reused handlers | Reduction |
| --- | ---: | ---: | ---: |
| Range `do:` with empty body | 146.45 ms | 135.38 ms | 7.56% |
| Vector `do:` control | 39.21 ms | 38.55 ms | 1.67% |
| Vector identity `collect:` control | 42.25 ms | 41.58 ms | 1.59% |

The controls show the scale of incidental drift; their changes are not attributed
to Range. The complete module suite on the final rebuilt prefix passes **672
assertions**, including 30 Range assertions. Qualification used a disposable
prefix; the user's normal installation was not changed.

The [measurement receipt](../measurements/range-handler-reuse-20261012.json)
records the final paired timings and acceptance results. Initial source overlays
used a different compilation mode from production; performance comparisons therefore use synchronous AOT Range
methods for both variants, loaded onto the same runtime. The default 16 MiB GC
threshold remains unchanged. A Range microbenchmark gain is not a demonstrated
whole-application speedup.

`modules/range-test.birnam` covers signed limits, overflowing counts and index
addition, invalid indices, element identities, Error-valued callbacks, forced GC,
and changes to bound slots/getters. The task regression changes a bound while a
Range callback is suspended, collects, and resumes at quanta 128 and 1.

`RangeHandlers.lean` proves default size results, endpoint-selection value and
identity, and overflow examples. Its wrong-endpoint mutation must be rejected.
The existing reusable-branch Lean/TLA+ models cover current handler entry,
selected-only invocation, argument roots and no replay. These are abstract
contracts, not a proof of all native code or generated machine instructions.

The formal gate passes 191 Lean theorems, rejects 38 proof mutations, and
checks all 118 TLA+ configurations, including required counterexamples. Native
collections and all 177 task-control assertions pass on the rebuilt candidate.
Saved task images pass mapped and rebuilt loading.
