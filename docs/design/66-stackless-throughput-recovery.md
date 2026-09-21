# 66 — Stackless Throughput Recovery

> **Status:** Implementation, allocation investigation, and development
> validation complete, 2026-09-05. This is not release qualification; the
> strict comparator findings are recorded in section 8.
> Research task: `birnam-v0j`. Publication closeout is committed as `a1a5a69`.
> The measurements in sections 2–3 describe the pre-optimization runtime.
> Section 7 records implementation evidence; Beads owns acceptance status.

## 1. Direction

Start with **indexed weak Symbol lookup**, then measure its effect on complete
workloads before selecting the next optimization. Current profiling identifies
linear symbol interning as the largest individual leaf cost in an empty
stackless loop. This is a small runtime-internal change with useful existing
identity and reclamation tests, and it preserves the language execution model.

The next investigation is allocation in generated control code, especially boxed
Machine operands. It needs a separate semantic analysis: replacing visible
Machine messages with native slot access or reusing observable boxed Integers
would not be a routine implementation optimization under today's contracts.

## 2. Evidence and its limits

### Historical release evidence

[Design 60HB](60hb-v0.14-performance-tradeoffs.md) records the accepted v0.14
tradeoff against the retained v0.11 Rust snapshot: `loops.to-do` 49,574 ns/op,
`query.single-clause` 701.7 ms/op, and `docgen.full` 54.36 s. Those were roughly
237×, 456×, and 30× the prior measurements. They are historical measurements,
not a current-checkout comparison. v0.15 returns to ordinary performance policy;
the v0.14 exception is not permission to relax future thresholds.

### Current quick micro run

After publication acceptance completed, a fresh release-runtime prefix ran the
unchanged micro suite with `BENCH_QUICK=1` (two samples per case). All cases
completed without a benchmark failure.

| Case | Net ns/op | Raw ns/op |
| --- | ---: | ---: |
| `loops.to-do` | 53,425 | 53,425 |
| `loops.whileTrue` | 49,242 | 49,242 |
| `loops.timesRepeat` | 56,609 | 56,609 |
| `sends.unary` | 1,505 | 8,035 |
| `sends.compiled-unary` | 860 | 10,189 |
| `string.scan-derived` | 3,508,443,000 | 3,508,662,000 |

This confirms that loop/control cost remains substantial after the publication
change. It does not establish a regression against v0.14: quick calibration,
two samples, machine noise and subtraction-sensitive send cases are not release
comparison evidence. No new relational or full-docgen timing was collected in
this research pass; the rollout validation task below owns that measurement.

### Isolated control profile

`bench/control-throughput-probe.birnam` loads the time module, disables automatic
GC, performs one collection, then times 100,000 empty `to:do:` iterations.
`scripts/control-throughput-probe.py` runs it in three fresh processes and
checks the loop's receiver answer. A fourth process emits a ready marker before
sampling, separating source compilation from the profile window. Sampling does
not contribute to the timing median.

- Uninstrumented elapsed times: **7,988,805; 7,915,867; 8,135,047 µs**.
- Median: **79,888 ns/iteration**.
- Separate sampled process: 8,387,081 µs; `sample` ran for 3 seconds at 1 ms.
- Main-thread samples: **2,484**.
- Sampled process peak physical footprint: **1.3 GiB**.

| Leaf function/group | Samples | Share of main-thread samples |
| --- | ---: | ---: |
| `Symbols::intern` | 659 | 26.5% |
| Heap address-map insertion | 233 | 9.4% |
| Heap address-map rehash | 218 | 8.8% |
| `Runtime::make_integer` | 152 | 6.1% |
| `memcmp` | 97 | 3.9% |
| Generated vtable cache lookup | 87 | 3.5% |
| Vtable lookup | 76 | 3.1% |
| `resolve_stackless_target` | 72 | 2.9% |

