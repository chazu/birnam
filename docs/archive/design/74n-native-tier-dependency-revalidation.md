# 74N — Revalidate native-tier dependencies

Status: completed, 2026-09-11. Correctness and increased native coverage are
verified. Seven controlled timing groups show no independent execution or memory
benefit beyond variation; both chain warmups regress relative to the disabled tier.
Tracker: `birnam-native-tier-revalidation-7qd`.

## Problem and change

74M proved that interning an unrelated symbol permanently withdraws a warmed
candidate. Its global dictionary epoch changes, although the candidate's actual
assumptions still hold. Native PC specialization retains the original body and
descriptor, and performs ordinary Id sends; it does not cache ordinary method
targets or language values.

Remove the candidate's saved epoch and its blanket epoch-difference rejection.
On every admission, retain exact descriptor allocation identity, the saturation
check, and `guarded_slot_access_eligible`. That guard already revalidates canonical
Machine read/write and lookup bindings after dictionary mutation, while checking
the actual Machine header and resumable registration independently on every
attempt. Its cached verdict is private and non-effectful. An actual dependency
failure still withdraws the candidate permanently, with same-Frame baseline
continuation and the selected activation's independent native-code owner.

Replacing an ordinary method, including the active body's published binding,
need not withdraw the candidate: an old Frame keeps its descriptor and body;
later sends resolve the current binding. No cached send targets are introduced.
New symbol and export activity should retain native admission. Saturated epochs
remain baseline-only. No IdApi, ABI, primitive semantics, descriptor, registration,
image format, eligible program subset, allocation order or hot threshold changes.
Eager candidate preparation remains as before; lazy preparation is separate work.

## Verification and fixed performance protocol

Extend the existing paired native/baseline checks with symbol insertion, export,
ordinary target replacement and registration without a dictionary mutation.
Retain the existing mutation, custom lookup, private Machine, saturation, exact
descriptor, nested entry, GC, active replacement and eviction/lifetime checks.
Run debug and release correctness plus the ordinary production verification gate.

Repeat all seven 74M scenarios unchanged, with seven alternating off/on groups,
five repetitions per process and the existing host preflight/interval gates.
Use the same median-difference criterion: a classified change must exceed both
observed process ranges. Report setup, execution, wall time and peak RSS separately.
Finish compilation and private diagnostic runs before timing. Do not tune counts,
workloads or thresholds after observing comparative results.

Archive committed `24e200a3` plus an explicit patch containing only this slice's
Rust changes. Build enabled, disabled and private-profile release variants from
that identical candidate snapshot. Reuse the 74M staging/selection disable and
private instrumentation transforms; compose prefixes with identical other
binaries and retain complete build manifests, patches and export inventories.
Keep 74M scripts and receipts intact. The new runner records its own source and
the imported 74M helpers and benchmark hashes. Strip runtime and host-gate
environment overrides for the experiment.

Run three separate diagnostic repetitions for coverage, compilation and native
unit retention. Require at least 120,000 native entries in each steady-chain
diagnostic. Repeat the existing/new-symbol probe; both must retain at least the
original 122,817 warm-plus-measured native entries and correct answers. Diagnostic
clocks are not comparative timing samples. A correctness pass or increased native
coverage alone does not establish a performance benefit.

## Correctness and coverage results

All eighteen native/baseline pairs pass in debug and release, including the six
new symbol/export/current-target and registration cases. The active replacement
case now keeps all three old-Frame PCs native, still returning the old answer
28 before a later send returns the replacement's 99. A separately replaced
terminal target returns 99 during the current invocation, proving sends remain
late-bound. Real Machine/lookup changes, saturation and eviction still fall back
without repeating effects. Registration at each canonical read/write/lookup
address vetoes admission without moving the dictionary epoch; removing that
unpublished registration does not resurrect the withdrawn candidate.

The five guarded Machine/lookup tests pass in debug and release in both normal
and disabled snapshots. `mise run verify` passes: 448 workspace tests pass,
11 remain ignored, and native-prefix, lifecycle, cold-archive and relocation
checks succeed. The public compatibility authorities remain byte-identical to
`24e200a3`, and all three floor variants expose the same 25 symbols.

