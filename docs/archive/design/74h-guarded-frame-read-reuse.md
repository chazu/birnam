# 74H — Guarded reuse of Frame reads

Status: completed 2026-09-10; correctness passes in debug/release;
the completed measurement is inconclusive and does not support integration.
Full repository verification passes. Authorized after committing 74G
as `a10d1b55`. The protocol below preceded implementation and measurement.

## Problem and proposed solution

74G proves generated recovery and executable ownership. The next experiment
removes repeated ordinary Machine reads within one compiler basic block.
The comparator is 74G PC specialization with unchanged block bodies, so any
incremental benefit must pay for the new guard. Whole-method baseline emission
is a separate semantic reference.

The compiler identifies repeated reads of a physical slot with no intervening
compiler write to that slot. Eligible slots receive a native-local cache that
exists for one StepMethod invocation. The first read remains an ordinary send.
A later read may reuse its answer; all original index boxes, selector interning,
store operations, allocation order and action publication remain in place.
Ordinary emission and the existing descriptor bytes remain unchanged.

## Legality boundary

Dictionary epochs alone are insufficient. Index allocation can collect; a
native mark/finalization callback can change Frame contents without changing
method dictionaries. A test-only monotonically increasing GC epoch advances
before every mark traversal, including retirement reachability traversals.
It saturates, and its terminal value never admits reuse.

The read guard executes after the original read operands have been evaluated.
It checks canonical Machine identity, canonical read/write methods and lookup,
the dictionary epoch and the GC epoch. The first ordinary read is remembered
only if those conditions hold before its invocation. A mismatch permanently
disables that local cache for the remainder of this basic block; subsequent
reads use the ordinary send. Compiler stores invalidate a cache for their
destination slot. No cache survives an action return or crosses a language
send. The supported body consists of existing literal/global/binding operations
and call staging; nested block construction is excluded from this first slice.

Under the guard, the only calls between reads are native Integer allocation,
non-reentrant interning/import and canonical Machine slot operations. Collection
invalidates cached Oops before any reuse, so the cache introduces no extra root.
Every miss sends once with the already evaluated original operands. Driver
recovery uses 74G's exact descriptor and native-unit leases at the next boundary.

The helper is supplied through artifact-local test initialization, as a function
pointer. It is not a new floor export, IdApi field, runtime ABI version or image
format. Production admission is conditional follow-up work, not something this
test-local transport establishes.

## Frozen checks

- Compare whole-method baseline, PC-only control and read-reuse candidate.
- Check answers, argument identity/order, once-only effects and same-Frame PCs.
- Exercise no-repeat bodies and sparse/dense repeated-read bodies.
- Invalidate before entry, between repeated reads and on every measured call.
- Replace native Machine read/write methods and global lookup, including an
  override introduced after a read has already been cached.
- Force precise GC between reads; run a real root callback that changes the
  cached Frame slot without changing the dictionary epoch. Both old staged and
  newly read values must survive and appear in the expected argument order.
- Exercise collection with no mutation, epoch saturation, ordinary recovery,
  native retirement, and repeated runs with no remaining Frame/native leases.
- Retain 74F/74G regressions, ordinary compiler tests and full repository verify.

## Measurement and decision

Compile once before measuring, run release Rust with native `cc -O2`, and turn
off observation counters in timed samples. Prove activation separately. Use
fresh-process paired samples with alternating control/candidate order, warmup,
identical inputs, and the existing host preflight and interval-monitor gates.
Retain raw samples and compilation/native-size costs. Include 0, 2, 4 and 16
repeated arguments and ordinary, per-call invalidation and persistent fallback
conditions. Choose iteration counts before looking at comparative results.

Use seven pairs. A claimed improvement must exceed both variants' observed
within-variant sample range; report medians and ranges without a percentage
threshold. A regression beyond that same spread in any admitted shape or
invalidation/fallback condition prevents promotion of this implementation.
An inconclusive comparison does not count as a measured benefit.
The zero-repeat variant emits the same complete C as the control. An
above-spread difference there invalidates attribution and prevents promotion.

If correctness and measurement justify it, continue with an opt-in normal
source/compiler/runtime integration using the current ABI. Otherwise retain
the experiment and its evidence, explain the limiting guard cost, and do not
silently widen assumptions or tune eligibility after seeing the results.

## Implementation and correctness

The `dev-frame-read-reuse` compiler feature adds an explicit emitter entry
point. It scans physical-slot reads/writes in emission order and emits one
native-local cache for each reusable slot. It refuses combination with the
74B/74D guarded-slot mode. The fixture compares three entries derived from one
canonical CodeProgram and descriptor: whole-method baseline, PC-only control,
and PC-specialized read reuse. All baseline/control paths remain ordinary.

The helper uses a three-state cache: empty, remembered, disabled. It records
the dictionary/GC epochs before the first ordinary read, reuses only on a
matching guard, and performs the original send on every miss. A compiler
store clears remembered availability without re-enabling a disabled cache.
The GC epoch is private, test-only Runtime state; it advances before
`run_mark_phase` invokes any foreign provider/finalizer and saturates at
`u64::MAX`. No descriptor/registration structure or floor ABI changes.

The helper pointer is installed through the candidate library's local
`__birnam_read_cache_init` function. It uses 74G's actual native owners,
registration-generation lease, per-entry pin and unload checks. This fixture
initializer establishes no production loader/admission mechanism.