These are leaf samples, not call counts or predicted speedups. Allocation also
appears below generated StepMethods and Integer construction in the call tree.
The long no-GC probe intentionally accumulates objects: its footprint does not
prove a leak or represent default-GC application memory. Its different loaded
module set and long iteration window also make its timing incomparable to the
micro suite's 512-iteration calibrated case. Both harnesses must be compared
against themselves in future A/B runs.

## 3. Source-backed cost model

The current path is:

```text
to:do: -> source-authored stackless control methods and conditional blocks
       -> generated StepMethods
       -> late-bound Machine slot reads/writes and action messages
       -> symbol interning, boxed operands, dispatch and checked frame access
       -> driver Call/TailCall/Return transitions
```

Verified implementation facts:

1. [`src/id/stackless-control.birnam`](../../src/id/stackless-control.birnam)
   replaces production Integer iteration with `stacklessTo:do:answer:`,
   `stacklessToBody:do:answer:` and `stacklessToContinue:do:answer:`. The native
   `integer_to_do` implementation still exists in the Rust prelude, but treating
   it as the production benchmark path would misdiagnose the problem.
2. [`resumable_c.rs`](../../rust/birnam-codegen/src/resumable_c.rs),
   `emit_get`, `emit_store_prefix`, `emit_call_staging` and `emit_publish`,
   generates `birnam_intern`, `MKV` and ordinary `birnam_send` operations for
   frame transport. Slot numbers and action operands become boxed Integers.
3. [`symbol.rs`](../../rust/birnam-id/src/id/symbol.rs), `Symbols::intern`, scans
   `ordered` with `iter().find(...)`; `contains_bytes` also scans.
   `intern_in_state` can call both in compatibility mode. `by_address` indexes
   object addresses, not bytes, so it does not accelerate these name lookups.
4. The same Symbol table owns weak identities, stable C-name buffers, insertion
   order and post-GC compaction. An index must preserve all four properties.
5. [`runtime.rs`](../../rust/birnam-id/src/runtime.rs) and
   [`execution.rs`](../../rust/birnam-id/src/execution.rs) already share
   resolve-and-pin classification. Frame arenas reuse storage. Temporary
   argument, capture and initial-slot vectors still allocate; their existence
   alone does not make them the dominant cost in this profile.
6. Exact heap membership is already a hash map. Replacing an imagined linear
   heap-membership scan is not a supported optimization direction.

The old benchmark comments describing a native loop and a Zig counter are
historical. The benchmark's stable names and body shapes remain useful, but its
mechanistic explanation needs correction when updating benchmark documentation.

## 4. Bounded implementation sequence

Beads is authoritative for execution status. The dependency order is:

| Slice | Issue |
| --- | --- |
| R1: weak Symbol index | `birnam-symbol-index-oy6` |
| R2: workload validation | `birnam-throughput-validation-ay9` |
| R3: allocation investigation | `birnam-control-allocation-spi` |

### R1 — Index weak Symbol lookup

Scope: `rust/birnam-id/src/id/symbol.rs`, its caller in `runtime.rs`, focused tests
and a small Rust lookup benchmark. Keep `ordered` as enumeration/name ownership
authority and add a derived byte-lookup index. Prefer safe owned byte keys or
hash-to-index buckets with byte equality; choose based on measured lookup cost
and retained bytes. Do not introduce pointers into movable `Vec` entries.

Required behavior:

- Existing arbitrary non-NUL byte names still canonicalize to one live Oop;
  reject embedded NUL without changing state.
- Lookups allocate nothing on hits and do not scan the complete symbol table.
- Preserve enumeration order and stable `c_name` pointers while symbols live.
- Sweep removes dead index entries before reclaiming names; compacted indices
  are rebuilt correctly. The index does not root symbols or retain dead names.
- Reinterning after collection produces the same fresh-identity behavior as
  today. Independent runtimes and image recovery rebuild their own indexes.
