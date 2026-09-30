# 74R — Compiler-generated small-callee recovery plans

Status: implemented and correctness-verified, 2026-09-12. Qualified timing
completed after checkpoint `fbe7a566`; alias benefits are supported, while small
regression and incremental cold-preparation acceptance remain open.

## Authorized batch

Implement Stage 1 of [74Q](74q-optimization-roadmap.md) after checkpoint
`d15a40ce`: extend 74P to bounded single-PC BlockBodies composed of entry
receiver/argument/capture values and local `Bind` aliases ending in Return.
No nested inlining, source allocation elimination, target cache, changed ABI,
canonical descriptor encoding or image format is included.

## Compiler and runtime contract

The compiler derives an immutable process-local plan from logical definitions
and their colored physical slots. It traces each alias to its original entry
value, records the physical return slot separately, and describes PC-zero
recovery with explicit entry-slot or nil sources for every physical destination.
Recovery restores baseline entry state, not a partially evaluated alias body.
The plan also records the exact baseline boxing order: each Bind's destination
index before its source index, then the return index. Runtime PC-token boxing
remains unchanged. At most 32 operations and 64 physical slots are admitted; typed refusal
reasons distinguish region shape, limits, observing operations and invalid layouts.

The runtime validates the canonical descriptor against the admitted compilation
and retains the plan with its weakly indexed registration. A successful hot call
uses the returned entry value without constructing a callee Frame. It preserves
all baseline index boxes, allocation order and reflective ReturnAction contents.
It checks the whole boxing budget before execution; a possible collection or an
observing/overridden Machine operation recovers first through production admission.
Existing targets remain resolved afresh and code acquisitions remain owned until
return or transfer. No guard miss may execute aliases twice or expose a missing
Frame. Unsupported operations and non-entry continuation shapes decline.

The compiler plan is ordinary data (slot sources, boxed indices and return
locations), not privileged language policy. The existing Id-described dense
recipe operation remains the floor interface; the new compiler producer feeds
the same one-shot checked recovery transfer.

## Acceptance and measurement

- Compare argument/capture aliases, chains and distinct colored slots against canonical
  compiled baseline code. Preserve exact Oop identity and boxing values/order.
- Override Machine reads, writes, return and lookup; verify callee observations,
  ordered effects and current-target replacement. Exercise GC before any alias
  boxing can collect, plus refusal for calls, literals and other observing work.
- Keep malformed/stale recovery, ownership, capture/restore and generation
  retirement regressions from 74P. Run debug/release and full production gates.
- Measure baseline, 74P-only admission and the new pass separately, with
  projection, alias, ineligible and persistent-guard-miss controls. Report cold
  preparation/warmup, steady time, allocations and actual hits/recoveries.
- Earlier exploratory measurements were permitted without qualification. The
  subsequent user request requires waiting for qualified host gates; keep those
  thresholds intact and retain the measured spread.
  Pause the previously authorized `rk` process only during timing and resume it
  with guaranteed cleanup. No speedup is assumed before measurement.

## Implemented mechanism

The pure compiler pass lives in
[`small_callee.rs`](../../../rust/birnam-codegen/src/small_callee.rs), and production
selection consumes it in
[`native_tier.rs`](../../../rust/birnam-runtime/src/runtime/native_tier.rs). Plans contain
no Oops or executable addresses and do not introduce a new recipe category at the
Id boundary: their entry-slot/nil sources populate the existing dense slot recipe.
The existing compiled Id recipe, stale-admission, capture/restore and retirement
regressions remain passing.

For a three-alias argument body, the entry source is slot 1 and the final return
location is slot 4. The plan retains that distinction. Entry recovery initializes
slots 0 and 1 from their original Oops and explicitly sets alias slots 2–4 to nil;
baseline execution then performs the bindings. Successful inlining returns the
original slot-1 Oop and publishes a ReturnAction naming slot 4. The canonical
boxing sequence is `[2, 1, 3, 2, 4, 3, 4]`, plus the existing driver PC token.

The current one-PC coloring keeps the step-safepoint union live, so these alias
slots are distinct. This batch does not claim reconstruction under a future,
more aggressive slot-reuse algorithm. Likewise, larger/multi-PC bodies, sends,
literals, global reads and closure construction remain outside this pass.

