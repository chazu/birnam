# 74S — Dispatch specialization, redundant work and native optimization

Status: all three families implemented; combined performance acceptance in progress.
Authorized goal: implement the next three
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

## Family two: guarded Frame regions

The compiler derives whole existing PCs containing entry values and Bind
operations, ending in Return, Continue, Call or TailCall. Admission is bounded
by 64 operations, 128 physical slots and the first 16 PCs per candidate. Call
staging remains an exact ordered copy of receiver, selector and arguments into
the descriptor's existing call window. Ordinary target resolution follows at the
publication boundary. Literals, other observing body operations, absent selectors
and unsupported layouts decline before body effects.

The consumer retains the existing Frame and descriptor. After the normal PC-token
allocation it proves canonical Machine read/write/lookup semantics and enough GC
budget for every baseline body index box. It allocates those boxes in canonical
order, then runs a private native kernel over validated Frame words. The kernel
has no allocation or callback. Every final Frame word exists before ordinary
Return/Continue/Call/TailCall publication. Guard failure runs the original PC.
Moving boxes before writes is admitted only when no collection or observer can
intervene. Source allocations, public ABI, descriptors and image formats are
unchanged. Candidate kernels share one owned native unit with their fallback PCs;
an executing selection retains that unit through publication and native reentry.

The retained optimization removes repeated Machine dispatch, index decoding and
Frame validation from the region. The additional physical-read cache is an
experimental test mode: qualified measurement did not justify enabling it.
Production uses the simpler kernel with ordinary loads and every baseline store.
Family three selectively propagates dependent copies and groups native stores,
as described below; its final timings do not establish an independent steady
execution gain.

Correctness probes compare every final Frame word, exact box order and bytes,
argument and capture identity, Continue/Return boundaries, Call/TailCall staging,
late target replacement, missing-selector allocation, Machine overrides, GC
admission and candidate eviction. The older PC-specialized tests explicitly
select their original path because their event injection is in that path's C
shims; region boundary probes exercise the new path independently. The installed
fixture adds source-independent materialization, hot calls, eviction and GC.

The [initial six-pair qualified matrix](../measurements/74s-regions-initial-timing.json)
compared regions off, plain regions and regions with read reuse at 10,000 calls
per sample. Plain regions reduced median paired time by 54.07% for repeated-source
aliases, 53.64% for chained aliases and 25.54% for repeated call arguments. The
fallback median was essentially unchanged. Extra read reuse provided no median
benefit and regressed the repeated-source fixture relative to plain regions.
These are focused fixture measurements, not whole-program speedup estimates.
This receipt predates the production-default choice, refined metadata accounting,
installed fixture and explicit old-path test selection; final reruns retain their
own source and binary hashes.

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

Family two was developed in an isolated checkout and is now integrated for full
verification and a committed-source timing rerun.
[Integrated correctness](../measurements/74s-regions-correctness.json) passes
focused debug/release checks and full production verification, including the
installed native-region fixture, clean archives and relocation. Family three
was still pending at that checkpoint; its implementation and receipts follow. `birnam-kwr` owns the complete three-family goal; `birnam-bew` remains open
for the prior and continuing small-cost acceptance questions.


## Committed-source qualified rerun

The user requested a checkpoint followed by waiting for a quiet host and repeating
qualified timings. Dispatch was committed as `cb425272`; integrated Frame regions
and full correctness were committed as `40aa896f` before this rerun. Full
verification passed 469 workspace tests (15 ignored), focused debug/release checks,
installed-prefix lifecycle checks, the new region fixture, clean archive builds
and relocation.

All three eight-pair matrices passed on AC power using unchanged host thresholds.
Five busy preflights were waited out before starting their pairs (two in dispatch,
three in native controls); no measured interval failed. The authorized `rk` process
was resumed during waits and after each matrix, with final running-state verification.
All 280 samples use the same release binary, SHA-256
`81a94be0ab8c9614a21ab5abef7ad2484896c4907e837ce2856ec4b21ad732dd`.
Source hashes match the committed implementation and integrated correctness receipt.

