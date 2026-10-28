# 80U — Source-authored structured tasks and cleanup

**Follow-up:** [80W](80w-timesliced-task-executor.md) enables automatic task
timeslicing and protects the executor transitions described here.

This implements a cooperative, single-owner task lifecycle on top of the
boundary mechanism from [80N](80n-cancellation-to-cleanup-transition.md). It is
part of the concurrency implementation; automatic timeslicing and parallel
domains are still separate work.

## Behavior

The ordinary `TaskControl` and `TaskExecutor` methods in
[stackless-control.birnam](../../modules/stackless-control.birnam) implement:

- `start:` creates a root task when called by the owner outside task execution.
  Inside a task it creates an owned child. `fork:` registers a child before
  enqueueing it, and returns before that child runs.
- A ready queue drives one Machine until it returns or parks, then drives the
  next. Completion notifications enqueue work; they never recursively resume a
  parent while its child is still executing. `yield` explicitly gives peers a
  turn. A CPU loop that never waits or yields still monopolizes this owner.
- Every task owns and joins its children. Body return cannot publish an outcome
  while a child or its cleanup is unfinished. Admission closes after the body
  and its registered cleanup return, before the automatic join.
- `await` waits for settlement and returns a successful value, or propagates
  failure/cancellation through the calling task's control boundary. `outcome`
  waits for the same settlement and returns the outcome as data. Both require
  an eligible task context, checked before consulting whether the result is
  ready. `pollOutcome` is the explicit nonblocking observation, answering nil
  until settlement. Cancelling an observer removes its registration without
  cancelling the observed task.
- A task waits; cancellation exits its body into the nearest protected handler.
  The handler installs shielding before acknowledging delivery. Cleanup runs
  once and may wait again. Repeated enclosing cancellation cannot resolve that
  cleanup wait.
- Each wait owns a fresh identity. A notification consumes it before enqueueing
  a resume. Duplicate notifications, including duplicates before the queued
  resume runs, cannot resolve a subsequent wait or resume execution twice.
- `fail:` records evidence in traced task state before exiting. It requests
  cancellation of owned children before entering cleanup, allowing cleanup to
  join those children. Outer cleanup still runs after an inner cleanup fails.
- A child's **terminal** failure records its complete outcome in its parent and
  requests cancellation of the remaining scope work. The parent still joins
  everybody. Observing or awaiting the same failure does not count it twice.
  Cancelling a child alone leaves siblings running; cancelling an owner reaches
  its descendants. Cleanup failures take precedence over induced cancellation.
- Children created during shielded cleanup remain owned. They are protected
  from the enclosing cancellation request and joined before publication. An
  actual child failure still cancels its siblings, including cleanup-created
  siblings. Resource code can explicitly await children before releasing their
  resources; automatic joining does not infer resource dependencies.
- Final publication occurs after the body, registered cleanup, and descendants
  settle: actual failure wins, then recorded cancellation, then the body value.
  An Error-valued ordinary return remains success. Published outcomes never
  change. Settled tasks drop Machine, parent, child-list and observer references;
  the ready queue releases each consumed entry.

Cancellation requests are recorded across the owned tree before the executor
starts draining their wakeups. A value notification already claimed before
cancellation is not replayed; cancellation can still prevent the task from
continuing its body when that queued wake is delivered.

### Outcome representation

These are ordinary implementation-protocol vectors, not a frozen public data
representation:

- `#[#success value]`
- `#[#cancelled reason]`
- `#[#failure evidenceVector cancellationReason childFailureOutcomes]`

The last vector keeps each contributing child's complete failed outcome,
including its original cancellation context. Flattened evidence supports
inspection without discarding the causal outcomes. A late cancellation does not
erase actual failure, and aggregation does not replace a child's original
cancellation reason with the parent's induced cancellation reason.

### Small program

Inside a caller selected for resumable execution:

```birnam
let task := [TaskControl start: [:scope |
  let left := [scope fork: [:child |
    [child ensure: [|
      [child wait]]
    cleanup: [|
      [System println: "left cleanup"];
      [child wait]]]]];
  let right := [scope fork: [:child | 42]];
  [left outcome];
  [right await]]];

// The owner supplies an event using the identity recorded for that wait.
let left := [[task children] at: 0];
let oldWait := [left waitToken];
[left cancel: #stop];
let cleanupWait := [left waitToken];
[left resolve: oldWait with: 7];        // false
[task pollOutcome];                   // nil; left cleanup is still owned
[left resolve: cleanupWait with: nil];
[task pollOutcome];                   // #[#success 42]
```

The root owner uses `pollOutcome`; a task uses `outcome` or `await`. Outside
task execution, `start:`, `cancel:`, and `resolve:with:` drive the ready queue
before returning, so their calls can take time while ready code runs. Waiting
callers must currently be compiled as resumable. The fixture uses the existing
candidate-selection API. This is not yet the `Tasks scope:` convenience API
sketched in 80A or a background I/O executor.

## Runtime connection and invariants

