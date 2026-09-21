# 80K — Second concurrency review through an Ingalls-inspired lens

> **Independent adversarial review, 2026-09-16.** Historical lens interpretation,
> not a statement by Dan Ingalls or an endorsement. Findings are review proposals,
> not accepted amendments or implementation authorization.

## Fixed input and lens

Reviewed commit **`f6bbe471ec396a0916c7f8011ff80ae112c3053b`**:
[80](80-concurrency-domains-and-structured-tasks.md),
[80A](80a-concurrency-api-sketches.md),
[80G](80g-concurrency-task-lifecycle-exercise.md), and
[80H](80h-concurrency-review-edge-cases.md), with the accepted dispositions in
[80F](80f-concurrency-review-synthesis.md). All line references below identify
that commit. Targeted source and 60A/60P checks also refer to that baseline.

This reuses [80D's sourced lens](80d-concurrency-ingalls-review.md#sources-and-lens):
small comprehensible mechanisms, ordinary message protocols, useful live
inspection, and sound storage management. The analysis below is this reviewer's
application of that lens; it adds no historical attribution or external claim.

The user's stronger malleability preference governs this review. A replacement
scheduler may stall its domain; a replacement cleanup protocol may break its own
application. No finding requires policy certification, frozen methods, watchdogs,
native-library admission, or worker hot reload.

## Verdict

**The revised semantic direction is coherent enough to retain. Two execution
bridges need concrete design before the affected implementation.** Neither is
evidence that the accepted domain or cleanup model is impossible. Both are
explicitly acknowledged engineering gaps whose smallest useful next artifact is
a transition trace, not another broad framework.

The revision materially improves the first review: synchronous scheduler
selection, native-callback rejection independent of result readiness, cancellation
shielding, and debugger pause ownership now have comprehensible contracts.

Severity below distinguishes **pre-implementation** requirements for the affected
mechanism from **deferrable details**. It does not block review or demand that all
mechanisms be implemented before the design can be accepted.

## 1. Pre-implementation — Cancellation must leave a suspended Call without executing its normal successor

**Anchors:** [80G](80g-concurrency-task-lifecycle-exercise.md), lines 23–26 and
91–94; [80H](80h-concurrency-review-edge-cases.md), lines 81–86 and 99–104.

The new trace correctly requires that a cancelled receive stop waiting, skip the
remaining body, and enter cleanup that may itself wait. The existing suspension
contract does not yet provide that transition:

- [60P](60p-v2-control-completion.md), lines 13–29, stages a result-bearing Call.
  Its visible PC is the successor while predecessor roots remain active.
  Resumption supplies a result and commits copies, clears, and successor roots.
- [execution.rs](../../rust/birnam-runtime/src/execution.rs), lines 2290–2300, performs
  that normal result-bearing resume. Its `fail` path, lines 2304–2319, clears
  drive/delimiter and suspended-result state and marks the Machine failed.
- [runtime.rs](../../rust/birnam-runtime/src/runtime.rs), lines 4101–4120, uses that
  `fail` path for the decoded Abort action. It does not execute language cleanup.

**Concrete trace:** A is suspended in `receive`, with a pending V2 Call edge and
a resource reachable from retained execution. Cancellation claims its wait.
Cleanup must now close the resource and can suspend on a different operation.
Resuming A with a fabricated `nil` or Error can commit the ordinary successor
and run application code after `receive`. Calling `fail` removes control state
needed for resumable cleanup. Jumping to cleanup by changing the PC alone skips
the root-map/edge contract. All three are tempting shortcuts; none implements
the agreed behavior.

**Smallest remedy:** specify this one transition from pending Call to task-control
exit: who retains the pending edge and cleanup records; how normal continuation
is bypassed; which root state remains valid; and how cleanup gets a legal new
Call/suspension edge. Include nested cleanup failure and release of the old edge
after its values are no longer needed. The solution may be an ordinary control
protocol over reviewed mechanisms; this finding does not prescribe a native
scope engine or a new serialized Frame format.

**Classification:** remaining first-review control concern, substantially narrowed
by 80H's native-boundary rule. This is a missing mechanism contract, not a request
to reconsider the accepted cancellation/result semantics.

## 2. Pre-implementation — Name the strong owner during queue and driver handoffs

**Anchors:** [80G](80g-concurrency-task-lifecycle-exercise.md), lines 21–28 and
136–145; [80](80-concurrency-domains-and-structured-tasks.md), lines 189–192 and
629–635.

The text requires live execution to remain rooted, gives waits distinct
identities, and releases incompatible borrows before invoking scheduler policy.
It still does not identify the strong tracing owner through the transfer itself.
Lifecycle ownership by a scope is not yet an explicit statement that the scope
strongly traces the task's Machine throughout every unfinished phase.

**Concrete trace:** a receive notification claims A's wait. The supplied runtime
or channel removes the wait entry, then invokes the ordinary ready-queue method.
That send allocates and GC runs before insertion. If the wait entry was the last
strong owner of A's execution, its Frames and cleanup state are unrooted during
the handoff. An analogous gap exists when taking a selected task out of the ready
queue before installing a drive root. This requires only an ordinary allocating
policy method, not a malicious override.

The existing scoped host-root mechanism in
[execution.rs](../../rust/birnam-runtime/src/execution.rs), lines 3460–3477, proves
that explicitly registered drive roots are traced. It does not, by itself,
establish retention before registration or after the drive root is removed.

**Smallest remedy:** choose and state one ownership arrangement for the supplied
protocol. For example, a live scope can strongly trace each unfinished task and
its Machine across waiting, ready, running, cleanup, and debugger pause; queue
entries then need not be the sole owners. Alternatively, retain a temporary
handoff root before removing the previous owner and release it after the next
owner is established. Trace one cancellation/readiness/GC interleaving and mark
the old wait identity invalid before it can target a later cleanup wait.

Also state when terminal evidence stops retaining execution. Permanent strong
registry entries are not a substitute for an ownership decision. These facts can
be expressed through ordinary traced Birnam objects; the runtime need not certify
arbitrary replacements of those objects' policies.

**Classification:** remaining first-review rooting concern. Wait identity and
stale-event semantics are improved; the exact strong-owner handoff remains open.

## Disposition of the earlier Ingalls concerns

| Earlier concern | Round-two assessment |
| --- | --- |
| Where scheduler policy executes | **Narrowed.** Synchronous select-and-return and borrow release resolve the policy question. The concrete drive handoff must establish the new task's entry scope; 80 lines 242–246 already require that. No independently preemptible scheduler or policing of nontermination is required. |
| Failure and cancellation control | **Narrowed; remaining bridge in finding 1.** Ordinary Error values, explicit task failure, cleanup behavior, and native-entry rejection are now separated. The ready-versus-pending callback rule in 80H lines 93–104 removes the timing-dependent legality problem. |
| Debugger inspection deadlock | **Resolved at the default-policy level.** Structural inspection executes no methods; peers may run; cancellation and disconnect retain the pause and its ownership. Explicit evaluation can block and is the user's responsibility. The accepted native eligibility limits still apply to pause/resume. Exact inspector presentation is deferrable. |
| Root ownership through transitions | **Remaining in finding 2.** Distinct waits and preserved pins are stated, but strong tracing ownership across queue/driver transfers needs one explicit trace. |
| Domain locality in inspection | **Narrowed.** Worker-instance-bound handles, unknown remote outcomes, and restart-based updates remove the misleading transparent-worker interpretation. Displaying code provenance and a detailed retained-by view is deferrable; no live-upgrade compatibility machinery is needed. |

## Review limits

This was a document and targeted source review, not an execution test or a proof
of GC correctness. No implementation, release plan, historical review, or frozen
input was edited. The two findings concern the supplied implementation's concrete
execution obligations, not protection against users changing their own semantics.
