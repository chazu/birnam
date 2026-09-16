# 80F — Concurrency review synthesis

> **Review synthesis, 2026-09-16.** Four agents reviewed the same proposed
> concurrency design through distinct historical design lenses. These are
> interpretations of published work, not participation or endorsement by Ian
> Piumarta, Alan Kay, Dan Ingalls, or Brad Cox. Findings and remedies remain
> review input; they do not authorize implementation or amend accepted runtime
> invariants.

## User decisions after review

The user narrowed the scope on 2026-09-16. The current Design 80 and sketches
record these decisions; the individual reviews below retain their original
findings against the earlier revision.

- **Worker updates:** stop and recreate a worker from updated code. No automatic
  propagation, coordinated live edits, rolling upgrades, or new in-place worker
  redefinition support is required. Preserve existing main-domain live editing;
  do not add restrictions solely to disable already-working local redefinition.
- **Application/native globals:** thread safety and coordination of C-library
  statics and application-module static/global state belong to the user. The
  suggested native-module admission/qualification system is not accepted scope.
  Birnam still must separate its own execution, heap, root, dispatch, and callback
  ownership correctly between domains.
- **Cancellation:** give it a distinct control path; do not equate it with an
  ordinary Error return or assume the current fatal Machine failure path performs
  language cleanup. Detailed transitions remain open.
- **Protected updates:** the user accepted short sections combining mutual
  exclusion, deferred cancellation, and no waiting inside; no automatic rollback.
  Exact API and enforcement details remain open.
- **Debugger defaults:** peers continue, structural inspection does not execute
  user methods, and paused tasks remain visibly unfinished. Detailed dependency,
  cancellation, deadline, and disconnect policies remain open.
- **Transport:** assume unreliable networks. Losing a connection cannot certify
  that remote work stopped or that its effects did not happen.
- **Task outcomes:** deliberate child cancellation is local to that child's
  owned work; scope/body cancellation propagates to children. Terminal child
  failure cancels the remaining scope work. Join through cleanup, retain genuine
  and cleanup failures, and leave ordinary Error objects as values. `await`
  propagates a cancelled child's cancellation; `outcome` observes it as data.
- **Cleanup:** shield required cleanup from repeated enclosing cancellation;
  allow waits with independent explicit cleanup budgets. Attempt registered
  cleanup in reverse nesting order and continue outer cleanup after failure.
  Preserve genuine failures and protect resource acquisition/registration from
  a cancellation gap. An unhandled timeout is failure; explicit resource policy
  may handle it and fall back to local release. No universal hidden timeout.
  Finished-but-failed cleanup may settle; still-executing cleanup may not.
  Shutdown ordering follows resource dependencies and all new work stays owned.
- **Malleability and responsibility:** the user explicitly prefers less
  protection from the consequences of changing the system. Public protocols and
  policies remain replaceable; their replacements are the user's responsibility.
  Checks need a concrete execution-invariant justification, such as ownership,
  roots, code lifetime, or native/control boundaries. Avoid certifying replacement
  behavior, guaranteeing progress under arbitrary policy, or restricting live
  edits just to preserve the behavior of the supplied protocols.
- **Cancellation versus completion:** after body, children, and cleanup settle,
  check and publish in one indivisible owner-domain transition. Genuine failure
  takes precedence, otherwise cancellation recorded before publication produces
  Cancelled even if not yet delivered, otherwise publish success. Published
  outcomes are permanent. Sending a remote cancellation request is not evidence
  that the owning domain has recorded it; cancellation does not imply rollback.
- **Channels and workers:** support zero-capacity rendezvous and positive bounded
  capacity, explicit end-of-stream, and copied values across domains. Workers
  handle whole requests serially by default. Graceful shutdown drains accepted
  work; cancellation requests interruption. Detecting owner loss stops admission
  and requests cancellation, without assuming reliable partition detection.
- **Next exercise:** the user accepted these recommendations and proceeding to
  [80G](80g-concurrency-task-lifecycle-exercise.md). Its proposed checked lifecycle
  mechanisms, remaining race rules, and policy boundaries require further discussion.
  Exact selectors and implementation remain unapproved.
- **Implementation language and root domain:** concurrency behavior belongs in
  replaceable Birnam objects over minimal execution/platform mechanisms, with one
  independently driven scheduler per domain. The root domain handles startup and
  application/session ownership using the same execution model, normally on the
  standalone application's main thread. The initial task is not the lifetime of
  an interactive session. Preserve the independent bootstrap as implementations
  migrate into Birnam.

## Reviewed revision

This section and the findings below describe the first review. The user requested
a second review after accepting the revised recommendations. Its new input
[80H](80h-concurrency-review-edge-cases.md) records channel closure, old-worker
handle identity, cancellation of paused tasks, and explicitly marked draft
details. The second review must identify its own fixed committed baseline.

The design was updated before review to include automatic timeslicing,
completion after children and cleanup settle, `await`/`outcome`, automatic scope
joining, cancellation requests, and debugger suspension before terminal failure.
The earlier explicit `join` sketch was replaced by `outcome`.

All reviewers were assigned the same fixed inputs. The exact original files are
preserved in the [review-input archive](../archive/reviews/80-concurrency-review-inputs-2026-09-16.zip),
under their original repository paths. The links below open the subsequently
revised working documents; the hashes identify the archived review inputs:

- [80 — Design](80-concurrency-domains-and-structured-tasks.md), SHA-256
  `0d7f0a217e3adf6c42dc1952327a126b901441e9cb14f942c7fd06ed9e242055`.
- [80A — API sketches](80a-concurrency-api-sketches.md), SHA-256
  `06e79906abfc92e4cd3345e36993db63cd63a785fc08e2cb4108640fbe0f079e`.