- Compatibility limits and `owned_name_bytes` remain correct. Account separately
  for index overhead if the implementation duplicates key storage.

Tests: extend existing equal-byte, non-UTF8, enumeration/pointer stability and
sweep/reinterning cases with survivors in the middle of the table, all-dead
cycles, growth after compaction, and collision handling where applicable.
Measure hit/miss behavior at multiple table sizes (for example 1k/10k/100k),
including late-inserted names. Timing belongs in benchmarks, not flaky test
assertions. Then run the existing GC/image/lifecycle gates and the same control
probe. Keep the change only with a repeatable benefit or a clearly demonstrated
scaling improvement without significant macro regressions.

### R2 — Validate workload payoff and establish the next profile

Use independently built before/after release prefixes. Run full, non-quick
micro and relational suites with the existing fresh-process discipline, then
`docgen.full` through the canonical benchmark driver. Preserve case order,
loaded module sets, cardinality checks, raw timings and calibrated operation
counts. For small effects alternate A/B order on the same boot.

Report loop, query and docgen improvements separately. The query engine may have
additional costs; a faster empty loop is not evidence of a 456× query recovery.
Retain the tagged v0.14 baseline and publish a separate development comparison.
Reprofile the loop and a representative slow macro to choose the next slice.
An unchanged dominant cost is useful evidence; do not keep stacking optimizations
without measuring the first one.

### R3 — Investigate generated-control allocation

Use diagnostic counters or allocator instrumentation in an isolated build to
separate: boxed slot/action operands, user-visible Integers, literal Blocks,
temporary Rust vectors, and retained arena capacity. Vary iteration count and
run with default GC as well as no-GC to distinguish allocation rate from retained
memory. Include a workload with captures and callbacks, not just the empty loop.

Choose one measured source of allocation. Reusing private Rust scratch may be
possible while preserving the existing rooted-copy-before-reentry discipline;
it must survive nested callbacks without sharing mutable borrows or overwriting
outer operands. Boxed operand caching, direct slot access, fused control or
cross-send specialization need an explicit proof/design decision before code.
This task produces that bounded decision and acceptance criteria; it does not
authorize replacing the late-bound execution model.

## 5. Semantic and release gates

[60E](60e-resumable-registration-and-entry.md) requires protected lookup through
classification and acquisition, including lookup-triggered mutation. No new
per-resolution allocation, mutex, atomic RMW or reference-count work belongs in
that fast path. [60F §4](60f-resumable-compiler-lowering.md) explicitly preserves
late-bound Machine microcode and source sends. [60O](60o-image-v8-identity-preservation.md)
preserves boxed identity. No proposal may assume these contracts away.

For every implementation slice, select the relevant existing tests plus:

- `cargo test --workspace --locked --offline` and the Birnam module suite;
- million-transition stack-depth and callback-control coverage;
- ordinary lookup/override/DNU behavior and mutation during argument evaluation;
- GC during slot transport, nested callbacks, suspension and one-shot restore;
- CodeBinding retention, replacement/rollback, unload and bounded churn;
- image-v8 ordinary/rebuild/baked recovery and symbol reclamation;
- project launch smoke, ensuring throughput work does not undo Design 65.

Before a release, complete Design 60H's immutable-archive acceptance, ordinary
performance comparison and longevity gate. The diagnostic results in this note
cannot substitute for those checks. Do not change thresholds, rename cases or
overwrite historical baselines to make an optimization pass.

## 6. Reproduction and provenance

Build a private release prefix, then run commands from the repository root:

```sh
scripts/rust-install.sh install --root "$PWD" --prefix /tmp/birnam-throughput-prefix --profile release
BENCH_QUICK=1 BENCH_OUT=/tmp/birnam-throughput-micro.json \
  /tmp/birnam-throughput-prefix/bin/birnam bench/micro.birnam
python3 scripts/control-throughput-probe.py /tmp/birnam-throughput-prefix/bin/birnam \
  --sample /tmp/birnam-control.sample.txt
```

