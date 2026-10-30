# Send-reduction validation failures: diagnosis and remediation

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

Date: 2026-10-08. Investigated checkout: `172a43fc`.
Status: diagnosis and recommendations; production fixes are not implemented here.

The failures have several causes. Generation adoption falsely treats collection of
unrelated private Block behaviors as a source/binding change. ImageFormat loses its
intended rejection error because validation does not stop dependent parsing. The
image-reclamation test compares different resident-code states. Multicore deadlines
are sensitive to a worker workload that checks the clock only every 1,000 costly
increments. Documentation exercises quadratic sorting with expensive authored
String comparison. These findings do not establish a failure of process isolation
or justify bypassing Id, restoring native Integer policy, or dropping necessary roots.

## Evidence and scope

The existing published test prefix was checked against the worktree: changes since
its build are test/script changes, not production Rust or Birnam changes. Diagnostic
copies are explicitly unqualified installations. Experiments modify one condition
at a time; temporary instrumentation is removed from production files at completion.

The [measurement receipt](../../measurements/send-reduction-diagnosis-20261008.json)
records source identity, outcomes and hashes of local evidence. Raw logs and
throwaway reproducers live under `.cargo-target/diagnosis/20261008/`.

This investigation distinguishes a reproduced cause from attribution to a particular
optimization commit. The guard, parser structure and sorting algorithm predate the
latest send-elimination commits. Changes to allocation/reachability and the cost of
authored Integer execution can expose those assumptions. A full historical image
bisection has not been performed; specific-commit regression claims would be premature.

## 1. Valid generation adoption rejected after unrelated GC

**Confirmed runtime false rejection.** The compiled-unit fixture fails during
`DispatchGeneration initializeCurrent`. A one-module setup succeeds; the original
five-module setup reproduces the failure, including with precise payload tracing.
No image save is needed.

Generation captures the global Id behavior epoch before inspecting managed bindings.
During inspection of the `modules` component, automatic collection retires seven
behaviors, all children of `BlockVt`. Id increments its global mutation epoch for
retirement. The instrumented run records:

```text
snapshot start=48503
retire 7 private Block behaviors: epoch 48503 -> 48504
snapshot end=48504; binding_match=Some(true)
behavior epoch changed during generation adoption
```

The one epoch increment is accounted for by retirement, rather than a dictionary
binding replacement. The managed bindings still match. Turning off automatic GC
for this diagnostic adoption makes it succeed. This is an experiment, not a remedy.

The source already acknowledges this problem during authority parsing, but moves the
epoch observation only past that phase. Collection can also occur during the later
binding inspection. See [adoption](../../../rust/birnam-generation/src/candidate_ledger.rs),
[matching](../../../rust/birnam-generation/src/candidate_ledger.rs), and
[Id retirement](../../../rust/birnam-id/src/id/vtable.rs).

**Recommended remediation:** replace adoption's global-epoch requirement with a
coherent observation of its actual dependencies. Id should supply revision facts for
relevant behaviors/bindings and associations through its message interface. Preserve
Store basis/source-registry checks, exact target/entry/global comparisons, current
lookup/permission dependencies, and rejection of relevant mutation followed by restoration
(ABA). A final equality comparison alone does not detect ABA.

Keep the global epoch as a conservative fast check where useful; on unrelated epoch
change, validate relevant dependencies instead of rejecting automatically. The current
code already walks the binding ledger: reuse/batch those facts rather than add a new
check to every ordinary send. Do not globally stop counting dictionary mutations,
suppress collection, retain dead Blocks, or remove the stale-generation guard entirely.

Acceptance: force retirement of unrelated private behaviors inside adoption and succeed;
replace a relevant binding, change its target association, mutate the registry/Store,
or mutate and restore a relevant dependency inside a callback and reject. Include
custom lookup and callback/service replacement. Validate the guard with TLA+ transitions
for unrelated retirement versus relevant mutation/ABA, then relate dependency revisions
to the selected bindings in Lean. Those models must cover the implemented observation
protocol; the existing abstract rooting proofs do not prove this guard.

## 2. ImageFormat rejection error is lost

**Confirmed error-path defect; the previous validation turn misclassified it.**

