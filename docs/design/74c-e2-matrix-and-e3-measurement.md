# 74C — E2 matrix and E3 measurement of guarded Machine slot access

> **Status: E2 delivered, E3 protocol frozen and measured 2026-09-09 under the
> user's authorization of both; the §3 measurements were taken only after the
> commit that froze §2. E3 verdict: the frozen 20% criterion is not met (13–20%
> real improvement, every guard hit); the operator later withdrew the
> percentage threshold (§3.2a), under which E3 is met; the next hypothesis in §3.5 is written
> up as [Design 74D](74d-e4-slot-index-box-elision.md). Development-only throughout: the E1
> artifact ([Design 74B](74b-e1-guarded-slot-access-implementation.md)) is
> unchanged by this document. A production slot-access slice is not proposed
> here; Design 74 §7.6 governs what a positive result may lead to.**

## 1. E2 — adversarial semantic and lifetime matrix

Each row of Design 74 §7.4 is mapped to evidence. "Script" rows run
[`bench/e2-slot-access-matrix.birnam`](../../bench/e2-slot-access-matrix.birnam)
through [`scripts/e2-slot-access-matrix.py`](../../scripts/e2-slot-access-matrix.py):
one process per case per configuration, baseline (ordinary prefix) and
candidate (dev prefix, `BIRNAM_DEV_GUARDED_SLOTS=1`), with byte-identical
stdout required and the candidate's guard counters recorded. "Test" rows are
the feature-gated Rust test in 74B §4. "Suite" rows are the module suites run
on the dev prefix with the guarded emission selected for every unit they
compile (74B validation record).

| §7.4 row | Evidence | Result |
| --- | --- | --- |
| Ordinary execution: same answer, checksum, effects, error behavior | Script `empty`, `ordinary-error`, `invalid-body`; E1 driver checksums at five sizes | Identical: empty loop answers the receiver with zero calls; an Error answered at index 3 stops the loop with kind `#e2Stop` after exactly two step calls; a non-Block body answers `#doesNotUnderstand`. |
| Replace `step:` during execution, including during argument evaluation | Script `replace-step`, `replace-during-argument` | Identical checksum 20,150 (indices 1–4 under the first definition, 5–20 under the second) and exactly 20 step calls, so later sends observe the current definition and nothing runs twice. In the argument-evaluation variant the redefinition happens inside the argument expression of the very send that then dispatches to the new definition (Design 60F §5 order). |
| Override Machine slot access before and during execution | Script `override-oopat-before`, `override-oopat-during` | Identical, and fatal in both configurations: a Birnam-level `oopAt:` override on `Machine` answering a constant makes every staged send operate on that constant, which collapses into a synchronous does-not-understand chain and a native stack overflow. The guard misses (condition 2) and the ordinary send runs the override, so eligibility fails before the affected operation as required; the fatal outcome is the baseline's and is reported in §4. |
| Custom lookup returns the same target but logs/mutates | Test | Any non-primitive `lookup` on `vtable_vt`, and any non-canonical metavtable on the Machine vtable, makes the guard miss without inspecting the lookup's result, so a lookup's effects occur exactly once on the ordinary path. |
| Remove/restore a binding, change inheritance, activate/roll back code | Test (remove/restore, `set_parent` epoch), script `activate-rollback` | Identical: 350 before, 20,350 while a successor generation redefining the step is active, 350 after rollback; every mutator moves the epoch. |
| Force GC and nested callback entry | Script `gc-nested` | Identical checksum 21,500 over 200 iterations with an explicit collection every seventh iteration and a native sort invoking a Birnam key block every iteration; every guarded access hit. |
| Suspend and resume on a fully resumable path | Suite (`portable-resumable`, `stackless-image`, `stackless-control` smokes in the release gate set run on the ordinary build; module suites on the dev prefix with the guarded emission) | See §1.1. |
| Deliberate native barrier and foreign-owner attempt | Test suite under the feature (existing barrier and owner tests) | Unchanged: the guard adds no path around `require_driving`, `assert_owner`, or the delimiter barrier; a barrier crossing still fails as before. |
| Invalid slot/action/lifecycle input | Test | Raw slot, out-of-range index and non-Integer operand answer nil on both paths; before a drive the guarded read fails to nil exactly as the send. Action and lifecycle validation are untouched. |
| Repeated invalidation, retirement, abandoned execution | Script `churn` | Identical checksum 2,015,000 and 2,000 calls with a method redefinition on every iteration; each redefinition moves the epoch and forces re-verification, and every access still hit. Elapsed 4.07 s baseline versus 4.24 s candidate, dominated by the redefinitions themselves. The guard holds no registrations, roots or leases, so nothing accumulates. |

