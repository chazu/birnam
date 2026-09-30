# 74M — Isolate native-tier performance and coverage

Status: completed, 2026-09-11. Seven controlled on/off groups show no qualified
steady-execution or whole-process memory benefit. The smaller chain's warmup
regresses by 420 us. Separate diagnostics prove limited coverage and candidate
withdrawal on unrelated symbol interning. Production source remains fixed at
`24e200a3`; no optimizer expansion or public API/ABI/semantic change was made.
The protocol and diagnostic amendment below preceded the timing comparisons;
their original text is preserved in the measurement receipt.

## Question

74L qualified the combined implementation on one warm portable fixture. Its
separate native-tier and lookup gains were inconclusive. Determine whether the
native tier itself earns its compilation, retention and footprint costs, and
whether representative ordinary work reaches it.

## Isolation

Build ordinary release prefixes from the same committed source. The enabled
variant is unchanged. In an isolated source snapshot, the disabled variant
empties only native-tier staging and selection; 74J Machine sends, 74L lookup,
baseline compilation, registration and execution remain identical. Retain the
exact patch, source/prefix manifests and floor exports. Compare other installed
binaries to expose any build differences. No development helper ABI is added.
Common registration bookkeeping, including descriptor copying, stays in both
variants. This isolates the optional staging/compilation/selection work rather
than reconstructing a pre-74K runtime. Both snapshots receive identical hashed
prebuilt terminal-wrapper inputs required by the normal source-build manifest.

During preparation, normal rebuilding changed temporary `LC_ID_DYLIB` paths in
native and stage1 library headers. For timing, compose three experiment prefixes
from the enabled prefix's identical host/module files, replacing only the floor
with each variant's release-built floor. Assert that every other binary is
byte-identical. Keep the original build receipts separately; remove the canonical
install receipt from each composed prefix and label its exact composition.
These are explicit experiment prefixes, not a new packaged-runtime publication.

A third isolated snapshot adds private native-tier counters and clocks. Run it
separately for coverage and compilation diagnostics; never use its execution
times as performance samples. Record staging attempts, considered/eligible rows,
descriptor matches, selection attempts, native entries, invalidations, compiler
attempts/failures, compilation time, generated C bytes and live/peak native units.
Counters allocate no Birnam objects and send no language messages. Generated C
bytes are a source-size metric, not executable-memory accounting.

## Workloads and fixed protocol

Run six fresh-process scenarios for seven enabled/disabled pairs, alternating
order. Use five timed repetitions per scenario. Retain each new-workload
repetition and the existing portable-chain fixture's process medians.

- Existing portable-chain fixture at 1,024 and 4,096 iterations, including cold
  materialization, 32 warmup calls, method mutation, native forwarding lookup,
  post-eviction execution and GC. Preserve its existing process-median output.
- Ordinary compiler/evaluation: 32 compilations/evaluations of a small collection
  program per repetition, with checked answers.
- Ordinary collections: 32 collect/select pipelines over 256 values per
  repetition, with ordinary callback Blocks and checked answers.
- Portable collections: the same pipeline through an explicitly materialized
  multi-PC portable Block; callbacks are arguments and collection implementations
  remain ordinary. Record cold materialization and 32 warmup calls separately.
- Cold portable materialization: 64 cache-evict/materialize/call cycles per
  repetition. Each new native baseline gets one call, below the tier threshold.

New warm workloads receive 32 calls of warmup; cold materialization receives no
warmup. Workload syntax and answer checks may be repaired during untimed smoke
validation. Freeze hashes and all counts before the first performance pair.

Use the existing host preflight and interval gates unchanged. Complete all
builds/diagnostics before timing and run no parallel workload during measurement.
Record wall time and macOS maximum resident-set size for each process, plus
floor file size and separately measured tier compilation/retention diagnostics.
RSS is whole-process peak memory, not a direct measurement of JIT code size.

For each timed row, compare the median of seven process medians. Classify a
change only when its magnitude exceeds both observed process ranges. Apply the
same descriptive rule to wall time and peak RSS. Keep cold/setup costs separate
from steady execution. Preserve rejected attempts; do not tune workloads or
thresholds after seeing comparative timing. An above-spread regression prevents
a benefit claim for the tier on that workload. No overall speedup is inferred
from a single case or from diagnostic counters.

