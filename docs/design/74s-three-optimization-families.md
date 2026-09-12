# 74S — Dispatch specialization, redundant work and native optimization

Status: implementation in progress. Authorized goal: implement the next three
optimization families after the first inlining batch and measure each separately.
The starting checkpoint is `d06f7558` (74R). This document records the concrete
contracts before implementation; passing experiments do not imply whole-program
performance acceptance.

## Sequence and acceptance

1. Specialized dispatch: retain the resolved canonical method for a repeated
   receiver-vtable/selector pair, avoiding repeated dictionary/delegation walks.
   Direct dictionary hits bypass caching: the initial qualified run found a
   direct/churn regression, so cache admission now targets inherited lookup.
   A bounded runtime cache admits one receiver shape per selector bucket. This
   is shared dispatch specialization, not yet per-source-call-site profiling.
   Guard the current canonical lookup operation, global dictionary/delegation
   mutation epoch, receiver vtable and selector. Do not cache unknown lookup,
   exhausted epochs or unbound results. Preserve normal target classification,
   acquisitions and invocation. Discard address-only entries before GC/retirement
   tracing and prohibit refill during collection, so cache metadata neither roots
   objects nor keeps code alive. Mutation and collector hooks remain ordinary
   observation boundaries. Test inherited lookup, replacement/removal, parent
   changes, custom and resumable lookup, native reentry, GC and generation lifetime.
2. Redundant reads/computations: derive bounded observation-free regions from
   canonical lowering. Reuse repeated physical Frame reads only while writes,
   allocation/GC, lookup mutation, reentry and Machine overrides cannot invalidate
   the value. Retain every baseline box and every live recovery-visible value.
   Profile the proposed region before choosing the concrete producer/consumer;
   isolated 74H read reuse did not demonstrate a win.
3. Conventional native optimization: eliminate redundant native copies and dead
   implementation temporaries within the derived representation, preserving all
   semantic operations, allocation order and baseline recovery state. Choose the
   concrete pass from the preceding region profile. No source allocation or
   numeric identity change is part of this family.

Each family requires independent on/off timings against the preceding family,
combined results, controls and mutation/churn coverage. Save source/binary hashes,
actual hits/misses/recoveries, preparation, metadata size and allocation totals.
Run debug/release correctness and full production verification after each family.
Use unchanged host gates for qualified timing; pause only the authorized verified
`rk` process and always resume it. Keep 74R fallback/control and preparation costs
visible (`birnam-bew`). Preserve public ABI, descriptors, image format, full Id
malleability, stackless control and code lifetime. Later family contracts will be
made concrete before their code changes, with evidence recorded here.

## Dispatch iteration evidence

The initial implementation passed 147 runtime tests and full production checks.
Its first complete eight-pair qualified timing matrix showed a strong inherited
lookup benefit but repeatable direct and churn regressions. That version is not
accepted. Local diagnostic receipts are under `.cargo-target/74s/dispatch-third*`;
final retained receipts will identify the corrected implementation. The correction
keeps direct dictionary hits outside the cache. Added inherited-churn measurement
ensures frequent invalidation of the actual admitted path remains visible.

## Next-family mechanism under evaluation

The next producer will consider whole existing PCs containing only entry values
and Bind operations, ending in Return or Continue. Use a bound of 64 operations
and 128 physical slots; long alias bodies exceed 74R's one-callee limit and expose
repeated reads inside actual framed execution. Unknown sends, literals, captures
constructed in the region and other observing operations are excluded. Entry
captures already held in the Frame remain ordinary Oops.

The proposed consumer retains the existing Frame and descriptor throughout.
After the normal PC-token allocation, prove canonical Machine read/write/lookup
semantics and enough GC budget for the entire region's baseline index boxes.
Allocate those boxes in their exact canonical order, then run a bounded private
native kernel over validated current Frame words, with no allocation or callback
inside it. Publish the unchanged ordinary Return/Continue operation only after
all final Frame values are present. A guard miss runs the original PC before any
body effect. This needs no new absent-Frame recipe, ABI field or artifact encoding.
Moving housekeeping boxes before writes is permitted only while the complete
allocation budget proves no collection/observer can intervene; otherwise decline.