| Fixture | Median paired time reduction | Receipt |
| --- | ---: | --- |
| Inherited lookup, 12 levels | 79.47% | [Dispatch](../measurements/74s-dispatch-rerun.json) |
| Direct lookup | 2.65% | Dispatch |
| Direct rebinding every send | 1.52% | Dispatch |
| Inherited rebinding every send | 0.31% | Dispatch |
| Custom lookup | 0.35% | Dispatch |
| Repeated-source copies | 57.36% | [Regions](../measurements/74s-regions-rerun.json) |
| Chained copies | 54.82% | Regions |
| Repeated call arguments | 23.94% | Regions |
| Region-ineligible control | 0.48% | Regions |
| Persistent region fallback | -2.50% | Regions |

Dispatch uses one million sends per sample. Regions use 10,000 calls per sample,
with dispatch fixed on and only region preparation/execution switched off/plain/
read-reuse. The retained plain-region benefits are positive in every pair for
all three eligible fixtures. Extra read reuse reduces repeated-copy performance
by a further 4.64% at the median relative to plain regions; it remains disabled in
production. Allocation totals and expected execution counters agree in every mode.

[Native workload controls](../measurements/74s-native-controls-rerun.json) use
100,000 calls per sample with only dispatch caching toggled and the 74R consumer
fixed on. Median reductions are projection 0.66%, one alias 7.55%, three aliases
-1.41%, ineligible control 4.45% and persistent fallback -3.20%. Every control has
mixed-sign pairs. Persistent dispatch fallback's roughly 3% median cost repeats
the earlier run; this remains a possible regression, not a cleared control.
The region fallback also varies from -14.06% to +7.44%, with a -2.50% median.
`birnam-bew` remains open for these small costs. These fixture gains do not establish
whole-program profitability. The third family was still pending at that checkpoint.

## Family three implementation contract

Propagate initial physical-slot identities through the exact ordered copy and
selector-store sequence. Emit only each destination's final value, dropping
overwritten stores and identity stores. Preload every distinct surviving input
before any store, so slot reuse, cycles and selector overwrites cannot clobber an
input. Coalesce contiguous stores of the same value into a bounded private native
fill helper. That helper only writes existing words: no allocation, callbacks,
new floor entry, registration or image field. Retain every canonical index box
and every final Frame word at the existing observation boundary.

Compare propagated kernels against plain guarded regions independently, then
compare the combined consumer against regions off. Differential tests must cover
physical-slot cycles, overwritten temporaries, selector insertion and every word,
as well as the existing production boundary, mutation, identity and GC probes.
Keep the pass only where its measurements support profitability. The unresolved
fallback costs and preparation costs remain acceptance work for the whole batch.


The third-family implementation and [correctness receipt](../measurements/74s-copies-correctness.json)
now pass focused debug/release and full production verification. The native
kernel differential probe covers 256 ordered physical-copy sequences including
selector insertion, slot reuse, cycles, dead stores and canaries around storage.
The existing production boundary probes additionally run the propagated mode.
The subsequent matrices below measure profitability independently against plain regions.
The production application runner builds original 74R and current floors, then
composes two explicitly labelled experimental prefixes with an identical binary
closure except for `libbirnam_id.dylib`. It measures eval, ordinary collections,
portable collections and cold materialization separately from kernel fixtures.


### Third-family profitability iteration

The [first qualified eight-pair matrix](../measurements/74s-copies-initial-timing.json)
found median plain-to-propagated reductions of +1.90% for chained copies, -3.37%
for repeated-input copies and -0.19% for repeated arguments. The cold compiler
cost fell, but that does not justify regressing the repeated-input workload.
The revised policy admits propagation only when an ordered copy reads a previously
written slot or overwrites a prior destination. Straight repeated-input copies
and ordinary argument staging retain the plain region kernel. The general pass
remains differential-tested for all copy shapes. Retained-policy measurements
and the production application comparison follow separately; these first results
are not presented as acceptance of the unrestricted variant.

[Selective-policy correctness](../measurements/74s-copies-selective-correctness.json)
passes focused debug/release checks and full production verification. Production
application prefixes have built successfully and their binary comparison confirms
that only the floor differs.


### Retained-policy timing and current acceptance state