The individual reviews identify their primary sources and distinguish those
authors' published principles from the reviewers' own conclusions.

| Review | Lens | Central challenge |
| --- | --- | --- |
| [80B — Piumarta](80b-concurrency-piumarta-review.md) | Small object floor, open implementation, metacircularity | Which lifecycle facts require checked mechanisms, and which policies remain ordinary replaceable behavior? |
| [80C — Kay](80c-concurrency-kay-review.md) | Messaging, local state, understandable live systems | What makes a stateful receiver safe and comprehensible under interleaving, cancellation, and live editing? |
| [80D — Ingalls](80d-concurrency-ingalls-review.md) | Small comprehensible machinery, live inspection, practical storage management | Can scheduling, debugging, cleanup, and rooting work together without hidden execution traps? |
| [80E — Cox](80e-concurrency-cox-review.md) | Component contracts, separate evolution, native coexistence | Do task and domain boundaries preserve honest, usable contracts across placement and failure? |

## Assessment

**Retain the architectural direction; do not freeze its public API yet.** The
reviews identify concrete design gaps rather than demonstrating an unavoidable
violation of Birnam's invariants. That is not a proof of invariant preservation:
several plausible implementation shortcuts would violate existing contracts.

The useful result is a short list of semantic decisions. Most are acknowledged
as open in Design 80; the reviews supply counterexamples showing why they matter.
They do not justify expanding this into a large implementation or release plan.

## Decisions that matter most

### 1. Checked lifecycle facts versus malleable policy

An overridden `outcome` method must not accidentally convince a scope that its
still-running child has settled. Equally, native code must not secretly duplicate
the entire scope policy while presenting it as a replaceable Birnam protocol.
Specify the checked ownership/settlement mechanism and the replaceable policy
above it. Also define where scheduler policy executes, including a queue method
that reenters Birnam, waits, or fails while selecting the next task.

**Subsequent user disposition:** the first sentence is an original review concern,
not an accepted requirement to police overridden methods. Supplied protocols must
account for their owned work correctly; users own changes to those protocols.
Preserve Machine execution/lifetime integrity without certifying replacement
behavior. Scope accounting can live in Birnam. Initial scheduling decisions use
a synchronous select-and-return contract, with runtime borrows released before
policy calls. A replacement that stalls its domain is the user's responsibility.

### 2. Task failure and cancellation need a control protocol

The current Machine failure/Abort path is not a language-level cleanup protocol.
Existing runtime failures crossing ordinary synchronous native entry can abort
the process because that entry has no out-of-band failure channel. Adding an
`await` selector does not make this boundary recoverable.

Define legal task-control entry contexts, rejection of ineligible waits, and
cleanup transitions. A native comparator that is live-edited to await a pending
task is a useful counterexample. The same rule must explain both already-ready
and pending results without silently unwinding foreign frames.

### 3. Mutual exclusion is not cancellation safety

A task can hold a mutex, debit one account, then receive cancellation before
crediting the other. Releasing the lock during cleanup exposes inconsistent
application state. Distinguish protection against competing tasks from protection
of a short state transition against cancellation. Also define whether an endpoint
serializes requests to a stateful receiver and what happens if that receiver
awaits a request to itself.

### 4. Domain boundaries need explicit compatibility and failure contracts

The new task model treats a returned Error object as a successful value. The
shipped isolate worker classifies a handler's returned Error as an application
failure, and its admitted wire values do not include arbitrary Error objects.
Reusing the transport therefore needs an explicit outcome conversion contract;
an asynchronous wrapper alone is insufficient.

A disconnected request can settle locally without proving remote cleanup.
Account for who owns unresolved remote activity. The original review also
recommended native-module admission rules because logical heap separation does
not make C statics safe to share. **User disposition:** application/native shared
state is the user's responsibility; no qualification or admission scheme is
required. Birnam-owned runtime separation and callback routing remain necessary.

The original review requested a live-update compatibility contract: a live edit
can change `render:` from returning encoded image bytes to returning a path while
preserving selector and transferable type. **User disposition:** workers can be
stopped and recreated; cross-domain live-update machinery is excluded. The user
coordinates application protocols and deployment, while basic message validation
and honest outcomes remain required.
Also demonstrate a fallback component: observing a decoder's failed `outcome`
does not stop an ordinary fail-fast scope from cancelling its alternative.
Expected-error recovery and supervised execution need explicit examples.

### 5. Debugging must remain usable when tasks are blocked

Both `await` and `outcome` wait for completion. Add a separate nonblocking
inspection protocol for state, owner, wait dependencies, and debugger suspension.
An inspector that sends `printString` can itself block on a mutex held by the
paused task. Separate structural inspection from evaluation of user methods,
and define the owner and escape path for debugger evaluation. Pausing must not
silently release locks or make external effects replayable.

### 6. Root ownership must span transitions

Keeping Machines rooted in stable queues is not sufficient. Moving a task from
waiting to ready must retain it throughout cancellation and possible collection.
Specify temporary handoff roots, stale readiness rejection, and separate retention
of execution state from terminal evidence. Debugger retention belongs in the same
ownership accounting.

## Recommended next discussion

Start with the checked-mechanism/policy boundary and task-control transitions.
Then revisit a stateful receiver, cancellation during an update, and a paused
lock holder using a few short message traces. These will expose the remaining
API choices more clearly than adding selectors to the happy-path examples.

The individual reviews remain unchanged, and their exact input revisions are
archived above. The working design and sketches now reflect the user's narrower
scope. No code changes or runtime tests are part of this review; validation is
limited to document consistency, local links, archived baseline hashes, and
targeted source checks of existing behavior.