## Deliverable

Retain the reproducible runner, source-only snapshot patches, correctness
outputs, coverage/compilation diagnostics, all timing/host/memory samples and a
decision about the next bounded optimization. Ineligible ordinary programs are
a coverage result, not evidence that native code ran or a reason to change
language semantics. Existing Id malleability and same-Frame recovery remain the
required boundary for any subsequent implementation.

## Diagnostic amendment before the first timing pair

The initial three diagnostic repetitions found only 36 native PC entries in
each existing chain process and 33 in each portable-collection process, followed
by one invalidation. Those counts fit entirely within the 32-call warmup.
Ordinary evaluation/collections had zero native entries. All 320 cold
materializations per process were eligible but remained below the hot threshold.
The initial diagnostic receipt is retained separately; its clocks are diagnostic
only and some collection overlapped separate test compilation.

Retain all six scenarios and add a seventh, `steady-chain`, to answer the
otherwise-unmeasured steady-execution question. Create the batch/timing/result
closures and cells before portable admission; exercise that exact harness on
the synchronous Block first. Replace its target cell with the portable Block,
warm it with five 4,096-call batches, then measure another five such batches.
Print results only afterward. No runtime policy changes are made.

The instrumented smoke observed 122,817 native PC entries in this case. Its
portable warmup can account for at most 61,440 entries, so native execution
necessarily extends into the measured batches. Require at least 120,000 native
entries in each of the three final diagnostic repetitions before starting
timing. The extra scenario and this check precede all on/off timing comparisons;
the seven-pair host gates and classification rules are unchanged.

## Results

The [measurement receipt](../../measurements/74m-native-tier-focus.json) contains all
seven controlled pairs, three diagnostic repetitions per scenario, exact
snapshot patches, build receipts, source hashes, composed-prefix binary hashes,
raw program output, resource usage and host samples. All host gates passed:
median idle CPU was 54.95–70.20%, with at most one low-idle sample per group.
Every timed binary except `libbirnam_id.dylib` is byte-identical across variants;
both floor export inventories have the same 25 symbols. Timed prefixes contain
no counters or diagnostic clocks.

Median batch times, in milliseconds, follow. Every row is inconclusive under
the predeclared spread criterion:

| Work | Tier disabled | Tier enabled | Enabled time change |
| --- | ---: | ---: | ---: |
| Original chain, 1,024 calls | 36.923 | 37.758 | +2.3% |
| Original chain, 4,096 calls | 203.800 | 195.918 | -3.9% |
| Compile/evaluate, 32 programs | 22.227 | 21.614 | -2.8% |
| Ordinary collections, 32 pipelines | 363.877 | 379.399 | +4.3% |
| Portable collections, 32 pipelines | 357.704 | 363.722 | +1.7% |
| Cold materialization, 64 cycles | 37.541 | 39.381 | +4.9% |
| Sustained native chain, 4,096 calls | 182.937 | 174.755 | -4.5% |

The sustained-native median saves 8.182 ms, below the observed process ranges
of 14.281 and 13.207 ms, so it does not establish a speedup. Both forwarding
lookup fallback comparisons, every whole-process wall-time comparison and
every peak-RSS comparison are also inconclusive. The apparent differences in
those medians must not be reported as improvements or regressions.

One setup cost *is* distinguishable: the original 1,024-call chain's 32-call
warmup takes 1.116 ms disabled versus 1.536 ms enabled. The 420 us increase
(37.6%) exceeds both ranges, 70 and 173 us. This is a warmup regression, not a
37.6% whole-program slowdown. Other setup/first-call comparisons are inconclusive.

## Coverage, compilation and memory

All three final diagnostic repetitions agree on the following counts. Counts
cover the entire process, including startup, and PC entries are not CPU-time
shares or a count of all language sends.

| Scenario | Eligible/considered portable rows | Native compilations | Native PC entries |
| --- | ---: | ---: | ---: |
| Original chain, either size | 1/7 | 1 | 36 |
| Compile/evaluate | 0/4 | 0 | 0 |
| Ordinary collections | 0/4 | 0 | 0 |
| Portable collections | 1/5 | 1 | 33 |
| Cold materialization | 320/324 | 0 | 0 |
| Sustained native chain | 1/5 | 1 | 122,817 |