The program doubles input 7, stages the resulting Oop repeatedly as arguments
to a sum, and returns that sum through a third PC. There are 0, 2, 4 or 16
repeated arguments. In ordinary execution the three forms agree on every
answer, staged alias, effect order and Frame handle. The candidate eliminates
1, 3 or 15 reads in the repeated-argument cases; the dense case goes from
22 reads to 7. The zero-repeat body and complete emitted C equal the control.

The matrix has **36 observations per profile**: twelve scenarios/shapes, each
executed as baseline, control and candidate. Five additional groups execute
64 calls each, including repeated invalidation with re-admission. The 356
calls per profile finish with no live Frames, acquisitions or registrations,
and **82 native libraries actually unload**. Debug and release observations
agree. The scenarios include read/write/lookup overrides installed after the
first cached read, pre-entry invalidation, between-read invalidation, precise
GC and GC-epoch saturation through two real collections.

The decisive GC test runs a real native root provider during collection.
It changes the cached slot from 14 to 7 without changing the dictionary epoch.
The first already-staged argument remains 14 and the other fifteen become 7;
all three variants answer **119**, rather than the ordinary case's 224.
A second precise collection at the sum callback uses an empty native range,
collects a garbage control, and preserves the compiler-staged arguments.
This demonstrates why dictionary invalidation alone cannot authorize reuse.

## Measurement result

Reproduce correctness and the frozen measurement with:

```sh
mise run read-reuse-experiment
python3 scripts/frame-read-reuse.py --measure --output docs/measurements/74h-frame-read-reuse.json
```

The [completed receipt](../../measurements/74h-frame-read-reuse.json) retains
source/compatibility hashes, compiler versions, prepared native-source/binary
hashes, compiler costs, all correctness observations, host gates and raw samples.
Two earlier attempts were rejected by the host preflight when background work
became busy. Their partial data remain separate in
[attempt one](../../measurements/74h-frame-read-reuse-host-rejected.json) and
[attempt two](../../measurements/74h-frame-read-reuse-host-rejected-2.json), and are
excluded from the completed comparison. The runner subsequently used the
existing host gate's bounded `--wait` policy before each pair. No eligibility,
workload, iteration count, sample count or comparison threshold changed.

All seven completed pairs pass their host gates. Each pair finishes before
the first interval sample, so the existing monitor policy uses the immediate
preflight for these short probes. Each sample has 64 warmup calls and 4,096
measured calls. Read/effect observation counters are compiled out; common
driver-selection counts remain, and invalidation injection is identical in
control and candidate. Fresh processes alternate which variant runs first.

Times below are median nanoseconds per complete three-PC call. Positive
differences favor reuse. Spread is the larger of the two within-variant
seven-sample ranges, as specified before measuring.

| Repeated arguments | Condition | Control ns | Reuse ns | Difference ns | Spread ns |
| --- | --- | ---: | ---: | ---: | ---: |
| 0 | Ordinary | 4,550 | 4,432 | +118 | 403 |
| 0 | Invalidation injection control | 4,452 | 4,339 | +113 | 1,355 |
| 0 | Persistent baseline fallback | 4,263 | 4,039 | +223 | 528 |
| 2 | Ordinary | 4,836 | 4,642 | +194 | 507 |
| 2 | Invalidate each call | 5,025 | 4,930 | +96 | 685 |
| 2 | Persistent baseline fallback | 4,663 | 4,688 | -25 | 411 |
| 4 | Ordinary | 5,186 | 5,062 | +124 | 442 |
| 4 | Invalidate each call | 5,585 | 5,342 | +242 | 431 |
| 4 | Persistent baseline fallback | 5,176 | 5,171 | +5 | 563 |
| 16 | Ordinary | 8,670 | 8,196 | +474 | 814 |
| 16 | Invalidate each call | 9,137 | 9,269 | -133 | 846 |
| 16 | Persistent baseline fallback | 8,416 | 8,728 | -312 | 1,451 |

With zero repeated arguments there is no eligible source read at which to
inject a mid-entry mutation; that row is an additional unchanged-body control.
The 2/4/16 invalidation rows do mutate each call and prove two selected entries
followed by one baseline entry per call. Persistent fallback proves zero
selected entries in both variants. The ordinary rows retain all three.

**Every comparison is inconclusive under the frozen rule.** The 2/4/16
ordinary medians nominally improve 4.0%, 2.4% and 5.5%, respectively, but none
exceeds the observed spread. No above-spread regression is established either.
The identical-source zero-repeat control also varies. These measurements do
not establish a useful speedup or a regression for this implementation.

For 16 repeated arguments, control/reuse sources occupy 11,007/11,692 bytes;
native files occupy 33,728/33,832 bytes. Prepared native compilation takes
roughly 35–44 ms across the twelve units. These are fixture costs, including
the local adapter. All original slot-index boxes and stores remain: read
elimination reduces only part of the work. The experiment does not attribute
the remaining time among allocation, guards and other dispatch.

## Decision and validation

Keep this implementation experimental. The conditional normal
source/compiler/runtime integration is **not taken**, because no measured
benefit cleared the predeclared criterion. The existing ABI/image contract
remains unchanged; no production optimizer or speedup is claimed.

The ordinary compiler's 61 tests pass, and 74F/74G retain identical debug and
release observations. `mise run verify` passes the workspace, native-prefix,
cold-archive and relocated-runtime gates; two independent source archives
produce identical source/output closures. A future experiment should target a larger source of work
or explicitly predeclare a measurement with longer samples; these data do
not justify silently broadening cache lifetimes or removing guards.
