# 74C — E2 matrix and E3 measurement of guarded Machine slot access

> **Status: E2 delivered and E3 protocol frozen 2026-09-09 under the user's
> authorization of both; the E3 measurements in §3 were taken only after the
> commit that froze §2. Development-only throughout: the E1
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

E3_RESULTS_PLACEHOLDER

## 4. Findings to route

- Overriding `oopAt:` on `Machine` from Birnam with a constant-answering
  method aborts the process with a native stack overflow in the ordinary
  runtime. The Machine microcode is metacircularly visible by design (60B §1),
  but this failure mode is a fatal abort rather than a kinded failure; the 60B
  owners should decide whether a Birnam-level microcode override deserves a
  kinded rejection.
