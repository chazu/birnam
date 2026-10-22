# Runtime simplification implementation and measurements

Approved 2026-10-22. Implement in ascending complexity, measure each retained
slice against its immediate predecessor, and commit its code, checks and evidence
together. Logue remains unchanged as an application acceptance workload.

## Accepted contracts

- Range traversal uses current bounds and ordinary bound-accessor/numeric sends.
  It need not dispatch `size`/`at:` per iteration. Keep changing bounds, fresh
  element identity, callback/error behavior, suspension and collection.
- Execution may expose a coarser compiled-call operation instead of promising
  interception of each compiler-generated Integer index or Machine slot access.
  Keep source-level replacement, logical frames, selected-call ownership and
  current Id messages. Demonstrate an authored Birnam implementation of the
  coarse operation before treating its contract as settled.
- Numeric optimization is a later, bounded use of existing Word lowering.
  Preserve changed factories/getters, escaping identities and recovery at
  observation points. Do not add a second arithmetic engine.
- GC scratch belongs to an explicit collector context and reusable owned
  storage. Do not base the optimization on putting traversal state on the Rust
  stack. Native call glue remains during this implementation; collector rewrite
  into Birnam is not claimed by this work.

## Sequence and status

| Slice | Status | Measurement / commit |
| --- | --- | --- |
| Established Id lookup improvement | Complete | `5018aad2`; [4–6% Logue CPU reduction](id-default-lookup-fact.md) |
| Collector scratch and metadata retirement | Complete | `1498a182`; results below |
| Live-bound Range traversal | Complete | `bcd31b31`; results below |
| Admitted call plans, coarse compiled execution and authored implementation | Complete | Results below |
| Bounded numeric temporary elimination | Planned | Pending |

The earlier [diagnosis](runtime-costs-exposed-by-logue.md) supplies the starting
workloads and costs. Proposed gains are not recorded as delivered gains. Results
with concurrency, instrumentation or different setup are kept separate from
quiet paired timing. Record rejected experiments and remaining limits explicitly.

## 1. Collector-owned scratch and compact retirement

The edge snapshot buffer now belongs to the explicit `Walk` context and reuses
capacity across objects. Callback admission ends before upper-owner tracing;
no registry borrow spans a message. This removes the separate per-object
`Memory.core_edges` allocation/ownership cycle without introducing a Rust-stack
edge container. Direct trace-owner hits no longer allocate a cycle-detection
set; parent walks still perform fresh owner/identity/parent queries.

Id's allocation snapshot keeps `(object, size, identity)` facts (24 bytes each)
instead of preconstructed ABI packets (72 bytes each). It constructs ordinary
message arguments when visiting; the snapshot stays owned heap data. GC and
ObjectModel retire dead metadata keys directly, removing temporary membership
sets. Id's required weak-key membership index is unchanged. Id's per-enumeration
reference snapshot is also unchanged; this slice does not claim all collector
scratch or native implementation state has been eliminated.

[Evidence](../measurements/runtime-simplification-gc-20261022.json). Two fresh
processes per variant run ABBA, with five collections per case. Setup and garbage
creation are excluded. CPU medians on this host:

| Collector workload | Before | After | Reduction |
| --- | ---: | ---: | ---: |
| 20,000 live objects | 27.33 ms | 25.27 ms | 7.6% |
| 20,000 live + 20,000 dead | 37.42 ms | 31.91 ms | 14.7% |
| 200,000 live objects | 132.25 ms | 124.87 ms | 5.6% |
| 200,000 live + 200,000 dead | 217.32 ms | 199.99 ms | 8.0% |

The smaller garbage baseline varied more; the larger cases give the more
conservative benefit estimate. The separate real-Termbox Logue runs produce
identical terminal bytes. They do **not** establish an application speedup:
the initial ABBA medians show full-map rendering 2.57 → 2.64 seconds (+2.9%),
ordinary redraw 36.08 → 36.53 ms (+1.2%), and explicit collection 509 → 505 ms
(-0.9%). A subsequent candidate/baseline pair shows a 0.95% full-map difference
and 5.7% lower explicit-GC time. Do not describe these mixed application samples
as a solved hitch or an across-program speedup.

Checks: Id/Collect/ObjectModel unit suites, deterministic context-buffer reuse,
and eight production native suites covering collection, activation, accounting,
object ownership/scopes and whole-heap tracing pass, as do all 34 headless Logue
checks. The matched PTY workloads
exercise actual game generation, floor transitions and 120 turns per process.

Implementation ordering was refined after source inspection: the old descriptor
does not contain call-source operands. A separate static-plan cache would add
machinery. Admit plans together with the new coarse-call contract instead; do the
smaller Range source change first.

## 2. Live-bound Range iteration and folding

The selected resumable Range `do:` and `inject:into:` drivers compute one fresh
`rangeStart + position` value, compare it exactly with the current `rangeEnd`,
and advance after the callback. They no longer send `size` or `at:`. Public
indexing and the separate indexed `detect:` implementation remain unchanged.
The synchronous bootstrap Range entrypoints are unchanged; this slice changes
the normal selected stackless control implementation.

This is the accepted library-contract change, not a guarded bypass of the former
indexing protocol. Bound getters and arithmetic remain current messages. A
candidate arithmetic Error terminates traversal rather than wrapping; callback
Errors remain ignored by `do:` and remain ordinary accumulator values in folds.
A count need not be representable to start traversal. Tests stop an enormous
range by changing its bound after the first callback, rather than attempting
billions of iterations. First/later element identities remain fresh.

[Evidence](../measurements/runtime-simplification-range-20261022.json). Paired
ABBA processes use identical native libraries, changing only the compiled
stackless-control artifact and rebuilt images. Median CPU:

| Workload | Before | After | Reduction |
| --- | ---: | ---: | ---: |
| 1,080 empty Range iterations | 593.07 ms | 328.14 ms | 44.7% |
| Empty nested 60×18 loops | 611.92 ms | 345.86 ms | 43.5% |
| Logue generation | 2,958.91 ms | 2,217.25 ms | 25.1% |
| Logue ordinary redraw | 40.43 ms | 34.68 ms | 14.2% |
| Logue descent + ascent, logic and rendering | 5,276.36 ms | 5,067.50 ms | 4.0% |
| Complete measured Logue phase sequence | 19.73 s | 16.58 s | 16.0% |

Allocation-only observations for flat/nested empty loops fall from
152,431/157,363 to 104,918/109,184 objects, approximately 31% fewer. These
include the prepared probe's entry/return. The first full-map sample falls
38%, but collection moves between phases; that is not a universal floor-change
speedup. Both variants still have two observed automatic collections during the
120-turn waiting phase. Steady game logic is effectively unchanged (+0.36% in
these samples). Terminal output is byte-identical.

Validation: 869 language assertions; 91 Range/search assertions; the actual
native task-control fixture's 177 assertions; all 34 game assertions and 24
independent game-state checkpoints. New tests cover indexing overrides, exact
signed limits, fresh identities, changing getters, extending the former final
endpoint, and a fold suspended across bound mutation and collection. The old
implementation fails the new traversal-contract checks before the source edit.

An initial direct CLI invocation of the task fixture lacked its native observer
and failed the same 12 assertions on both prefixes. It is not acceptance evidence;
the proper native fixture supplies that observer and passes. No test expectation
was weakened to accommodate that setup error.

## 3. Admitted call plans and coarse Execution calls

Newly emitted resumable code sends `Machine prepareCall:selector:` with a native
Word site index and an ordinary selector object. Method construction admits an
immutable source/copy plan alongside the existing canonical descriptor. Admission
checks version, extent, site count, source PC, arity and Oop-slot bounds, and excludes
the destination call window from every source and copy. Ordered trailing bindings
remain visible in their logical slots. The native implementation performs those
ordered copies, fills the call window and publishes the existing call/tail-call
action without allocation or a dependency callback. It uses the explicit frame
as storage; there is no transfer array on the Rust stack.

The coarse operation is an ordinary current Id send. It replaces the accepted
compiler-bookkeeping interception contract; it is not a speculative fast path
that rechecks Integer factories, namespace lookup and individual Machine methods
on every call. Actual source sends, return/continue operations, inspection,
suspension and selected code ownership retain their existing protocols. Legacy
constructors and optional preparation operations remain for old compiled code.
Compiler mask 253 selects the new convention; mask 125 retains the previous
emission for comparison and compatibility tests. The durable descriptor and image
formats are unchanged; materialization regenerates plans from portable IR.

The [authored reference](../../tests/fixtures/execution/prepared-call-reference.birnam)
reads the admitted plan through ordinary `callPlan:field:` inspection messages,
performs ordered copies with the existing slot protocol, and publishes the call
with `callSite:` or `tailCallSite:`. It does not invoke the saved native operation
for the selected transfer. Its helpers run on another explicit Machine. The test
fixture retains native preparation for the replacement's own ABI/bootstrap helper
calls, avoiding infinite metacircular entry. This is a working authored transfer
implementation, not a claim that the complete Execution owner or its ABI bridge
has already been rewritten. The proof deliberately collects between inspecting
the plan and copying the live values.

[Evidence](../measurements/runtime-simplification-execution-20261022.json).
Two fresh processes per variant, serial ABBA, compare the preceding Range
installation with a complete rebuilt installation. Median CPU:

| Workload | Before | After | Reduction |
| --- | ---: | ---: | ---: |
| Small block call | 0.011 ms | 0.009 ms | 15.1% |
| 1,080 empty Range iterations | 305.45 ms | 229.43 ms | 24.9% |
| Empty nested 60×18 loops | 319.70 ms | 238.72 ms | 25.3% |
| Logue generation | 1,982.69 ms | 1,247.89 ms | 37.1% |
| Logue ordinary redraw | 30.83 ms | 21.55 ms | 30.1% |
| Logue descent + ascent, logic and rendering | 4,425.32 ms | 2,892.37 ms | 34.6% |
| Complete measured Logue phase sequence | 14.63 s | 11.14 s | 23.9% |

The flat/nested loop allocation counts remain 104,918/109,184: the preceding
guarded optimization already omitted those boxes under default behavior. This
slice removes recurring guards, metadata decoding and fallback preparation
machinery from new code. The first full-map phase changes only 1.3%; GC-sensitive
individual phases vary substantially, so neither the 69% explicit-GC difference
nor a single rendering phase is a general collector speedup. Actual PTY output
is byte-identical across all four runs. These percentages compare this slice
with its immediate predecessor, not the original unoptimized installation.

Checks: 39 Execution/backend unit tests, including truncated/corrupt plan
rejection and binding cloning; all 869 language assertions; production compiler,
Machine, binding, invocation, observation and candidate-retirement suites;
177 task-control checks; 34 Logue assertions and 24 independent game-state
checkpoints; source-free mapped and rebuilt images with fresh target
replacement. The authored replacement runs with both the bootstrap library and
the selected stackless library, survives explicit collection, preserves result
identity, propagates entered failures without replay, and retires its code after
removal and explicit materialization-cache eviction. Native fixture setup and
temporary test-owned bindings were corrected during validation; no acceptance
expectation was weakened.