```birnam
let answer := [ImageFormat inspect: ["short" asBytes] architecture: 1 abi: 0];
[System println: [answer kind]];
[System println: [answer message]]
// Current result: #nativeInvalidRequest, nil.
```

ImageFormat's declared validation path constructs `#imageRejected` with context.
Calling `require: false context: "sentinel"` on a request worker produces that exact
error; inspecting with the already-failed worker returns the same error and message.
The defect occurs when the worker starts healthy and fails partway through parsing.

`checked:` checks the failure cell before and after its body, but does not unwind
that body. `inspectBytes:architecture:abi:` records a short-header rejection, then
continues into parsing and consumes helper results as byte buffers, indices or
other success values. The first traced native failure is `CodeObject inspectInventory:bindings:selection:`
with status 1/detail 0: the parser reaches inventory inspection after rejection with
invalid arguments. That failure escapes before the final failure-cell check.
Ordinary language execution converts it to `#nativeInvalidRequest`; native callers
receive the native failure directly. Image's owner distinguishes `#imageRejected`
from genuine native failures, so the wrong error also changes load diagnostics.

See [parser and request control](../../../modules/image-format.birnam) and
[ImageFormat result admission](../../../rust/birnam-image/src/format.rs).

The 2026-10-07 changes to `native_compilers.rs` and corrupt-image CLI expectations
accepted generic native failure instead of investigating this distinction. The frozen
format test checks only `isError`/acceptance, which lets the wrong error pass. Those
weakened expectations should be replaced with specific error-contract tests when fixed.

**Recommended remediation:** stage the authored parser so a failed validation prevents
execution of dependent reads/conversions. Return the original request-local error,
including its context. Guard stage continuations rather than introduce another closure
or failure check for every field on the valid path. A small number of stage gates is
preferable to per-field wrapping or a new native whole-format validator.

Preserve genuine owner/native failures unchanged. Do not relabel every native failure
as an image rejection; that would hide broken providers and mapping/compilation failures.
No ABI bridge restoration or foreign-payload access is needed.

Acceptance: exact `imageRejected` kind/context for truncated headers, bad magic/version,
checksum/refusal boundaries; request-local isolation and earliest-error retention; native
provider failures retain their original status/detail. Restore native-caller and CLI
checks, and retain the successful frozen-byte round-trip matrix. Use a small control-flow
model asserting that rejection is absorbing and dependent reads are skipped. A Lean
result-composition lemma can describe first-error preservation; implementation-level
negative tests are still needed.

## 3. Image-reclamation baseline compares different core states

**Confirmed test-baseline defect; 85 versus 192 is not by itself a fixture leak.**
The failure reproduces without an image, fixture compilation, or historical-fact loop.
The current harness first removes original Module methods, measures 192 units, and
reloads Module. That produces 214 units before the experiment. Then:

| Cleanup after harness initialization | Live units |
| --- | ---: |
| None, followed by collection/checkpoint | 214 |
| Evict materialization cache only | 214 |
| Clear Store facts only | 85 |
| Clear Store facts and evict cache | 85 |

The image test performs Store clearing at the end, while `CompiledFloor::pin_lang_only`
records its baseline before that clearing. Store clearing removes code-retaining references that were present at baseline capture;
we have not classified every retired mapping by source. Removed manager definitions and
their metadata are a likely contributor. The cleanup releases resident mappings included
in the earlier baseline. This happens without any snapshot.

**Recommended remediation:** compare equivalent resident states. Establish an independent
no-fixture cleanup control, and track temporary mapping identities using TCC's existing
`mappingIdentityOf:` / `mappingBytes:` messages. Verify that fixture mappings retire and
required resident code remains. Do not change equality to `<=`, arbitrarily replace 192
with 85, pin obsolete code to satisfy the count, or clear required metadata before saving:
that would weaken the test or alter the setup it is meant to exercise.

Remaining scope: full image roundtrip/resave qualification still requires completing its
checks after repairing the baseline. This experiment does not prove every image lifetime.

## 4. Multicore failures are deadline-sensitive workloads