### 1.1 Suspension coverage

The four lifecycle smokes from the release gate set were run on the dev
prefix with `BIRNAM_DEV_GUARDED_SLOTS=1` exported, so every unit they compile
in-process (candidates, portable Blocks, forced-source recovery of the
production control unit) used the guarded emission, with guard counters
appended to a file from every process:

| Smoke | Result | Processes counted | Guarded accesses (all hits) |
| --- | --- | ---: | ---: |
| `stackless-image-lifecycle-smoke` (generation-projected recovery, rooted-origin retirement, image rollback, physical successor reclamation) | passed | 6 | 50,696 |
| `stackless-control-smoke` (G0 Module AOT and forced source recovery of the 100,000-iteration callback loop) | passed | 1 | 28,313,404 |
| `portable-code-binding-lifecycle-smoke` (publication, replacement, self-removal, retirement) | passed | 1 | 19 |
| `portable-resumable-lifecycle-smoke` (eviction survival, quiescent retirement) | passed | 0 | 0 (its materialized Block performs no frame slot traffic in the counted processes) |

The Rust suspension, capture, restore and barrier tests also ran under the
feature (74B validation record). Two observations: (1) with the counters
reporting on stderr (`…_COUNT=1`) the image and code-binding smokes fail
because they own stderr, which is why counting can now target a file; (2) the
image smoke persisted guarded units into image artifacts inside the dev
prefix's temporary directories. Such an image binds the dev symbols and cannot
load on an ordinary runtime, which is the containment Design 74 §7.1 asks for,
and no such artifact may be published.

## 2. E3 — frozen protocol and criterion

Frozen before any E3 measurement was taken, as Design 74 §7.5 requires.

**Continuation criterion.** All E2 rows pass (§1), and the guarded candidate
reduces the median warm time by at least 20% on both the empty-body and the
`step:`-body cases at both 1,024 and 16,384 iterations, with each improvement
larger than the larger of the two configurations' run-to-run spread
(max − min of the five pair samples). Cases at 0, 1 and 32 iterations are
reported, not judged. An unexplained regression in any reported case above the
applicable Design 46 tolerance (10% micro) blocks any promotion proposal. The
Objective-C figures are comparative evidence only.

**Prefixes.** Baseline: the ordinary release prefix of the tree at the commit
that froze this protocol. Candidate: the dev-feature prefix of the same commit,
run with `BIRNAM_DEV_GUARDED_SLOTS=1`. Control: the dev prefix without the
variable, to show the feature's presence costs nothing. Install receipts
(commit, source manifest digest, cargo invocation, toolchain) are recorded.

**Runs.** For each size, at least five fresh-process pairs in alternating order
(baseline first on even pairs, candidate first on odd pairs). Each process
reports compile-and-activate time, a cold first run, and the median of five
warm repetitions for each body; the fixture's ready marker separates loading
from timing. All samples are kept in the JSON record; medians are reported.
Runs use the default GC; an explicit-GC correctness variant is the E2
`gc-nested` case; the retained no-auto-GC diagnostic
(`bench/control-throughput-probe.birnam`) runs separately on both prefixes.

