# 80M — Second concurrency adversarial review: consolidated findings

> **Review synthesis, 2026-09-16.** Four independent agents applied the same
> Piumarta-, Kay-, Ingalls-, and Cox-inspired lenses as the first review. These
> are interpretations, not those individuals' participation or endorsement.
> Review findings do not change accepted decisions or authorize implementation.

## Outcome

**All four reviews support retaining the architecture.** Most first-round
semantic concerns are resolved, narrowed to concrete implementation questions,
or explicitly excluded by the user. None demonstrates that parallel owned
domains, Birnam-authored lifecycle policy, or the accepted malleability stance
must be abandoned. This is design review, not proof that an implementation
preserves the invariants.

The remaining work is concentrated in the cancellation/cleanup mechanism,
tracing ownership during transitions, and the supplied serial worker's request
and control boundaries. These must be specified before implementing the affected
parts; they are not reasons to grow a new framework or release checklist.

## Fixed reviewed revision

### Accepted dispositions after review

The user accepted the following bounded remedies and requested they be committed
before working through the cancellation-to-cleanup transition:

- **Cancellation:** explore an ordinary Birnam wait adapter receiving an internal
  tagged wake result through existing resume machinery. It enters task-control
  exit on cancellation before returning to application code. The cleanup/control
  mechanism remains to be worked through; this acceptance does not claim it exists.
- **Tracing ownership:** rooted application/service owners retain scopes, which
  strongly trace all unfinished tasks and their execution/cleanup state across
  queue transfers. Release execution-specific retention after settlement.
- **Serial workers:** one default dispatcher per initial domain handle; each
  request has a separate task/scope. Its failure/cancellation settles that request
  before the next begins, rather than implicitly terminating the service. Service
  failure/cancellation accounts for queued work; graceful shutdown drains it.
  Document self-submit-and-await deadlock without adding automatic prevention.
- **Lifecycle control:** ingress records cancellation/shutdown independently of
  application queue capacity and serial execution. Repeated cancellation may
  coalesce; this needs neither another socket nor an unbounded control queue.

The findings below remain the historical review judgments. These dispositions
select the supplied defaults; implementation and the detailed control transition
are not yet delivered.

The subsequent [80N worked transition](80n-cancellation-to-cleanup-transition.md)
proposes the pending-Call/adapter/handler sequence and a new generic delimited-exit
operation. It records the extra protected handoff needed before cleanup and the
remaining protocol/ABI proof obligations. It is a post-review design proposal,
not a claim that the second reviewers reviewed or approved that mechanism.

### Baseline

The design was committed **before** dispatching reviewers:

`f6bbe471ec396a0916c7f8011ff80ae112c3053b`

Inputs at that commit: [80](80-concurrency-domains-and-structured-tasks.md),
[80A](80a-concurrency-api-sketches.md),
[80G](80g-concurrency-task-lifecycle-exercise.md), and
[80H](80h-concurrency-review-edge-cases.md), with
[80F](80f-concurrency-review-synthesis.md) for accepted decisions and the
first-round dispositions. Exact inputs remain available with `git show` at
that commit. All reviewers' line references refer to the committed input.

| Review | Verdict and remaining focus |
| --- | --- |
| [80I — Piumarta](80i-concurrency-second-piumarta-review.md) | Open-implementation direction supported; define the cancelled pending-Call transition into cleanup. |
| [80J — Kay](80j-concurrency-second-kay-review.md) | Retain the design; separate a request's execution from the serial service loop and identify its queue. |
| [80K — Ingalls](80k-concurrency-second-ingalls-review.md) | Coherent direction; define the cleanup transition and strong tracing ownership during handoffs. |
| [80L — Cox](80l-concurrency-second-cox-review.md) | Retain the direction; clarify request/service failure and independent admission of lifecycle control. |

## Consolidated material findings

### 1. Cancellation needs a real route from suspended execution into cleanup

**Pre-implementation: cancellation substrate. Sources: 80I and 80K.**

The existing V2 suspension retains a pending Call whose normal resume supplies
a value and commits a root-map transition. Our new contract instead requires a
cancelled wait to skip its ordinary continuation and run cleanup that may itself
wait. Supplying a fake `nil` result, changing the PC alone, or invoking the current
fatal Machine failure path does not establish that behavior.