The original chain's 36 entries and portable collection's 33 entries are fully
accounted for by their warmups, followed by one invalidation. The added sustained
case uses the same precreated harness and retains native execution throughout
both portable warmup and timed batches. Its final invalidation occurs after
those batches. All compiler attempts succeed. Diagnostic native compilation
clocks range from 0.493 to 1.413 ms; these instrumented clocks are descriptive,
not a timing comparison with disabled execution.

Cold materialization emits 1,825,280 bytes of candidate C across 320 eligible
rows, spending 11.6–12.6 ms in private staging/preparation, yet compiles no native
candidate because each baseline gets only one call. This identifies optional
eager work that never pays off in this workload. C-source bytes are not JIT
executable bytes or retained-memory measurements.

Hot scenarios peak at one live candidate unit and finish with zero; the others
never allocate a candidate unit. Enabled/disabled floor files are 1,334,464 and
753,376 bytes respectively: this isolated tier costs 581,088 file bytes. This
differs from 74L's approximately 603 KB comparison against the older `5e242e70`
baseline because the disabled 74M build retains common registration bookkeeping
and the newer dispatch implementation. Whole-process peak RSS is noisy; none
of the measured RSS differences exceeds both observed ranges.

## Causal invalidation follow-up

After completing timing, the [symbol probe](../../measurements/74m-native-tier-symbol-invalidation.json)
repeats the sustained harness with one send between portable warmup and the
final batches. Interning existing `oopAt:` retains 122,817 native entries.
Interning a previously unseen symbol leaves only 61,377 entries, all from the
portable warmup. Answers remain correct, and both runs compile exactly once.
The probe compares counts and answers only; its instrumented times are unused.

The source explains this behavior: inserting an interned symbol updates an
internal opaque dictionary via `add_weak_opaque_binding`, which increments the
same global vtable epoch used by native-tier selection. On any epoch difference,
selection permanently disables an already-compiled candidate without first
revalidating its actual assumptions. This demonstrates an unrelated-symbol
trigger; it does not claim to identify every mutation in the original harness.

## Decision and next bounded work

The native tier's independent performance case remains unproven. 74L's combined
17.3% result remains valid for that comparison, but is not evidence that native
PC specialization provided a steady-execution gain.

The next proposed implementation slice is to revalidate the candidate's actual
dependencies after unrelated dictionary changes while preserving fallback for
real Machine/lookup changes, descriptor identity, same-Frame continuation and
native ownership. A new interned symbol should no longer permanently discard
an otherwise-valid candidate. Then repeat this exact on/off comparison before
claiming a benefit. This proposal is tracked as `birnam-native-tier-revalidation-7qd`.
Separately, defer candidate C generation for cold bodies
until they approach admission; the cold-materialization diagnostics justify
investigating that work. Wider program eligibility and more complex optimization
remain subsequent choices based on measured payoff.

## Reproduction and verification

From a prepared macOS checkout with its normal terminal-wrapper build inputs
and no `BIRNAM_*` or `DYLD_*` runtime overrides (none were set for this run):

```sh
python3 scripts/native-tier-focus.py --work-dir /tmp/birnam-74m-new \
  --prepare --diagnose --measure --output /tmp/birnam-74m-new.json
python3 scripts/native-tier-invalidation-probe.py \
  --profile-prefix /tmp/birnam-74m-new/run-profile \
  --output /tmp/birnam-74m-symbol-new.json
```

An existing matching release prefix can be supplied with `--on-prefix` to avoid
rebuilding it. Use fresh work/output paths for each attempt. The five guarded
Machine/lookup tests pass in both debug and release in the disabled snapshot.
`mise run native-tier-test` passes against the unchanged production source.
All seven product scenarios pass their answer/identity/mutation/GC checks in
both timed variants. The [verification record](../../measurements/74m-native-tier-verification.json)
links the exact evidence and archives the initial diagnostic runner, whose
source preceded the seventh-scenario amendment.

Follow-through: [74N](74n-native-tier-dependency-revalidation.md) implements
dependency revalidation and repeats all seven controlled comparison groups.
Native coverage now survives unrelated dictionary activity, but no independent
execution or memory benefit exceeds variation; both chain warmups regress
relative to the disabled tier. Deferring unused cold candidate preparation is
the next proposed experiment.