**Host.** AC power and the unchanged `scripts/bench-host-check.sh` preflight
before each pair; a pair that cannot obtain a quiet window within the wait is
retried, and if the gate refuses, timing is left pending rather than taken
noisy. Peak RSS comes from one extra unmeasured run per configuration and size.

**Instrumentation.** Guard counters come from one separate instrumented run per
size; the overhead of counting is reported by timing the candidate with and
without `BIRNAM_DEV_GUARDED_SLOTS_COUNT=1`.

**Generated code.** The candidate's compiled unit is inspected with `nm` and
`otool`: it must import the two dev symbols and the ordinary `birnam_send`,
and the baseline unit must import neither dev symbol.

**Objective-C references.** Built on the same host with the recorded clang
version at `-O2 -fobjc-arc`: a message version whose loop object sends
`-body:` to itself, `-step:` to a receiver and `-add:` to an accumulator per
iteration (three `objc_msgSend` dispatches, no cached IMPs; a `BOXED=1` variant
boxes each step result in an `NSNumber`), and a primitive-integer C loop at
`-O2` with an opaque step function. Representation differences are stated with
the numbers.

## 3. E3 measurements

Taken 2026-09-09 after commit `228b85fa` froze §2, on `Mac15,12`, both
prefixes from that commit's tree (source manifest `8217f70ef39f…`, rustc
1.95.0, release profile), the candidate built with the three
`dev-guarded-slots` features and run with `BIRNAM_DEV_GUARDED_SLOTS=1`, the
control being the same dev prefix without the variable. Five alternating
fresh-process pairs per size after a `bench-host-check` preflight; every
sample, receipt, counter and inspection is in
[`74c-e3-record.json`](../measurements/74c-e3-record.json). Every run's
receiver answer and checksum matched; every guarded access hit.

### 3.1 Warm medians (microseconds for the whole loop; spread = max − min of five)

| n | body | baseline | candidate | control | Δ | Δ exceeds spread | ≥20% |
| ---: | --- | ---: | ---: | ---: | ---: | --- | --- |
| 0 | empty | 16 (3) | 12 (2) | 15 | −25.0% | yes | reported only |
| 0 | step | 22 (4) | 18 (4) | 21 | −18.2% | no | reported only |
| 1 | empty | 41 (6) | 35 (13) | 40 | −14.6% | no | reported only |
| 1 | step | 55 (6) | 46 (12) | 53 | −16.4% | no | reported only |
| 32 | empty | 778 (61) | 605 (52) | 808 | −22.2% | yes | reported only |
| 32 | step | 1,016 (79) | 796 (59) | 1,051 | −21.7% | yes | reported only |
| 1,024 | empty | 25,545 (650) | 21,183 (2,427) | 25,383 | −17.1% | yes | **no** |
| 1,024 | step | 32,628 (917) | 25,989 (3,390) | 32,058 | −20.3% | yes | yes |
| 16,384 | empty | 499,236 (41,356) | 433,231 (43,335) | 487,800 | −13.2% | yes | **no** |
| 16,384 | step | 698,875 (45,626) | 602,785 (10,358) | 701,657 | −13.7% | yes | **no** |

Cold first runs and compile-and-activate times track the warm figures
(compile ≈ 0.52–0.55 s in every configuration). The control matches the
baseline within spread at every size, so the feature's presence costs nothing.

### 3.2 Verdict against the frozen criterion

**Not met.** The guarded candidate is faster at every size and every
improvement at 32, 1,024 and 16,384 iterations exceeds the run-to-run spread,
so the gain is real, but only the `step:` body at 1,024 reaches the frozen 20%
and the 16,384 cases sit at 13%. No reported case regressed. Under Design 74
§7.6 this is the "report the outcome and review the next hypothesis
separately" branch, not authorization for anything further.

### 3.2a Operator relaxation (recorded after the frozen verdict)

