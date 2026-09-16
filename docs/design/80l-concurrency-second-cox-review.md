# 80L — Second concurrency review through a Brad Cox-inspired lens

> **Independent adversarial review, 2026-09-16.** Reviewed baseline:
> `f6bbe471ec396a0916c7f8011ff80ae112c3053b`.
> Inputs: Designs 80, 80A, 80G, and 80H at that commit; 80F supplies accepted
> dispositions and 80E supplies the earlier review. All line references below
> identify that fixed baseline, not subsequent working-tree revisions.
> This is an interpretation of the component-contract lens used in 80E, not
> impersonation or endorsement by Brad Cox. No new historical claims are made.

## Verdict

**Retain the direction. Clarify the supplied serial worker's request boundary and
control-message progress before implementing that endpoint.** Two related gaps
remain in an otherwise substantially improved component contract. Neither calls
for a distributed framework, native-library certification, restrictions on user
redefinition, or worker hot reload.

The revised distinction between local observation, remote execution outcome, and
worker lifetime is sound. The documents no longer imply that a transport error
proves remote cleanup or that process termination and thread shutdown have the
same meaning. No demonstrated violation of the existing owner, GC, Frame, or
ABI invariants follows from the architecture itself. This is a document review,
not evidence that the future implementation preserves those invariants.

## 1. A serial worker needs an explicit per-request failure boundary

**Severity: pre-implementation blocker for the supplied worker endpoint.**

**Anchors:** [80 §8](80-concurrency-domains-and-structured-tasks.md), lines
546–552, defines a worker as a task/service that handles a whole request before
receiving the next; [80H §2](80h-concurrency-review-edge-cases.md), lines 81–86,
makes explicit failure exit the task; [80A §2](80a-concurrency-api-sketches.md),
lines 76–80, owns submitted operations through the domain. The failure envelopes
in 80H lines 108–119 do not define which execution terminates to produce them.

**Concrete trace:** W accepts requests R1 and R2. Its serial handler starts R1.
R1 explicitly fails, or is deliberately cancelled while waiting. Cleanup finishes.
If the handler runs as the worker task's body, task-control exit can terminate W
before it receives R2. If each request is a child of an ordinary worker scope,
terminal R1 failure invokes that scope's fail-fast policy. Merely observing its
outcome does not undo this. Alternatively, a request boundary could collect R1's
outcome and let the service process R2. These are observably different contracts,
and the current text does not select one or settle R2 when the worker stops.

**Smallest remedy:** add a short lifecycle table distinguishing request failure,
request cancellation, service failure, and owner cancellation. Identify the
execution boundary that owns each request's cleanup and publishes its result.
My recommendation is that the serial service collect each request's settled
outcome before selecting the next request; an individual request's failure alone
need not destroy the service. That requires an explicit request boundary rather
than accidentally applying the ordinary scope's fail-fast policy to the service
loop. The caller's owning scope can still independently cancel the entire domain.

For service shutdown or failure, specify how every accepted-but-unstarted request
settles and whether it was never executed. Graceful drain and owner cancellation
may deliberately differ. A few sentences are sufficient; this does not require
designing a general supervision API or changing ordinary local scope semantics.

## 2. Serial request admission must not serialize cancellation behind the request

**Severity: pre-implementation blocker for cancellable worker requests.**

**Anchors:** [80 §8](80-concurrency-domains-and-structured-tasks.md), lines
514–516 and 546–552, combines bounded admission with whole-request serialization;
[80H §3](80h-concurrency-review-edge-cases.md), lines 125–140, relies on owner
cancellation and owner-loss detection reaching outstanding work. The distinct
task-control path in 80 lines 345–357 does not by itself specify transport ingress
or delivery to a serial worker whose current request has suspended.

**Concrete trace:** W is handling R1, which awaits an ordinary asynchronous event
that will not arrive. Its request queue is full with R2. The owner cancels R1 or
W. If cancellation uses the same bounded admission path as another application
request, it cannot enter the queue. Even with spare capacity, putting cancellation
behind R1 in the serial handler loop prevents it from being acted on until R1
finishes. The cancellation that should release the wait cannot reach that wait.
This occurs with the supplied policy and eligible task suspension, without any
hostile edit, blocking C call, or network partition.

**Smallest remedy:** state that lifecycle control is accepted and recorded by the
owning domain independently of the serial application's request queue and its
capacity. An ingress/reactor task can demultiplex control and request traffic over
the same connection; a separate socket or OS thread is not required. Application
requests remain serial, and cancellation still takes effect only at eligible
boundaries. Network loss, domain monopolization by native code, and a deliberately
paused debugger task retain their documented limitations. This is a progress
condition for the supplied endpoint, not a guarantee against arbitrary user
replacement policy.

## Disposition of the first review's concerns

| Earlier concern | Round-two disposition |
| --- | --- |
| Ordinary Error values change meaning through the legacy isolate boundary | **Resolved at the design level.** 80H lines 108–119 requires separate outcome envelopes, admitted application-data encoding, honest result-transfer failure, and a distinct compatibility boundary preserving the shipped isolate API. Exact evidence encoding remains engineering work; arbitrary Error objects must not be smuggled into the codec. |
| A locally settled request leaves ownerless remote work | **Resolved at the ownership-contract level.** 80H lines 132–148 retains the original lifecycle obligation and permits the owner to remain unfinished. Instance-bound reconciliation details can be deferred; no automatic ownership transfer or replay is implied. |
| Native module admission/qualification | **Rejected scope, not a remaining finding.** The user owns native/application statics. Birnam-owned runtime routing, roots, callbacks, and code lifetime still require correct domain ownership, as 80 lines 53–58 and 601–606 explicitly acknowledge. |
| Live edits need automatic component compatibility management | **Rejected scope and narrowed by restart.** Worker-instance-bound handles and no forwarding/replay cover the execution concern. Application protocol compatibility belongs to the user. |
| Outcome observation is not an expected-error recovery boundary | **Resolved for the example.** 80H lines 50–68 demonstrates an ordinary Error value handled by the scope body without triggering task failure. The distinct service/request boundary in finding 1 is still needed for reusable serial workers. |

## Placement and evidence

The process/thread distinction is appropriately explicit: 80H lines 143–148 says
confirmed process exit proves execution ended, not that language cleanup ran;
thread hosting needs real settlement/join evidence. Design 80 lines 608–611 also
preserves the difference in native-crash containment. Keep these qualifications
when implementing the endpoint; no additional placement certification scheme is
required by this review.

Reviewed the fixed documents for contract consistency and traced the two cases
above. No implementation, runtime tests, release plan, or edits to review inputs
were performed.