All three separate diagnostic repetitions agree on these entry counts:

| Scenario | 74M native PC entries | 74N native PC entries |
| --- | ---: | ---: |
| Original chain, 1,024 calls | 36 | 15,399 |
| Original chain, 4,096 calls | 36 | 61,479 |
| Compile/evaluate | 0 | 0 |
| Ordinary collections | 0 | 0 |
| Portable collections | 33 | 513 |
| Cold materialization | 0 | 0 |
| Sustained native chain | 122,817 | 122,820 |

Both existing/new-symbol probes retain 122,820 entries, one successful native
compilation and correct answers. The three additional sustained entries are the
fixture's final post-measurement invocation. The original chain still withdraws
its candidate after the later custom-lookup installation. No ordinary program
became newly eligible: compile/evaluate and ordinary collections still never
enter this tier.

Hot scenarios peak at one native unit. Portable collections and the sustained
chain now retain that unit at the exit-profile callback, while their descriptors
remain live; original-chain scenarios end with zero after actual invalidation.
The retention is expected from preserving valid candidates. Diagnostic compilation
clocks range from 0.323 to 0.660 ms. Cold materialization still prepares 320 unused
candidates, emits 1,825,280 source bytes and never compiles a candidate; private
preparation clocks total 6.88–7.02 ms per process. These instrumented clocks are
descriptive and are not timing comparisons with 74M.

Enabled/disabled floor files are 1,334,256 and 753,376 bytes: 580,880 additional
file bytes. Generated source size and file size are not executable-memory usage;
whole-process peak RSS is measured separately in the timed comparison.

## Controlled timing results

The [preparation receipt](../../measurements/74n-native-tier-preparation.json) retains
the complete candidate patch, build/composition manifests, debug/release paired
tests, all 21 diagnostics and both symbol probes. The
[verification receipt](../../measurements/74n-native-tier-verification.json) retains
the production gate output, both disabled-variant guard test outputs, source
parity checks and the authorized process-pause/resume record. The
[completed timing receipt](../../measurements/74n-native-tier-focus.json) retains
all seven groups, individual samples, process medians, resource usage and host
checks. Its source hashes and candidate build are identical to preparation.

Every preflight and interval gate passed. Median idle CPU during groups was
71.22–73.45%, with zero low-idle intervals. With explicit user approval, `rk` was
paused for the whole comparison, equally for both variants, and resumed after
approximately 177 seconds. Its original process identity and running state were
verified afterward; the fallback resume watchdog did not fire. No build or
diagnostic workload overlapped timing, and no threshold or workload changed.

Batch results in milliseconds follow. Range width is maximum minus minimum
across the seven process medians. Every row is inconclusive under the declared
criterion, which requires the median difference to exceed **both** widths:

| Work | Disabled median | Enabled median | Disabled range width | Enabled range width |
| --- | ---: | ---: | ---: | ---: |
| Original chain, 1,024 calls | 36.168 | 36.145 | 3.031 | 3.945 |
| Original chain, 4,096 calls | 194.545 | 184.686 | 17.088 | 18.168 |
| Forwarding lookup fallback, 1,024 calls | 45.654 | 44.858 | 4.946 | 3.649 |
| Forwarding lookup fallback, 4,096 calls | 220.947 | 222.933 | 8.339 | 18.933 |
| Compile/evaluate, 32 programs | 21.918 | 21.804 | 2.292 | 2.665 |
| Ordinary collections, 32 pipelines | 352.759 | 350.579 | 35.384 | 30.719 |
| Portable collections, 32 pipelines | 344.960 | 352.843 | 49.200 | 21.772 |
| Cold materialization, 64 cycles | 37.025 | 38.885 | 1.808 | 2.502 |
| Sustained native chain, 4,096 calls | 174.779 | 172.023 | 16.522 | 19.886 |

The larger original chain's apparent 5.1% saving and sustained chain's apparent
1.6% saving are below variation; neither establishes a speedup. Cold
materialization's apparent 5.0% increase also remains inconclusive. All
whole-process wall-time and peak-RSS comparisons are inconclusive, including
the scenarios that now retain their native unit until process exit. Their full
individual samples are in the timing receipt; no memory improvement is claimed.
Whole-process medians and range widths are below, with disabled / enabled
values in each cell. RSS is peak resident memory for the whole process, not a
direct measure of native code allocation:

| Scenario | Wall median (ms) | Wall range width (ms) | Peak RSS median (MiB) | RSS range width (MiB) |
| --- | ---: | ---: | ---: | ---: |
| chain-1024 | 687.35 / 690.26 | 63.64 / 62.44 | 207.12 / 217.84 | 17.53 / 18.70 |
| chain-4096 | 2232.75 / 2216.82 | 54.61 / 40.59 | 223.66 / 226.50 | 40.22 / 25.08 |
| eval | 279.26 / 286.99 | 27.69 / 37.80 | 126.88 / 129.69 | 21.00 / 13.86 |
| collections | 2258.64 / 2235.16 | 53.08 / 222.94 | 221.58 / 237.33 | 31.67 / 23.67 |
| portable-collections | 2244.85 / 2253.48 | 175.06 / 179.62 | 221.38 / 222.59 | 38.09 / 16.95 |
| materialize-cold | 453.04 / 473.62 | 21.80 / 29.47 | 134.44 / 139.20 | 15.12 / 10.59 |
| steady-chain | 2631.37 / 2623.57 | 67.16 / 171.63 | 231.17 / 228.30 | 23.56 / 39.88 |

Two setup regressions exceed both observed widths:

| 32-call warmup | Disabled median (us) | Enabled median (us) | Increase (us) | Disabled / enabled range widths (us) |
| --- | ---: | ---: | ---: | ---: |
| Original 1,024-call scenario | 1,115 | 1,467 | 352 (31.6%) | 168 / 242 |
| Original 4,096-call scenario | 1,064 | 1,360 | 296 (27.8%) | 172 / 146 |

These are warmup costs relative to tier-disabled execution, not whole-program
slowdowns and not a direct comparison of the 74M and 74N implementations. The
other setup and first-call differences remain within observed variation.

## Rejected attempt and reproduction

The [first timing attempt](../../measurements/74n-native-tier-rejected-1.json) did
not pass group 1's preflight and collected no comparative samples. The unchanged
gate requires the hottest competing process to use at most 80% of one core.
The unrelated `rk` daemon repeatedly exceeded that limit (the last captured
preflight reported 193.1%). After repeated rejections, the still-retrying
preflight was terminated; this was not its automatic 900-second timeout.
No host threshold was relaxed and no other application's process was paused
during that first attempt. The subsequent approved pause enabled the completed
run reported above; the rejected evidence remains intact.

Reuse the verified prefixes on a quiet host and write a new receipt for each
attempt:

```sh
python3 scripts/native-tier-revalidation.py --work-dir /tmp/birnam-74n-run \
  --measure --output /tmp/birnam-74n-run/timing-new.json
```

To reproduce everything in a fresh directory:

```sh
python3 scripts/native-tier-revalidation.py --work-dir /tmp/birnam-74n-new \
  --correctness --prepare --diagnose --probe \
  --output /tmp/birnam-74n-new/preparation.json
CARGO_TARGET_DIR=.cargo-target mise run verify
python3 scripts/native-tier-revalidation.py --work-dir /tmp/birnam-74n-new \
  --measure --output /tmp/birnam-74n-new/timing.json
```

Finish other build/test work before timing. The original 74M scripts, fixtures,
counts and receipts remain intact.

## Decision and next work

Dependency revalidation fixes premature withdrawal without weakening Id
malleability or changing the ABI. The implementation and focused testing task
are complete. Increased native coverage still does not establish an independent
performance benefit, and the reproducible warmup costs remain material.

The next bounded experiment should defer candidate preparation until a body is
hot enough to need it. Cold diagnostics already identify 320 candidate sources
prepared but never compiled per process. This removes optional eager work from
cold bodies without widening language restrictions or program eligibility; it
will still need the same correctness and cost comparison. Hot compilation's
warmup cost will remain a separate question. General inlining, read caching,
unboxing and broader program eligibility are not justified by these results.