Use a fresh/nonexistent private prefix when reproducing. Full benchmark driver:

```sh
BENCH_PREFIX=/tmp/birnam-throughput-prefix scripts/bench.sh --floor=rust
```

Research machine: arm64 macOS 14.6. Runtime source-manifest SHA-256:
`2955a62643ef047406b57af1839a2f44bba104149223894d3c7080342d7406ab`.
Floor dylib SHA-256:
`bef2f84ff0e9233e2be9762e540b75937a0f73efbe1465f911d6658c419cb1eb`.
The private prefix was built before the closeout commit; its source manifest
matches the tested implementation and is the identity used for these runs.

Temporary raw evidence: `/tmp/birnam-publication-closeout.VJrOxG/`.

| File | SHA-256 |
| --- | --- |
| `micro-current.json` | `ca132612d1c8ed960558a329024881bceb40c56cc2afe8766f728662b2c218f9` |
| `control.json` | `373ee3bb3659884172fb4a61be2d9a278795a70e3e71700d1e3cc8fad26b8198` |
| `control.sample.txt` | `6718707847e3fb24ea33e2c421f1460e6372d8fb9af3c92c3cd39ba1d0c9588b` |

The timing samples and relevant profile counts are preserved above so the
conclusions remain reviewable after temporary logs expire. A sample taken during
the earlier module suite included parsing and compilation; it was exploratory
only and is not the evidence used to rank the steady-state optimization.

## 7. Implementation evidence

### Weak Symbol name index

The implementation retains ordered name ownership and adds randomized name-hash
buckets containing record indices, with linked collision chains and full byte
equality. It duplicates neither names nor Oop roots. Sweep clears the derived
indexes before reclaiming names and rebuilds indices after compaction. Added
tests cover forced collisions, middle survivors, all-dead cycles, independent
tables and allocation-free hits. Workspace Rust tests passed for this slice.

An ignored release-mode lookup diagnostic compares indexed lookup with the old
linear scan at 1k, 10k and 100k names (10,000 probes, three samples). Indexed
late-name hits were 32, 43 and 25 ns; linear hits were 3,312, 21,647 and 176,281
ns. Indexed misses were 23, 20 and 17 ns versus 2,080, 23,788 and 171,710 ns.
This demonstrates removal of linear scaling, not exact uncontended latency;
other build/test activity may have overlapped that isolated diagnostic.

Independent release prefixes are retained under
`/tmp/birnam-publication-closeout.VJrOxG/current` (before) and
`/tmp/birnam-throughput.To6mYL/symbol-index` (index only). The latter source
manifest is `18597e38113f831b94783548e00c6d74e7ac9a36ca33f5d783612056247c8f2f`.
The full, non-quick development driver has produced these initial results:

| Case | Before | Symbol index |
| --- | ---: | ---: |
| `loops.to-do` | 51,674 ns | 38,427 ns |
| `loops.whileTrue` | 46,496 ns | 35,515 ns |
| `loops.timesRepeat` | 51,585 ns | 38,383 ns |
| `query.single-clause` | 698.412 ms | 717.437 ms |
| `query.join-3way` | 2,424.801 ms | 2,417.849 ms |
| `query.rule-recursive` | 778.844 ms | 704.753 ms |
| `query.asOf` | 546.733 ms | 491.950 ms |
| `docgen.full` | 65.089 s | 52.413 s |

These are one fresh process per suite with full internal calibration; the
loop reductions are about 24–26%, while query results are mixed. Small effects
require repetition. Docgen uses three fresh timed processes plus a separate RSS
run: elapsed time fell 19.5%, while peak RSS rose from 418 to 433 MiB. Final
combined-change results follow below; none of these numbers is a release qualification or proof of general query
recovery. Raw logs and JSON are in `/tmp/birnam-throughput.To6mYL/`.

