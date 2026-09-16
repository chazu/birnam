# 80H — Concurrency edge cases for the second review

> **Review input, 2026-09-16.** Read with [80](80-concurrency-domains-and-structured-tasks.md),
> [80A](80a-concurrency-api-sketches.md), and
> [80G](80g-concurrency-task-lifecycle-exercise.md). The user accepted draining
> channel closure, no undo of committed transfers, worker-instance-bound handles,
> and recording cancellation without silently resuming debugger-paused tasks.
> This document supplies concrete traces. Additional choices marked **draft**
> are reviewable proposals, not separate user approvals or implemented features.
> Exact public selectors and cleanup-control representation remain tentative.

## 1. Channel closure and cancellation

Use a local channel with capacity one. Values retain local identity. An accepted
buffered send transfers responsibility to the queue; an unbuffered send commits
only when paired with a receiver. Entering a blocked-sender queue is not acceptance
of its value. Send success never means application processing has finished.

| Event | Observable result |
| --- | --- |
| Sender A sends `one`. | Accepted; the buffer holds `one`. |
| Sender B sends `two`. | B waits with its value still unaccepted. |
| The channel closes. | New sends are rejected; B resolves with a closed-channel result, not success. |
| Receiver C receives. | C gets `one`. |
| C receives again. | End-of-stream, distinct from the ordinary value `nil`. |

For capacity zero, a waiting sender has no accepted buffered value to drain.
Closing before pairing resolves that sender as closed and waiting receivers as
end-of-stream. Closing after a committed pairing does not retract the transfer.

The supplied channel resolves each pending operation once in the owning domain's
order. If cancellation wins before admission/pairing, withdraw the send/receive
without consuming a value. If transfer wins, subsequent task cancellation does
not undo it. If close wins an unaccepted send, its operation result is closed;
the task's final outcome still follows its own cancellation/publication rule.
These are separate channel-operation and task-lifecycle outcomes.

**Draft defaults:** close is idempotent; buffered values drain in acceptance
order; simultaneous contenders have no promised scheduler-independent order.
An empty closed channel immediately reports end-of-stream. Closing resolves
pending operations so no waiter is forgotten. Queue fairness is ordinary policy.
The supplied channel is not a durable work queue and performs no automatic
redelivery when a consumer is cancelled after receiving a job.

## 2. An Error value, explicit failure, and a native callback

These illustrative snippets use the existing `fork:` sketch. In the second
snippet only, its block receives a proposed task-control argument. This control
argument and `fail:` are candidate API spellings, not additions to the language
grammar or claims that the selectors already exist.

### Ordinary Error value and expected failure

```text
let answer := [Tasks scope: [:scope |
  let decode := [scope fork: [| [Parser parse: text]]];
  let other := [scope fork: [| [Index refresh]]];
  let parsed := [decode await];
  [[parsed isError]
    ifTrue: [| [Fallback parse: text]]
    ifFalse: [| parsed]]
]];
```

Assume `Parser parse:` returns an ordinary Error object for malformed text.
`decode` succeeds with that object; it does not cancel `other`. The scope body
chooses a fallback through ordinary messages and waits for owned work at exit.
Here `isError` is an explicit application decision, not a runtime promotion rule.

### Explicit task failure

```text
let decode := [scope fork: [:control |
  let parsed := [Parser parse: text];
  [[parsed isError]
    ifTrue: [| [control fail: parsed]]
    ifFalse: [| parsed]]
]];
```

In the supplied protocol, `fail:` initiates task-control exit and does not return
to the body. It runs the appropriate cleanup path, settles descendants, and
publishes failure. Ordinary-scope fail-fast begins at terminal failure, as in
Design 80. Observing `decode outcome` afterward does not undo that policy. A
replaceable supervised policy can be explored separately; this example needs
neither one nor automatic catching of arbitrary VM failures.

### Native callback tries to await

A synchronous native comparator invokes Birnam code that sends `job await`.
The surrounding foreign activation has no way to return a suspended task.

**Draft default:** waiting APIs require a task-control-capable calling context,
checked before consulting whether the answer is ready. Reject `await`/`outcome`
through an ineligible synchronous native entry for both ready and pending jobs;
do not make legality depend on timing. An explicit nonblocking observation API
can report ready/not-ready without suspension or task-control propagation.