A GC-budget regression leaves enough allocation room for the old projection's
two boxes but not the alias sequence. The candidate must reconstruct before the
sequence begins; actual collection then observes the callee at depth two. Tests
also compare the complete canonical index-box sequence, change results through
custom reads, observe writes/return, replace lookup and the current target, and
prove an observing source send executes exactly once.

## Correctness evidence

`python3 scripts/small-callee.py --verify` completed with exit zero.
[The receipt](../../measurements/74r-correctness.json) contains source hashes,
frozen-authority checks and all command output:

- Two compiler tests and six production runtime tests pass in debug and release;
  the 17 runtime result rows agree across profiles.
- All 463 workspace tests pass, with zero failures and 13 intentionally ignored
  experiments. No compiler warnings were emitted by these runs.
- Native-prefix, named/block generation lifetime, image-v8, V2 control, two cold
  source archives and relocated-runtime gates pass.
- The installed source-independent
  [fixture](../../../scripts/fixtures/small-callee-recovery.birnam) checks alias and
  captured Cell identity, exactly-once effects, cache eviction and GC.
- Sixteen three-alias calls allocate the same 13,184 Id-heap bytes in baseline and
  optimized modes. All 256 observed body/caller index boxes agree in order/value;
  optimized mode records 16 hits. Two calls under read/write overrides produce
  exactly 12 callee read/write effects and reconstruct once before cached fallback.

The canonical public ABI, descriptors, baseline C emitter and image authorities
match checkpoint `d15a40ce`. The small-callee data is private, regenerated metadata.
The 74P runner now selects its original three tests explicitly, and its historical
control retains 74P eligibility even though the alias body is now optimizable.

## Measurements and limits

AC power became available, but both qualified attempts failed the host gate:
[the first](../../measurements/74r-host-rejected-load.json) had 22.61% median idle CPU
with another compiler above the per-process limit;
[the second](../../measurements/74r-host-rejected-load-repeat.json) had 4.45% median
idle CPU and other busy processes. No host threshold was weakened. The authorized
`rk` pause was confined to timing attempts and the process was resumed afterward.

[Seven exploratory pairs](../../measurements/74r-unqualified-timing.json) completed
with 10,000 calls per sample. All 133 samples passed their identity, allocation,
hit/recovery and ownership checks. The receipt includes CPU monitor output and
labels the timings unqualified. Negative reductions below mean a slowdown.

| Case | Median elapsed-time reduction versus current-binary 74P eligibility | Paired range |
| --- | ---: | ---: |
| Three aliases | 18.0% | 1.2% to 24.7% |
| One alias | 12.6% | -1.9% to 17.5% |
| Projection (same eligible behavior) | -0.4% | -11.2% to 20.4% |
| Ineligible literal control | 0.1% | -7.2% to 14.5% |
| Persistent return-guard miss | -6.3% | -11.4% to 16.4% |

Against disabled optimization, the three-alias median reduction is 21.4%.
Actual hits are 10,000 for eligible optimized samples and zero for controls;
persistent misses recover during warmup and record zero steady-state recoveries.
All compared allocation totals match, including 8,240,000 bytes for 10,000
three-alias calls. No source-object allocation saving is claimed.

The saved release test binary matching the 74P receipt hash supplies additional
projection/one-alias references. Its one-alias comparison has an 11.9% median
reduction but includes a 56.7% slowdown outlier. Cross-binary controls and the table
above show why these measurements cannot clear regressions. Those references
also contain the earlier test harness; the primary isolated comparison uses one
current binary and changes only eligibility.

The measured compiler-plan build averages approximately 64/76/129 ns for
projection/one-alias/three-alias plans. These are warmed repeated builds from
already lowered and colored IR, not cold compilation latency. The fixture's
roughly 0.73–0.79 ms preparation includes native baseline compilation and staging;
all current-binary modes pay that preparation before eligibility is selected.
Consequently it does not isolate incremental startup cost. Recipes retain 2/3/5
slot sources and 1/3/7 boxed indices for these cases. No application-wide timing
or retained-memory improvement is established.

## Qualified rerun after checkpoint