The separate no-GC control probe gave 9,362,759; 7,466,529; 7,296,660 µs
(median 74,665 ns/iteration). Its first process was unusually slow. A separate
2,451-sample profile shifted toward Integer construction (272 leaf samples),
heap-map insertion (251), heap-map rehash (202), and generated-vtable lookup
(126); Symbol interning itself fell to 45 leaf samples, with additional hashing
and indexed-lookup cost elsewhere. Do not interpret leaf-count differences as
predicted end-to-end speedups.

### Follow-on candidates and allocation diagnostics

`birnam-runtime-overhead-arn` delivered two further changes: derive payload
pointers from provenance-carrying allocation bases to shrink heap metadata,
and index the generated-vtable cache by generation and slot. Workspace Rust
tests passed and a private release prefix built successfully with both changes.
Both passed lifecycle acceptance and the development comparison recorded below.
They preserve ordinary Machine sends and fresh boxed Integer identities.

`scripts/control-allocation-probe.py` provides a separate diagnostic matrix of
iteration counts, automatic GC on/off and empty/captured callbacks. It measures
live floor-payload bytes before, after and after explicit collection; it does
**not** count total allocations, Rust scratch vectors or arena capacity. Those
categories are covered by the isolated counters later in this section.

The combined-change prefix has source manifest
`c7170fdf5bf46c09a08766fc0c9c0de965f8db478601ee526bba1d87dbdf82fe`.
Its first three-process control probe measured 6,109,585; 7,407,049; 6,215,350
µs (median 62,154 ns/iteration); a separate sampled process took 6,120,951 µs.
The first full combined-change driver completed successfully:

| Case | Before | Combined changes |
| --- | ---: | ---: |
| `loops.to-do` | 51,674 ns | 35,666 ns |
| `loops.whileTrue` | 46,496 ns | 30,656 ns |
| `loops.timesRepeat` | 51,585 ns | 34,849 ns |
| `query.single-clause` | 698.412 ms | 723.393 ms |
| `query.join-3way` | 2,424.801 ms | 2,351.128 ms |
| `query.rule-recursive` | 778.844 ms | 705.738 ms |
| `query.asOf` | 546.733 ms | 477.876 ms |
| `docgen.full` | 65.089 s | 43.808 s |

Docgen peak RSS was 370 MiB (before 418 MiB; Symbol-only 433 MiB). These are
development comparisons, not release acceptance; small mixed query effects
still need alternate-order repetitions. Lifecycle gates passed as listed below.
The combined prefix's allocation matrix produced:

| Callback / GC | 1k iterations: live bytes | 10k: live bytes | 100k: live bytes | After explicit GC (all lengths) |
| --- | ---: | ---: | ---: | ---: |
| Empty / on | 3,451,368 | 16,700,040 | 14,949,992 | 113,552 |
| Capture + add / on | 3,467,376 | 16,860,048 | 16,550,168 | 113,560 |
| Empty / off | 3,451,368 | 33,475,368 | 333,715,368 | 113,552 |
| Capture + add / off | 3,467,376 | 33,635,376 | 335,315,376 | 113,560 |

Initial live bytes were 112,256 (empty) and 112,264 (capture). No-GC growth is
exactly 3,336 bytes/iteration between tested lengths, plus 16 bytes/iteration
for the captured addition. Those slopes account for all floor allocation in
these workloads, not just boxed operands. Normal GC bounds live bytes here;
equal post-GC footprints argue against retained loop-length-dependent state.
This does not prove bounded registry/arena capacity or absence of unrelated
leaks. The diagnostic's own timing and telemetry allocate a small fixed amount.

`scripts/control-malloc-probe.py` prepares separate before/after snapshots using
macOS malloc stack logging. `allEvents` includes freed scratch allocations;
`allByCount` reports only live allocations. The workload pauses on stdin for
each snapshot so startup can be separated from the loop. This instrumented
execution must not supply throughput timings. An initial all-event dump was
stopped after exceeding 8 GiB and its incomplete generated output was removed.
The driver now defaults to live-only snapshots, makes all-event history opt-in,
and caps each snapshot at 256 MiB. Live-only snapshots cannot measure freed
scratch; isolated source counters below provide the required site attribution.