**Confirmed timing sensitivity; no demonstrated process-isolation corruption.** The
previous two complete runs failed five of 27 checks with request-deadline errors. The
native pending transport uses an absolute wall deadline, checked before socket progress.
A cooperative/native pause or an oversized worker batch consumes that budget normally.

The worker targets 500,000 microseconds of activity but checks its clock only after
1,000 `Cell inc` operations. These use the current Integer implementation. The test
also performs synchronous `System gc` while asynchronous requests are pending.

Controlled current-prefix experiments:

| Variant | Sequential time | Parallel/root time | Result |
| --- | ---: | ---: | --- |
| Original fixture, current rerun | 5.405 s | 4.045 s | 27/27 |
| Remove only the ticker's explicit GC | 5.370 s | 4.010 s | 27/27 |
| Keep GC and instrument its duration | 5.411 s | 4.003 s | 27/27; GC 80.731 ms |
| Change only worker batch 1,000 -> 100 | 1.133 s | 1.920 s | 27/27 |

The 100-increment run returns 200 increments per worker and approximately 0.646/0.641
seconds of worker CPU. The nominal half-second task has substantial batch overshoot.
The GC measurement rules out claiming that this observed collection alone consumed
five seconds. The unmodified fixture now also passes on this host. Earlier failures remain real timing
failures; their exact historical
pause/worker-time split was not recorded. Removing GC did not change cancellation,
zero-budget, single-outstanding-request, transport failure or cleanup checks.

**Recommended remediation:** shorten the workload's clock-check batch and separate
bounded concurrency responsiveness measurements from forced-GC rooting/cancellation
stress. Give the latter an explicit, appropriate test deadline. Preserve production
absolute deadlines, zero-budget refusal before transmission, cancellation ownership,
uncertainty on failure, and one request per worker. Do not accept late replies merely
because their bytes are queued, silently extend production timeouts, or require an idle
machine. Use small independent transport/deadline models and injected pauses for
correctness; CPU overlap ratios are measurements, not universal scheduling guarantees.

If more performance work is required, measure post-image execution separately from
synchronous direct-Id Integer microbenchmarks; one cannot substitute for the other.

## 5. Long Doc and image stress checks

A fresh, bounded `docgen` sample places all 742 sampled stack traces under native
`Vector sortBy:` and its authored comparison callbacks, including collection. The
previous full Doc-suite sample has the same sorting/collection pattern. The current
Vector implementation evaluates keys once, then uses stable insertion sort:
worst-case `n(n-1)/2` current `<` messages. String `<` is authored byte ordering;
its Range/inject loop keeps visiting the entire shared-length range after its verdict
is decided. It skips later byte comparisons but still pays loop/index/Integer costs.
This compounds quadratic sorting with expensive authored control/arithmetic. Both
algorithms predate the latest send-reduction slice.

**Recommended remediation:** optimize the String comparison path and Doc's source/name
work before more compiler plumbing. Reuse existing source metadata instead of repeatedly
recovering selector names from source text where the owner can expose those facts by Id.
Investigate stable O(n log n) ordering, but preserve key evaluation once and stable output.
Changing the number/order of effectful custom `<` callbacks is a semantic decision:
use an explicit ordering contract or a current-owner purity/permission guard with the
existing path as fallback. An owner-mediated generic byte-order operation is an option;
a private String payload read or native whole-Doc bypass is not.

The Host image-entry test collects on every restored slot through `collecting_send`;
each collection traverses a substantial resident graph. This is intentionally much more
expensive than ordinary loading. Split a small fixture with exhaustive forced-GC callback
coverage from a large production image with bounded, deliberate collection points.
Keep a separate scheduled exhaustive stress gate if needed. Do not report a capped run
as a semantic pass. Full large-image roundtrip completion remains unqualified here.

## 6. Additional low-risk GC improvement found

Id already owns an address index of its registered behaviors. After removing dead
records, `remove_objects` rebuilds the ordered survivor list by testing each entry
against a linear dead-vector search. This is O(B*D). A diagnostic candidate filters
against the already-updated owner index instead: O(B+D), no new Id sends, no extra
foreign data access, and no new dispatch cache.

A release microprobe retires alternating behaviors and checks exact survivor order:

| Behaviors B | Existing median | Indexed median |
| --- | ---: | ---: |
| 1,000 | 142,167 ns | 6,125 ns |
| 2,000 | 451,209 ns | 12,792 ns |
| 4,000 | 1,673,417 ns | 23,834 ns |
| 8,000 | 6,687,208 ns | 53,542 ns |

Three short trials per size; allocation/setup is outside timing. This is evidence for
this owner-local operation, not a 125x whole-GC or startup claim. The candidate is not
retained in production by this diagnostic task. Preserve registry/ordered-list equality,
retirement order, weak-key processing, and epoch notification. A filter-membership proof
plus focused Id/collector tests suffices; no new language protocol is necessary.

## Recommended order and invariant checks

1. Repair ImageFormat's first-error propagation and restore precise negative tests.
   This fixes observable behavior with a few request/stage gates rather than per-field work.
2. Replace generation adoption's overly broad epoch fence with dependency-scoped Id facts.
   Include relevant ABA/service/association mutations; avoid adding per-send overhead.
3. Repair the image-lifetime measurement using mapping identities and equivalent cleanup.
   Keep the native cold-bootstrap seed and prove required mappings survive.
4. Apply the owner-index retirement improvement; reduce multicore batch overshoot and
   split timeout/performance evidence from forced-GC stress.
5. Simplify String/Doc ordering and metadata work, then profile residual execution costs.

Every slice keeps Id as the floor and current messages across owners. No foreign payload
or context inspection, cached foreign invoker, alternate function table, silent image
recompilation, or Rust Integer policy substitute is proposed. Preserve identity/aliases,
shared Cells, supported logical Frames/continuations, native barriers, native/byte/code
lifetimes, genuine failure ordering, and all Rust needed for cold bootstrap. Existing
formal results are scoped models: extend them to each new contract and retain independent
codec/current-message/GC tests. Backward compatibility is not a reason to preserve
obsolete bridges, but it is also not a reason to erase an intended error distinction.

The remaining implementation and broad acceptance work are separate from this report.

## Implemented repair, 2026-10-08

ImageFormat now gates dependent parsing stages and the malformed-image matrix checks
`imageRejected`, rather than accepting any Error. Id's new `bindingEpoch` counts
explicit dictionary/parent/association/weak-binding mutations (including ABA), while
`behaviorEpoch` still includes dead-behavior retirement. Generation uses the former:
this fixes the observed GC false rejection without per-dependency versions or per-send
checks. Both epochs fail closed on saturation in guard consumers. Retirement filters
through Id's existing live index and preserves survivor order.

Image lifetime assertions use an independent no-fixture control with the same source,
mapped or rebuilt resident state. Large restore tests force GC at the first eight and
power-of-two slot callbacks; small restores remain exhaustive. Source String ordering
stops loop work after its first unequal byte, and the multicore fixture checks its clock
after 100 increments. Production timeout policy and native barriers are unchanged.

The fresh installation passed 535 Birnam checks, 57 compiled-unit policy checks,
all 42 native test binaries (including complete image roundtrip and 21 resave epochs),
132 ImageFormat admissions/16 byte-identical roundtrips, native compiler error checks,
image loading/rebuild policy, and 101 Lean theorems plus the bounded TLA+ checks.
Full Doc HTML rendering completed. Doc now uses explicit `mergeSortBy:`; existing
`sortBy:` insertion comparison order remains available and unchanged. The new merge
operation preserves stable ties and evaluates keys once through current Id messages.
The merge regression includes collection during key evaluation.

The Host image gate no longer duplicates the native resave test with a non-metrics
TCC library. Its lifetime/resave coverage is owned by the native gate, which uses the
required resource-metrics build. This is a test-configuration correction, not removal
of the image lifetime checks.

The [repair validation receipt](../../measurements/runtime-repair-validation-20261008.json)
records the final source and log hashes. Installed Host image acceptance also passed:
malformed-image diagnostics, source-free artifacts, mapped/rebuilt StringBuilder and OS
services, Alien wrappers, baked entries, and forced-GC entry lifetime. Rust workspace
finished with 318 passing tests. The published validation prefix is
`/tmp/birnam-fixed-final-20261008/prefix`; the user's installed launcher was not replaced.
The validation receipt records the working-tree state at validation time.