The implementation was committed as `fbe7a566` before timing, as requested.
[One further attempt](../../measurements/74r-host-rejected-load-third.json) passed
three pairs but stopped when the next preflight had 49.29% median idle against
the unchanged 50% requirement. Its partial samples are excluded from the results
below. After waiting, two complete matrices passed:

- [Seven pairs of 10,000 calls](../../measurements/74r-qualified-timing.json), with
  133 samples and all seven preflights passing. Each pair finished before a
  second CPU monitor sample, so the existing gate relied on immediate preflight.
- [Seven pairs of 100,000 calls](../../measurements/74r-qualified-long-timing.json),
  with 133 samples and all seven preflight and interval gates passing. This
  longer repeat checks the short run's control outliers and supplies actual
  during-measurement samples: 55 intervals, median idle per pair 59.20–63.22%,
  none below 30%. Preflight medians were 56.27–77.10%, on AC power.

Both runs passed the debug/release compiler and production tests, all sample
assertions and source-hash checks. Both used the exact same release binary and
179-file source manifest as the preceding verified implementation. The saved
74P reference binary matched its historical receipt hash. CPU logs are embedded
in the receipts. The authorized `rk` process was resumed after each attempt,
including the rejected attempt, and verified running after the final run.

All percentages below are median **paired elapsed-time reductions** against
current-binary 74P eligibility. Negative values mean slower. These compare the
same compiler/runtime binary with different admission, not old versus new cold
compilation.

| Case | 10,000-call median | 100,000-call median | 100,000-call paired range |
| --- | ---: | ---: | ---: |
| Three aliases | 20.47% | 18.93% | 13.70% to 26.27% |
| One alias | 11.85% | 9.09% | 3.50% to 15.85% |
| Projection (same eligible behavior) | -1.46% | -4.63% | -10.08% to 6.67% |
| Ineligible literal control | 3.20% | 0.26% | -7.97% to 7.66% |
| Persistent return-guard miss | -1.98% | -1.50% | -5.85% to 8.05% |

Each alias case improves in every pair in both runs. In the longer run, even the
smallest three-alias reduction exceeds the observed ineligible-control variation.
This supports a repeatable benefit for that fixture. The one-alias improvement is
consistent in direction, though its smaller pairs overlap control variation.
Against disabled optimization, longer-run median reductions are 19.43% for three
aliases and 12.57% for one alias. Against the actual saved 74P binary's enabled
mode, one alias improves 9.61% at median (4.32–18.91% across pairs).

Small regressions are not cleared. Persistent fallback is 1.50% slower at median
versus prior eligibility, or 4.07% versus disabled optimization, with mixed-sign
pairs. The unchanged projection admission also moves by several percent; its
old-binary enabled comparison is 4.23% slower at median. Those results and the
controls leave order/host variation and small implementation costs unresolved;
they do not establish a particular regression. No application-wide speedup is
claimed.

Allocations agree exactly across every compared mode and reference: 100,000
calls allocate 72,800,000 bytes for projection, 76,000,000 for one alias,
82,400,000 for three aliases/fallback and 79,200,000 for the ineligible control.
Eligible optimized samples record 100,000 hits; controls have none. Persistent
misses recover during warmup, with zero steady-state recoveries. Identity,
active-pin and acquired-entry checks pass throughout.

Longer-run median plan-build costs are 43/55/95 ns for projection/one/three
aliases, with the same 2/3/5 recovery slots and 1/3/7 boxed indices. These remain
warmed builds on lowered/colored IR. Median preparation is approximately
0.53–0.56 ms including native baseline compilation, paid by every current mode.
Warmup medians over 80 calls span approximately 0.21–0.27 ms. Neither these
numbers nor the cross-binary steady-state references isolate incremental cold
preparation or retained-memory cost.

## Disposition and next step

The first implementation batch (`birnam-svi`) and qualified rerun/assessment
(`birnam-sc7`) are complete. The host qualification blocker is resolved, and the
alias fixture benefit is supported. `birnam-bew` retains the remaining fallback,
control-variation and incremental preparation questions before broader
optimization. This is not blanket no-regression acceptance or promotion of
Stage 2.

The earlier implementation/roadmap checkpoint is `d15a40ce`; this batch's
implementation checkpoint is `fbe7a566`. The rerun changes receipts, documentation
and tracker state only. No push or user installation was requested.