Live snapshots did succeed. `scripts/control-malloc-summary.py` groups their
before/after deltas by allocating function and immediate caller. Over 100 empty
iterations, live Integers increased by 19,465: 18,468 from generated StepMethods,
898 from driver resume-token construction, and 99 from other callers. The
captured-addition variant adds exactly 100 further Integers. These are allocator
stack attributions, not a proof that every generated Integer is a private
slot operand; generated code can also construct source literals. The snapshots
also show 600 additional instance allocations (21,600 bytes). Freed scratch
and exact generated-operand/source-literal separation required the counters below.

### Accepted runtime slice and remaining gates

Runtime changes are committed locally as `ec281d1b`. The following passed on
the combined-change prefix: workspace Rust tests; 611 module tests; 100k control
callbacks in AOT/source recovery; resumable eviction/retirement; CodeBinding
publication/replacement/self-removal; stackless image generation projection and
rollback; image-v8 ordinary/rebuild/baked/resave identity; bounded method/Symbol/
history churn; and relocated project check/build/run/install/test with path/Git
dependencies. R1 and the runtime-overhead issue are closed.

R2 is complete with the host-gated repetitions in section 8. R3 is closed with the full
allocator attribution and rejected inline-storage decision documented below.
Final verification is not release qualification; no release or deployment has
been performed by this work.

The first alternate-order repetition batch was intentionally stopped when
unrelated host activity coincided with grossly inflated timings. Its `repeat-*`
files are excluded from comparisons. `BENCH_CHECK_HOST=1` now enables the
canonical driver's existing host preflight, interval monitoring and retry logic
for development runs, without changing release thresholds or tracked outputs.

An isolated archive overlay (`prepare-allocation-diagnostic.py`) builds
successfully and its 10-iteration smoke passes with GC on/off and captured/empty
callbacks. `run-allocation-diagnostic.py` counts between explicit GC boundaries;
its accounting regression tests pass. Empty-loop counts are 1,953 Integers,
including 1,844 generated Machine operands, 60 Blocks, 152 initial-slot vectors,
160 nonempty argument vectors and 30 nonempty capture vectors. Captured addition
adds exactly ten non-operand Integers. Retained arena bytes are zero at both
boundaries. Full varying-length counts and the bounded decision follow below.
This instrumentation exists only in the private archive, not production code.

### R3 full counters and bounded follow-on

The 100/1,000/10,000-iteration counter matrix completed with GC on/off and
empty/captured callbacks. An accounting regression initially omitted work before
automatic GC; retained raw logs were recounted after adding a regression test
that includes intermediate collections between the fixture's first and last
explicit collection. Corrected counts are identical with automatic GC on/off:

| Allocation site | Count over N empty iterations |
| --- | ---: |
| All Integers | 195N + 3 |
| Generated Machine slot/action operands | 184N + 4 |
| Other Integers (including driver resume tokens) | 11N - 1 |
| Literal Blocks | 6N |
| Initial-slot Vec backing allocations | 15N + 2 |
| Nonempty argument Vec backing allocations | 16N |
| Nonempty capture Vec backing allocations | 3N |

Captured addition adds exactly N non-operand Integers; other counts stay equal.
Initial-slot capacities in the 100-iteration diagnostic are 2, 4 and 5 entries.
Arena reservation is zero at the explicit boundaries; one automatic collection
in each 10k GC-on case observes 752 bytes. This is sampled arena capacity, not
an instrumented high-water maximum. Raw evidence and receipts are in
`/tmp/birnam-throughput.To6mYL/counters-full/`.