**Smallest follow-up:** one concrete Frame/control trace naming the pending edge,
the cleanup activation, retained roots/code pins, native barriers, and the new
identity of any cleanup wait. Compare an ordinary Birnam adapter over an internal
wake result with a dedicated control transition; choose from existing invariants,
not from a wish to recognize `await` or `ensure:` specially in the compiler.

### 2. State who strongly retains execution during scheduler handoffs

**Pre-implementation: scheduling and GC integration. Source: 80K.**

Removing a wait entry before an allocating ready-queue send can expose a gap if
that entry was the last strong owner. Selecting a task out of a ready queue has
the same problem before its drive root is installed. A logical scope owner is
not yet a specified strong tracing owner.

**Smallest follow-up:** explicitly choose either scope retention of every
unfinished task/Machine through all phases, or temporary handoff roots spanning
old-owner removal and new-owner installation. Specify release after settlement
so terminal evidence does not retain execution indefinitely. Ordinary traced
Birnam objects can implement this; no policy-certification layer is required.

### 3. A request's lifetime must be separate from the serial service's lifetime

**Pre-implementation: supplied worker endpoint. Sources: 80J and 80L.**

If a request runs directly as the worker loop's body, cancelling it may unwind
the whole loop and strand accepted later requests. Making it an ordinary
fail-fast child scope can also terminate the service on request failure. The
current prose does not select the intended service behavior.

**Smallest follow-up:** draw one worker, its active request, and its queued
requests. Define the request's control/cleanup boundary and the outcome of
request failure, request cancellation, service failure, and owner cancellation.
A serial service can collect each request's settled outcome before taking the
next; that is a proposed remedy, not an already approved supervision policy.
Explicitly settle accepted-but-unstarted requests if the service ends.

**Related medium finding (80J):** identify the queue behind `domainFrom:`/`submit:`.
Serialization applies to that queue, not every possible access to a target object.
Document self-submit-and-await as a possible deadlock; automatic detection or
reentrant service is not required.

### 4. Lifecycle control must reach a busy serial service

**Pre-implementation: cancellable endpoint transport. Source: 80L.**

If cancellation uses the same bounded queue or serial handler as ordinary jobs,
it can get stuck behind the request it must cancel. This happens even with
ordinary eligible waits, no native blocking, and a healthy connection.

**Smallest follow-up:** domain ingress records lifecycle control independently
of application admission and request handling. It may demultiplex the same
connection; a separate socket/thread or distributed control framework is not
required. Network partitions, blocking native code, and deliberate debugger
pauses retain their already documented limitations. This is a supplied-protocol
progress rule, not protection against arbitrary user replacements.

## What the second review closes

- Responsibility for replacement behavior is explicit. The original demand to
  prevent overridden `outcome` from misleading its caller is retired.
- Native callback wait legality has a concrete draft rule independent of ready
  versus pending timing, without inventing catchable native failures.
- Ordinary Error values, explicit failure, cross-domain envelopes, and the legacy
  isolate compatibility boundary are distinguished.
- Protected updates, cleanup shielding, completion/cancellation ordering, and
  debugger pause ownership have usable semantic contracts.
- Disconnection, old-worker handles, remote uncertainty, and process-versus-thread
  termination evidence are separated. Detailed codecs and reconciliation remain
  engineering work.
- Native-library certification and coordinated worker live updates remain outside
  scope; reviewers did not reinstate them.

## Editorial follow-up and verification

After all reviews finished, the parent corrected stale wording in 80A that still
called deliberate child cancellation undecided. It now reflects the previously
accepted rule. The parent also corrected missing outer send brackets in the two
80H conditional examples, following the existing language grammar. These are
editorial corrections after the fixed review baseline; no new lifecycle decision
or implementation was introduced. README and 80F link this latest synthesis.

Validation covers local document links, whitespace, fenced examples, preservation
of the first review's archived input hashes, and provenance of the fixed second
review commit. Reviewers inspected relevant existing execution contracts and
source. No runtime implementation changed, no runtime tests were needed for
these document changes, and thread-hosting capability has not been qualified.