The [selective-policy eight-pair matrix](../measurements/74s-copies-selective-timing.json)
passed at 30,000 calls per sample. Against plain regions, chained-copy execution
improved 0.90% at the paired median; the earlier 10,000-call unrestricted pass
measured 1.90% for that same eligible shape. This is a small incremental signal,
not a robust whole-program speedup. Combined region/propagation reductions against
regions off are 54.12% for chains, 53.54% for repeated inputs and 23.72% for arguments.
Persistent fallback is essentially unchanged in the incremental comparison
(-0.14% median time reduction). Plain and propagated modes now emit the same plain
kernel for repeated-input and argument controls; their measured variation remains
visible rather than being attributed to the propagation transformation.

All three requested families are implemented. Remaining acceptance work is the
combined production workload matrix and investigation of any repeatable regression
it exposes. [Both production prefixes pass workload correctness](../measurements/74s-applications-smoke.json).
The [first production timing attempt](../measurements/74s-applications-rejected-load.json)
was rejected in pair four: median idle CPU fell to 31.9% with five of eleven
intervals below 30%. It is not qualified timing evidence. The complete matrix is
being rerun after waiting for the host to settle. The authorized rk process was
resumed after rejection. No host thresholds were changed.


### Control diagnosis and production fallback coverage

The repeated-input control's variation was investigated rather than silently
accepted. [Three structural diagnostic repetitions](../measurements/74s-control-diagnostic.json)
confirm identical emitted C hashes for plain and selected-propagation modes on
repeated-input and persistent-fallback bodies. Inherited-cache hits and misses are
both zero for these fixtures; chained copies correctly produce different C.
The admission scan now uses a bounded stack bitmap rather than allocating a
heap vector while preparing a candidate. The diagnostic timings are unqualified
and are not used as performance evidence.

The [second application attempt](../measurements/74s-applications-rejected-load-retry.json)
also failed its interval gate (49.46% median idle in pair three). The revised
application runner gates each independent paired workload immediately before it
runs and monitors that pair. It may wait before another pair; an interval failure
still rejects the complete matrix, with the same CPU and power thresholds.
The next matrix also includes the existing ordinary-lookup override fixture,
which verifies late method replacement, forced fallback, eviction and GC through
the installed production floor. Both original and current prefixes are rebuilt
for the final source manifest; no stale build receipt is reused.

[Final correctness](../measurements/74s-final-correctness.json) passes all focused
debug/release and full production checks after removing admission heap work.
[The installed fallback fixture](../measurements/74s-final-fallback-smoke.json)
passes through both floors, including mutation, forced lookup fallback, eviction
and GC.


### Final focused matrix

The [final twelve-pair matrix](../measurements/74s-copies-final-timing.json), at
30,000 calls per sample, passes every host gate. The six mode orderings each occur
twice. Chained-copy propagation has a -0.16% median incremental time reduction,
so the earlier small positive medians do not establish a reliable execution gain.
Repeated-input and argument controls retain identical emitted C in both modes;
their median reductions are +0.31% and -0.78%. Incremental persistent fallback is
+0.61%. These controls do not show the earlier repeated-input slowdown after the
admission allocation was removed. The chained candidate's C source is reduced
from 39,231 to 35,187 bytes; compiler-cost measurements remain in the receipt.
The pass is bounded and reduces generated native work, but no independent steady
execution-speed gain is claimed for family three.

Combined region/propagation reductions versus regions off are 54.76% for chains,
54.66% for repeated inputs and 22.37% for arguments. This confirms the substantial
family-two fixture gain while keeping the small third-family result separate.
Two additional application attempts were rejected by interval load
([first](../measurements/74s-applications-final-rejected.json),
[second](../measurements/74s-applications-final-rejected-retry.json)). The next
attempt holds only the authorized rk process paused through a sustained cooldown
and the full matrix, with a separate pause owner responsible for resuming it in
all exit paths. Application acceptance remains pending that complete matrix.


The [sustained-cooldown attempt](../measurements/74s-applications-settled-rejected.json)
passed six consecutive preflights after 34 checks, then the production matrix was
rejected during the third group's chain workload: the monitored interval had
38.97% idle CPU. It remains unqualified. The externally paused rk PID exited
during this attempt; subsequent process inspection verified current rk processes
running with none left stopped. This is recorded as process exit, not falsely as
resumption of the old PID. Combined application performance acceptance is still
incomplete. There are no active measurement jobs from this attempt, and no code
changes are awaiting correctness verification.