Later on 2026-09-09 the operator withdrew the percentage threshold (Design 74
§7.5 amendment): a proven-equivalent change that regresses nothing and
improves by more than its run-to-run spread is accepted at any size. Every
judged case passes that test (−17.1%, −20.3%, −13.2%, −13.7%, each exceeding
its spread), so E3 is met as amended. The frozen verdict above stands as the
record of what the pre-registered criterion produced; the relaxation is an
operator decision about what to do with a real 13–20% gain, not a
re-measurement. The §3.5 hypothesis is being written up and reviewed first
([Design 74D](74d-e4-slot-index-box-elision.md)); the §7.6 production slice
proposal is deferred behind it.

### 3.3 Why the gain is bounded

Guard counters show 24,478,438 guarded accesses for 16,384 iterations, about
1,494 checked slot operations per loop iteration, on top of the source sends,
action publications and driver transitions. Each of those operations still
allocates a boxed Integer for its slot index (`MKV`) and interns its selector,
exactly as the baseline does, because E1 deliberately preserved operand boxing
to isolate dispatch savings. Removing dispatch saved 4–6 µs of a 26–37 µs
iteration; the remainder is allocation, collection, publication and driver
work that the guard does not touch. That is the "allocation remains the
dominant cost" outcome §7.6 anticipated.

Instrumentation overhead is not measurable (counting on: 25.4–26.0 ms at
1,024 and 603–606 ms at 16,384 for the step body; counting off: 26.2–27.9 ms
and 599–617 ms). Peak RSS moves within ±6% across configurations with no
direction (baseline 245 MB, candidate 253 MB, control 256 MB at 16,384). The
retained no-auto-GC control probe, which drives the production AOT control
unit in both prefixes, measured 5.08–5.15 s baseline and 4.94–5.08 s candidate
for its 100,000 iterations, as expected for an artifact that carries the
baseline emission in both. Inspection: the candidate unit imports both dev
symbols and `birnam_send`; the baseline unit imports neither dev symbol.

### 3.4 Objective-C distance

Same host, Apple clang 16.0.0, `-O2 -fobjc-arc`, five runs, medians, for
16,384 iterations:

| Reference | empty µs | step µs | Birnam candidate ÷ reference (step) |
| --- | ---: | ---: | ---: |
| Objective-C message version (three `objc_msgSend` per iteration, unboxed longs) | 26 | 96 | ≈ 6,300× |
| Same with `NSNumber` boxing of every step result | 26 | 309 | ≈ 1,950× |
| Primitive-integer C loop, opaque step function | 4 | 13 | ≈ 46,000× |

Representation differences: the Birnam iteration performs roughly 1,500
checked frame-slot operations, about ten late-bound sends, action publication
and driver transitions, and allocates a box per slot operation; the
Objective-C loop performs three dynamic dispatches on a C stack with no frame
machine, no reflection, and (unboxed variant) no allocation. `objc_msgSend` is
the actual dispatch in the reference binary (verified with `otool`, nine call
sites); no IMP was cached by hand. These are comparative evidence for the
size of the gap, not a promise of parity.

### 3.5 Next hypothesis (proposal, not authorized)

Elide the slot-index box on the guard's hit path: the index is a compile-time
constant that only ever reaches `oopAt:` as an argument, the guarded direct
path decodes it back to an integer immediately, and the box cannot escape or
be observed on that path. Creating the box only on a miss (before the
fallback send, so the send's operands are unchanged) preserves every 74A
argument while removing ~1,500 allocations per iteration in the fixture. It is
precisely the allocation question §7.1 set aside, so it needs its own short
legality note and measurement under this same protocol before anything else.
Selector interning per access (74A §6.2) is the second, smaller candidate.


## 4. Findings to route

- Overriding `oopAt:` on `Machine` from Birnam with a constant-answering
  method aborts the process with a native stack overflow in the ordinary
  runtime. The Machine microcode is metacircularly visible by design (60B §1),
  but this failure mode is a fatal abort rather than a kinded failure; the 60B
  owners should decide whether a Birnam-level microcode override deserves a
  kinded rejection.