The bounded follow-on experiment is invocation-owned inline initial-slot storage
for up to eight entries, with Vec fallback for larger entries. It targets the
15N+2 backing allocations without sharing mutable scratch or changing rooted
copy order, entry acquisition, semantic sends, or boxed identity. Overflow and
independent-owner tests pass, as do focused Rust tests including million-step
checks. A private `inline` release prefix built successfully. Retention requires
host-gated before/after measurement and the lifecycle matrix; it is not accepted
solely because fewer allocation calls are possible.

Full `mise run verify` passed for the committed `ec281d1b` runtime before this
experiment, including two independent cold source archives and relocated
runtimes. The inline experiment was subsequently rejected.

The experiment failed `portable-resumable-lifecycle-smoke.sh`: after dropping
the last published Block and collecting, live native units stayed at baseline+2
instead of baseline+1. The failure reproduced 3/3 times with the inline prefix;
the committed Vec-backed prefix passed 3/3 differential trials. Return values
before and after eviction remained correct. Precise heap tracing did not remove
the extra unit; this does not disable native-stack scanning, so stale inline
Oop copies/conservative stack retention remain a hypothesis, not a proven
internal root path. No thresholds or lifecycle assertions were relaxed.

**Decision:** reject and remove invocation-inline initial-slot storage. Keep the
three proven runtime improvements from `ec281d1b`; retain invocation-owned Vec
copies for control transport. A future scratch-storage design must first prove
quiescent reclamation as well as nested callback/root safety. Do not pursue
operand canonicalization or direct Machine access under current contracts.
The existing end-to-end retirement smoke is the regression seam; isolated
buffer tests did not cover this GC/retirement interaction. The production source
was restored exactly to `ec281d1b`, whose full verification had passed, and the
original retirement reproduction was rerun against its retained prefix.

### Representative macro profile and final open gate

A separate `docgen` process from the committed combined prefix completed
successfully. Sampling began five seconds after launch and ran for three
seconds at 1 ms, yielding 2,532 main-thread samples. Leading collapsed leaves
were deallocation (261), vtable lookup (111), portable-program inspection (110),
store current-value lookup (104), name hashing (92), and global import (88).
This window shows a more mixed workload than the isolated control loop;
allocation reclamation and portable/store processing remain relevant. It does
not establish that bypassing validation or caching mutable store results is
safe. No additional production change is justified by these leaf counts alone.

The profile was collected on battery and is attribution evidence only: neither
its elapsed time nor its partial-window footprint enters the timing comparison.
Raw sample SHA-256: `b743d6857c74eae19cbd3247ce6111670c1237746e61464f233be8ba2f57ff2a`.
Corrected allocation-result SHA-256:
`43413bf5e902263cf3bbb49754c8f472e452ffce745dcc604c9728a520fa83aa`.

The final R2 gate was a quiet-host, AC-powered repeat comparison.
Unstable-host repetitions were excluded, and the subsequent preflight observed
battery power repeatedly. Do not treat the initial development measurements as
release qualification or silently relax the host gate. Resume with the original
publication baseline and the accepted combined prefix, not the rejected inline
or instrumented prefixes:

```sh
BENCH_CHECK_HOST=1 BENCH_PREFIX=/tmp/birnam-throughput.To6mYL/optimized \
  BENCH_FINAL_OUT=/tmp/birnam-throughput.To6mYL/checked-after.json \
  scripts/bench.sh --floor=rust
BENCH_CHECK_HOST=1 BENCH_PREFIX=/tmp/birnam-publication-closeout.VJrOxG/current \
  BENCH_FINAL_OUT=/tmp/birnam-throughput.To6mYL/checked-before.json \
  scripts/bench.sh --floor=rust
```

This reverses the original before/after order. Inspect individual raw/net cases
and calibrated counts, retain only host-accepted runs, and close
`birnam-throughput-validation-ay9` only after the comparison is reviewed.

## 8. Final host-gated development comparison