[machine.rs](https://github.com/chazu/birnam/blob/ec640c21aee5a245cebeb993ef7b7501decead3e/rust/birnam-execution/src/machine.rs) supplies
small generic Machine adapters: `runBlock:`, `resumeWith:`, `protectedBlock:`,
`leaveBoundary:`, `park`, and `controlEligible`. Execution loading installs
their registrations once. The eligibility query runs as a StepMethod and checks
actual drive/native-entry barriers; a library-level "current task" alone is not
proof that a native callback can wait. Protected calls, returns, exits and
suspension use the checked action publishers and descriptor V2 frames. No native
callback wrapper remains parked. The adapters contain no cancellation, outcome,
queue, joining, or cleanup policy. Their native fallback entries have distinct
operation-specific rejection diagnostics: the registration table requires
distinct method addresses, and release optimization otherwise merged the
self-referencing wrapper functions. The ordinary installed release smoke covers
this path, beyond tests linked into a Rust test executable.

Repeated Execution loading and new Machines preserve existing bindings and do
not reinstall methods after user replacement. Calls retain ordinary lookup. Definitions live in the existing
resumable control unit, so normal builds and source recovery include them.
Existing `Object ensure:` keeps its prior behavior; explicit task cleanup uses
`ensure:cleanup:`. Task and executor policy remains replaceable ordinary Birnam.

Task/executor fields now live in ordinary named reference slots. Internal
accessors and explicit `field:` views share those slots; there are no field facts,
schemas or synthetic attribute names. The owner `imagePayload` protocol rejects
prepared task handles and resets executor state without changing the live owner.
A retained task handle is not an imageable execution snapshot. Independently
retained outcomes and unused task factory clones can be ordinary image data.
The executor initializes empty process-local state when first used after image
loading. No IdApi/Method ABI, descriptor grammar, or image version changes.

Publication and wait registration rely on one owner and the current cooperative
execution rules. Future automatic timeslicing must supply the agreed protected
transitions; it must not simply preempt between these library sends and claim
this policy remains correct.

## Evidence

The existing native integration step includes:

```sh
bash scripts/task-control-smoke.sh PREFIX
```

It compiles real Birnam methods and nested blocks through normal resumable
selection. It checks 84 assertions in both compiled-artifact startup and forced
source recovery, including the earlier cancellation/cleanup traces plus:

- Fork-before-execution ordering, automatic child/descendant joining, explicit
  yield, and cleanup-created work.
- Deliberate child cancellation versus terminal child failure, cancellation
  before first execution, multiple failures, and original cancellation context.
- Cancellable observers, waiter removal, duplicate queued wakeups, and
  cancellation after a wake was claimed but before the task resumes.
- Ready and pending results queried through a synchronous native callback,
  rejection of native failure/cleanup entry before state changes, and explicit
  nonblocking host observation.
- GC while tasks wait, fresh cleanup dispatch after method replacement, and
  Error-valued success.

Both startup paths reject saving a retained task handle before image publication.
They then save an independent outcome, load it in mapped and rebuilt modes, and
start a fresh task after loading. Rust adapter tests also exercise eligible
StepMethod execution versus a real nested native callback, Machine identity,
entry-pin release, and preservation of user method replacements.

The frozen IdApi client/module pass without rebuilding; their committed
checksums remain unchanged. The archived v4 image still rejects with the expected
version mismatch. The retained pre-change supported image from 80T loads in both
mapped and rebuilt modes and its saved method answers 42. That image was not
regenerated for this candidate; its producing source commit remains unknown.
`structured-baseline.json` and `structured-candidate.json` in the evidence
directory record the baseline and current source/artifact hashes. The full
workspace tests and all three adapter tests in a release build also pass.

The earlier [80T](https://github.com/chazu/birnam/blob/6339747b7e7b51df74fab518001281322510cac5/docs/design/80t-cleanup-control-implementation-evidence.md) exhaustive
result qualifies the boundary-only revision. Validation of this additional
implementation is recorded below; no earlier pass is treated as a pass for
changed sources.

**Current validation:** the ordinary release build and full native integration
suite pass, including all 84 task assertions in both startup modes and both
image-loading modes. `mise run verify-exhaustive` exits 0: all 82 existing checks
and the complete-matrix/evidence audits pass for this candidate. The normal
entrypoint reuses matching, audited receipts from the unchanged exhaustive plan.
Its final log is `.cargo-target/task-control-evidence/normal-exhaustive.log`,
and `.cargo-target/validation/run.json` records the passed result. A copy is
retained as `exhaustive-run.json` in the task evidence directory.

Independent phases ran concurrently using the existing plan's commands, input
bindings, dependencies, and timeouts. A four-worker attempt hit the 900-second
limit in `numeric-sequence-debug` without an assertion failure. The unchanged
phase passed alone in 593.2 seconds; remaining phases then completed with two
workers. Failed-attempt logs are retained separately from final passed receipts.
The task fixture remains inside the existing native integration step, with its
inputs bound to that step's receipt. No release step was added.

## Remaining scope

The next slice, [80V](80v-bounded-machine-drives.md), adds bounded Machine drives
and owner-side exit from a runnable stop. Task-executor integration still needs
protected transitions before automatic timeslicing is enabled. Independent
cleanup deadlines, capture ownership, asynchronous I/O/cancellation ingress,
channels, parallel domains, and debugger policy remain unimplemented here.
This executor shares one owner cooperatively; it does not provide execution
across multiple CPUs.

Store-backed task records still follow the Store's retention model. Settlement
clears execution/ownership references, but automatic bounded reclamation of task
records is not implemented. Native control rejection retains unfinished,
inspectable execution and reports `machineControlRejected`; it does not invent
a terminal outcome or claim cleanup ran. Recovery/disposal policy for those
rejected tasks remains separate from successful structured settlement.

Production installation and release qualification have not been performed for
this revision. No performance claim is made.
