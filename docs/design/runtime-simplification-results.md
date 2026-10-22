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
| Collector scratch and metadata retirement | Complete | Results below |
| Live-bound Range traversal | Planned | Pending |
| Admitted call plans, coarse compiled execution and authored implementation | Planned | Pending |
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