Rejection follows the existing entry error contract. Where native entry has no
out-of-band error channel and is fatal today, this design does not silently make
it catchable. An explicit asynchronous/resumable adapter can instead arrange
owner-domain notification. Cleanup cannot unwind arbitrary C frames. This
boundary and the new language cleanup path need engineering review against 60A,
60B, and 60P; a selector alone does not provide the mechanism.

### Cross-domain result representation

**Draft endpoint contract:** encode separate success, explicit failure, and
cancellation envelopes using admitted copied values. Transport status is separate.
For a parser that returns a local Error, its remote adapter explicitly produces
application data, for example `{#kind -> #parseError, #message -> "invalid input"}`.
That travels inside a success envelope and does not cancel siblings as task
failure. An arbitrary unencoded Error is nontransferable; report result-transfer
failure with the remote execution/effect status preserved, rather than consulting
`isError` and inventing task failure. No arbitrary Error or closure enters the codec.

The shipped isolate API retains its existing Error-to-application-failure rule.
The new endpoint must use a distinct worker protocol/adapter; merely wrapping
today's blocking request call asynchronously cannot implement the new contract.

## 3. Worker loss, ownership, and replacement

| Event | What the caller and owner can conclude |
| --- | --- |
| Scope S owns worker W1 and submits request R. | R is bound to W1; S owns W1's lifecycle. |
| W1 performs an external write. | The write may already have happened even before a reply exists. |
| The reply connection disappears. | The local request observation can fail with an unknown remote outcome. This does not settle W1's lifetime. |
| S requests cancellation of W1. | Sending the request proves neither receipt nor cleanup. S retains the unresolved lifecycle obligation. |
| W1 detects owner loss. | Under the supplied policy, stop admission and request cancellation of owned work; finish cleanup where possible. Detection has no fixed-time guarantee. |
| A replacement W2 starts. | New requests may target W2. R and all old handles still identify W1. No automatic forwarding or replay. |

There are three distinct records: the local request's observation, the remote
operation's known/unknown outcome, and the owned worker's lifetime. Retaining
these facts does not require a general distributed coordination framework.

**Draft ownership rule:** keep unresolved activity recorded by the existing domain
manager/application owner. A scope that owns W1 cannot complete shutdown merely
because its request handle reports transport failure. The initial model does not
automatically transfer that obligation to a detached service. Partitioned or
uninterruptible work may therefore keep its owning scope unfinished; inspection
must show why. Failure to detect owner loss is not treated as proof of health.

Confirmed worker-process exit establishes that execution in that process ended;
it does not establish language cleanup or undo external effects. Thread hosting
needs supported settlement/join evidence and cannot pretend disconnect killed
the thread. A new connection alone does not settle old work; reconciliation
requires evidence tied to the old instance and request. Wire representation and
reconciliation mechanics remain engineering questions.

## 4. A debugger-paused lock holder during cancellation

| Event | Observable result |
| --- | --- |
| Task A pauses in an explicitly enabled debugger session while holding a lock. | A is visibly paused and unfinished; its lock, Frames, roots, and code pins remain retained. |
| Task B attempts the lock. | B waits. Other runnable tasks continue. Structural inspection shows the dependency on A. |
| The scope deadline expires or cancellation is requested. | Record cancellation. Do not resume A or release its lock implicitly. B's ordinary cancellable wait may exit into cleanup. |
| The debugger disconnects. | Draft default: keep A paused under its existing owner and permit reattachment. Do not infer resume, release, or completion from transport loss. |
| The user explicitly resumes A. | Pending cancellation takes effect at an eligible boundary, respecting any protected update, and cleanup runs. |
| Cleanup and owned work settle. | Publish the outcome under the agreed failure/cancellation/success rule. |

Structural inspection must not execute user methods. Explicit debugger evaluation
is user code and can block or misbehave; the user owns those consequences. A
detached debugger does not make A ownerless. Ending the pause through unwind
requires a supported control path; it cannot mean discarding native frames.

Production scopes do not enter interactive pauses merely because an editor is
connected. This draft does not add a watchdog, lock release on disconnect, or
automatic pause timeout. An application may replace the debugging policy.

## Review focus

Review the supplied defaults and these traces against the accepted malleability
stance: users own the consequences of replacing protocols. Require concrete
ownership, GC, Frame, ABI, or control-boundary evidence for restrictions, rather
than treating every possible bad user method as a missing safety feature.

Separate contradictory semantics or invariant violations from implementation
questions. In particular, test the cleanup/control path, wait-to-ready root
handoffs, queue races, protocol conversion, and independent domain driving.
No source implementation or release plan is authorized by these documents.
