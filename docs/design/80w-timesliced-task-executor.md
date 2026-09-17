# 80W — Timesliced task executor

This follows [80V](80v-bounded-machine-drives.md). The supplied Birnam executor
now uses bounded Machine drives, so resumable CPU work shares its owner without
explicit `yield` or `wait`. This is single-owner concurrency; parallel domains
and execution on multiple CPUs remain future work.

## Scheduling contract

`TaskExecutor quantum` supplies a default budget of 128 outer driver turns.
The executor runs ready tasks in FIFO order, appending runnable tasks for a later
turn. A wait resumes with its claimed value; a budget stop continues without
inventing a Call result. The old unbounded Machine APIs remain available.

The quantum counts driver turns, not instructions or elapsed time. Synchronous
native calls still finish before the driver can stop. No wall-clock fairness or
latency bound is promised. Changing the ordinary quantum method or its transient
Store cell changes policy; its contract requires a positive integer.

Automatic scheduling allows another task to run between ordinary sends. In
particular, a child may run before its parent's next send after `fork:`.
Cancelling immediately after an unguarded fork does not prove that the child
never entered its body. The cooperative traces in 80U were observations of that
executor, not ordering guarantees of automatic scheduling. Applications that
require ordering need explicit coordination. Admission itself remains complete
before the child becomes eligible to run.

## Protected library transitions

`TaskExecutor transition:` runs internal bookkeeping under a nesting counter on
the executing task. A budget stop inside a transition causes the owner to drive
that same task again before selecting a peer. The library protects admission,
queue mutation, cancellation records, failure aggregation, observer registration
and removal, wake claiming, joining admission closure, and terminal publication.
Host executor work and synchronous native callbacks already hold the owner and
need no additional guard.

The guard uses the existing protected-boundary protocol to restore the nesting
counter on an explicit task failure or boundary exit. It preserves Error-valued
ordinary returns as values. It is ordinary, replaceable Birnam, with no native
scheduler lock or selector recognition. It does not roll back partial mutation.
The supplied transition bodies are internal bookkeeping. Arbitrary user code,
reflection, or replaced methods can still violate policy; this is not a sandbox.

Public `wait`, `yield`, and ready or pending `outcome` reject with
`#taskTransitionWait` while inside a transition. Internal waiting is different:
it registers the wait identity and actually parks the Machine under the same
guard. An explicit park releases the owner even with a retained transition
counter; after wakeup the waiting task finishes that transition before a peer
can run again. This closes the gap in which a token could otherwise be resolved
while its Machine was only budget-stopped rather than waiting.

`transition:` is scheduler infrastructure, not the final application mutex API.
It supplies neither mutual exclusion across domains nor thread safety for native
libraries. Native static state remains the caller's responsibility.

## Cancellation and publication

Before continuing a runnable body, the owner delivers recorded cancellation
through the checked `exitRunnableBoundary:` protocol when the task is unshielded,
outside a transition, and has no pending handler delivery. The body-active flag
is installed inside the root boundary. It is cleared under a transition before
automatic joining, so early startup and return bookkeeping are not mistaken for
an entered body.

Handler delivery remains protected until cleanup installs its shield and
acknowledges it. Cleanup may park, spawn owned work, and fail. Failure evidence
still wins over cancellation; terminal publication still waits for cleanup and
children. The owner retains a local Machine reference through publication and
finishes its return bookkeeping even after the task releases its reference.

A rejected owner transfer or drive is retained as `controlError`; the executor
stops driving that task rather than endlessly requeueing it or fabricating a
terminal outcome. The unfinished task remains inspectable.

## Compatibility and evidence

All implementation changes are in the ordinary Birnam control library. There
are no Rust edits, IdApi or Method/StepMethod ABI changes, descriptor grammar
changes, or image version changes. New policy cells use the existing transient
schema mechanism. Image restoration initializes a fresh executor and quantum;
live task handles remain rejected and independent outcomes remain saveable.

The existing task smoke retains the 105 cooperative and Machine regressions
with a deliberately large quantum for schedule-specific traces. Another 66
assertions run across quanta of 128 and one driver turn, for 171 checks per
startup mode. They cover CPU progress without explicit yield, cancelling an infinite body into waiting cleanup, nested
transition restoration on failure, forbidden waits before registration, wake
claim/duplicate/cancellation races, observer cleanup, explicit cancellation
before admission releases the owner, fresh dispatch after a peer's live edit
and GC, and reference release at settlement. The tests run with normal compiled
startup and forced source recovery, including mapped and rebuilt image loading.
They stay in the existing native validation phase; no release step is added.

The focused task smoke and full native phase pass, as does the workspace suite.
The frozen IdApi client/module pass with unchanged fixture checksums, and the
archived v4 image still rejects. The unchanged pre-change supported image from
80T loads in both mapped and rebuilt modes and its saved method returns 42.
Its producing source commit remains unknown.

**Validation status:** `mise run verify-exhaustive` exits 0 with all 82 existing
phases and the complete-matrix/evidence audits passed. Independent phases ran
with two workers using the unchanged plan's commands, inputs, dependencies,
and timeouts. The normal entrypoint accepted their matching audited receipts.

Evidence is retained under `.cargo-target/timeslicing-evidence/`:
`normal-exhaustive.log` records that entrypoint, `exhaustive-run.json` preserves
the passed receipt, and `candidate.json` binds the tested source and release
artifacts to their hashes. The compatibility logs and baseline provenance are
alongside them. This is fresh qualification of these edits; 80V's earlier result
belongs to commit `5f44c15b`.

## Remaining work

Cleanup deadlines, asynchronous ingress, channels, application coordination
APIs, parallel domains, capture ownership, and task-record reclamation remain
separate work. This revision does not install the production prefix or claim
performance or independent cold-source release acceptance.