After AC power was restored, the accepted combined runtime ran first and the
original publication runtime second, reversing the initial order. Both used
the full non-quick fixtures and `BENCH_CHECK_HOST=1`. The baseline batch exited
zero. The optimized batch produced host-accepted micro, relational, GC, image,
and docgen results but exited one because all four `boot.image` timing attempts
were spoiled by host load. That case was rerun successfully in isolation using
an exact copy of the canonical driver's measurement/monitoring functions, with
only the image-suite and image-startup invocations retained. No repetitions,
thresholds, fixture bodies, or release baselines were changed.

The composite optimized record adds only that missing host-accepted `boot.image`
result and explicitly identifies its supplemental source. It does not reclassify
the original partial batch as passing. Complete raw/net values, calibrated
operation counts, repetitions, stamps and supplement provenance are retained in
[`66-throughput-comparison.json`](../measurements/66-throughput-comparison.json).

| Case | Baseline | Optimized | Elapsed change |
| --- | ---: | ---: | ---: |
| `loops.to-do` | 48,592 ns/op | 34,700 ns/op | -28.6% |
| `loops.whileTrue` | 46,774 ns/op | 31,600 ns/op | -32.4% |
| `loops.timesRepeat` | 49,899 ns/op | 35,487 ns/op | -28.9% |
| `query.single-clause` | 712.557 ms/op | 722.978 ms/op | +1.5% |
| `query.join-3way` | 2,426.301 ms/op | 2,453.877 ms/op | +1.1% |
| `query.rule-recursive` | 733.919 ms/op | 706.414 ms/op | -3.7% |
| `parse.json-20kb` | 599.050 ms/op | 400.921 ms/op | -33.1% |
| `boot.image` | 1.960 s | 1.916 s | -2.2% |
| `docgen.full` | 62.723 s | 44.022 s | -29.8% |

Docgen peak RSS fell from 459,145,216 to 417,595,392 bytes (-9.0%). All query
cases stayed between -3.7% and +1.5%; this confirms loop and documentation
recovery, not broad relational recovery. No measured macro case regressed.

**Strict comparator: exit 1, two regression flags, five unusable values, zero
missing cases.** This result is preserved, not waived:

- `string.split-native`: 3.105 to 3.604 ms net (+16.1%); raw time also increased
  15.5%. The earlier development comparison instead improved 3.163 to 2.874 ms.
  This variable case is not evidence of a uniformly faster runtime and needs
  ordinary multi-process release qualification before a release claim.
- `error.propagate-5-deep`: 688 to 7,309 ns net, but raw time fell from 74,959 to
  59,415 ns (-20.7%). Baseline subtraction leaves a very small residual, so the
  net ratio is ill-conditioned; the comparator still correctly flags its input.
- `map.at-hit`, `cells.read`, `ffi.wrapped-call`, `ffi.peek-poke`, and
  `sends.compiled-unary` have a zero/negative net value on at least one side.
  No speedup is claimed from those net ratios. Frozen bodies were not enlarged
  or renamed to make the comparator pass.

The bounded Design 66 implementation and development evidence are complete.
Release acceptance remains governed by ordinary Design 60H policy, including
clean multi-process comparisons and its other gates; no release or deployment
was performed. Raw evidence hashes:

| Artifact | SHA-256 |
| --- | --- |
| `checked-before.json` | `3c5c6ca2bcd3364c7b9da02b7309903510162926f4a90e675895f4f0638e45c3` |
| `checked-after.json` (partial batch) | `e907db1260fe8ba1b88faadb33b44dc6bf9200aa99848fc3d46c05585feb2282` |
| `checked-after-image-supplement.json` | `843454fcca11665784414c1354447d277b79dc810d2e8ce0871e0220490033c4` |
| `checked-after-complete.json` (composite) | `ff767df57caedf8e5436c757736495a28a96e7a3af05ebf7f9d06646084e7066` |