Family two will use a compiler effect/availability analysis to cache repeated
physical reads and forward known writes within that kernel. Family three will
separately propagate entry-source identities through native copies and remove
intermediate native temporaries. All required final Frame writes remain. The
native emitter will keep individual switches so family two, family three and
combined execution can be measured against identical canonical input; neither
family may claim a gain from omitted Id allocations. These concrete choices
still require static/runtime profiling, counterexamples and independent timings
before acceptance.


A standalone native-kernel probe has now validated the proposed family-two
producer on a 100-alias body spanning two canonical PCs and 64 physical slots.
Both emitted variants preserve every final word. Reusing one unchanged input
reduces generated loads from 62/38 to 1/1 across the PCs, and C source size from
2,675/1,642 bytes to 924/587 bytes. A chained-alias control correctly retains
62/38 loads in this pass; removing those dependencies belongs to the separately
measured propagation pass. This is compiler/kernel evidence only, not production
Frame admission or timing acceptance. Draft source and probe output are currently
in `/tmp/birnam-74s-frame-region.rs` and `/tmp/birnam-74s-region-probe.log`.


## Family one: retained implementation and measurements

The corrected production cache is in `runtime/dispatch_cache.rs`. It checks the
current direct dictionary first, carrying the delegation edge forward on a miss
so that the same dictionary is not searched twice. Only inherited answers enter
the 64-slot cache. Metadata is a fixed 2,048 bytes per Runtime and holds neither
roots nor generation leases. No Id allocations are removed or introduced.

[Correctness](../measurements/74s-dispatch-correctness.json) passes debug/release
mutation, real GC, native reentry, custom lookup and late-registration checks,
plus the full workspace/installed-prefix/cold-archive/relocation gate. The cache
must reject an unpublished registration even if its carrier was previously cached
as synchronous; normal classification observes that state without an epoch move.

[The initial qualified version](../measurements/74s-dispatch-initial-regression.json)
regressed direct calls and direct rebinding and was corrected before continuing.
[The corrected dispatch matrix](../measurements/74s-dispatch-qualified-timing.json)
contains eight pairs of one million sends, with all preflight/interval gates
passing for the measured pairs. Busy preflights were retried before measurement;
`rk` was resumed during waits and after timing. No thresholds changed.

| Dispatch fixture | Median paired time reduction | Paired range |
| --- | ---: | ---: |
| Twelve-level inherited lookup | 79.30% | 78.65% to 79.62% |
| Direct lookup | 2.65% | -0.11% to 5.59% |
| Custom-lookup control | 1.60% | -0.10% to 5.47% |
| Rebinding on every direct send | 1.70% | 1.01% to 4.85% |
| Rebinding on every inherited send | -0.23% | -0.60% to 1.43% |

[Existing native-workload controls](../measurements/74s-dispatch-native-controls.json)
then ran eight pairs of 100,000 calls with only dispatch caching toggled; the 74R
inline consumer remained enabled in both modes. Median paired time reductions
are projection 1.16%, one alias -0.07%, three aliases -2.97%, ineligible control
-0.08% and persistent fallback -3.10%. All allocations and expected inline
hits/recoveries agree. Mixed-sign variation is substantial (the projection control
ranges from -15.1% to 12.1%). These do not clear small costs in native workloads;
keep the alias/fallback question visible in subsequent isolated and combined runs.
The dispatch fixture benefit is not a whole-program estimate.

The dispatch matrix predates only two measurement-harness changes: adding an
ignored-test dispatch toggle to the 74R fixture and adding the native-controls
matrix to the runner. All production Rust source hashes match the subsequent
correctness/control receipts. Their separate source and binary hashes are retained;
these are not falsely described as one identical test binary. Initial/failed
iterations remain local under `.cargo-target/74s` unless explicitly linked above.

Family two is being implemented in the isolated `/tmp/birnam-74s-next` checkout,
which began with exactly this dispatch implementation. This protects the main
checkout's timing sources while implementation proceeds. Family three remains
pending. `birnam-kwr` owns the complete three-family goal; `birnam-bew` remains open
for the prior and continuing small-cost acceptance questions.
