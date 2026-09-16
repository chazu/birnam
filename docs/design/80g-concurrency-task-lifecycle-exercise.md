# 80G — Task lifecycle and minimum runtime guarantees

> **Design exercise, 2026-09-16.** Based on the accepted semantic direction in
> [Design 80](80-concurrency-domains-and-structured-tasks.md). The transitions and
> mechanism/policy split below are proposals to examine, not delivered runtime
> behavior or authorization to implement. Existing dispatch, Frame, native
> barrier, identity, and lifetime invariants remain requirements.

## 1. One concrete task

A scope owns tasks A and B. A receives a job from a local channel, updates a
shared counter inside a protected section, and releases a resource during
cleanup. B is independent sibling work. The scope body requests cancellation
of A while A is waiting for a job, then observes A through `outcome`.

The resource and cleanup are conceptual: this exercise does not introduce a
new cleanup selector or change ordinary Error-value behavior.

| Step | Visible behavior | Required execution fact |
| --- | --- | --- |
| A starts | A belongs to the scope; B can run. | One domain owns A's Machine, Frames, and reachable roots. |
| A receives from an empty channel | A waits; B remains runnable. | Register the wait and relinquish execution without a lost wakeup. |
| The scope body cancels A | A has a cancellation request; B continues. | Record a pending request; do not publish a terminal outcome. |
| Cancellation wins the pending wait | A stops waiting for a job. | Withdraw/claim the wait exactly once; no job is consumed by this cancelled wait. |
| A enters cleanup | The body does not resume after `receive`; resource release runs. | Use a language cleanup control path; retain necessary Frames, roots, and code pins. |
| Cleanup itself waits | A is visibly cleaning up; B can run. | The cleanup wait is distinct from the withdrawn receive wait. Repeated enclosing cancellation cannot abort release. |
| Cleanup succeeds | A can settle as cancelled. | Owned descendants have also settled; no required cleanup remains. |
| The outcome is published | The observer receives `Cancelled`; B can still finish normally. | Commit one terminal outcome and notify registered observers without losing or duplicating a resumption. |
| The scope exits normally | It waits for B too, then may succeed. | A's deliberate cancellation alone does not fail the scope. |

Cancellation request, cancellation delivery, and terminal settlement are three
different events. Scheduling status (running, runnable, waiting, debug-paused)
is separate from lifecycle phase (body, settling/cleanup, terminal). A task can
be waiting during either its body or its cleanup.

## 2. Change one event at a time

### A job arrives at the same time as cancellation

Proposed rule: the domain owner orders the wait-resolution events. Exactly one
event claims that pending wait. Cancellation before handoff leaves the value
available to a receiver. Handoff before cancellation commits delivery; later
cancellation cannot pretend delivery never happened or automatically replay it.

This exposes a decision still needing a precise API contract: cancellation after
delivery can occur before application processing. A plain value channel cannot
promise durable processing or redelivery. If a job must survive consumer failure,
it needs an explicit acknowledgement/ownership protocol above the channel.

Each wait needs a distinct identity so a late receive notification cannot resume
A's later cleanup wait. Local ordering must not be mistaken for a distributed
agreement about remote execution.

### Cancellation arrives inside the protected update

The request becomes pending. The update continues with cancellation deferred and
waiting prohibited. On exit, release the lock and restore the enclosing
cancellation state before delivering cancellation. Lock acquisition itself can
wait and be cancelled before entering the body; granting ownership and entering
protection must not leave a cancellation gap that leaks the lock.

Ordinary sends retain ordinary dispatch behavior. If a replacement method tries
to wait, reject the prohibited suspension before parking. This does not promise
rollback of mutations already made, static purity, or interception of arbitrary
blocking C code. Automatic timeslicing may continue; other tasks still need to
cooperate with the lock.

### Cleanup fails or cannot finish

**Accepted behavior:** cleanup may wait and is shielded from enclosing
cancellation, with its own explicit deadline when needed. Attempt registered
cleanup in reverse nesting order on normal return, failure, or cancellation.
If one block fails, attempt remaining outer cleanup and preserve all genuine
failures. Acquire resources and register cleanup without a cancellation gap.
These are supplied-protocol guarantees; replacing that protocol carries the
responsibility described in Design 80's design priorities.

For example: cancellation -> attempt graceful connection shutdown with a 500 ms
cleanup budget -> peer does not respond -> timeout -> attempt local release.
An unhandled timeout produces a cleanup failure; a resource API may explicitly
handle an expected timeout and complete a fallback release instead. There is no
universal hidden timeout, nor a promise that the peer received the final message.

A genuine cleanup failure retains the original cancellation context. When A
publishes terminal failure, the ordinary scope requests cancellation of B and
its body and reports failure after owned work settles. A failed cleanup attempt
can finish; an uninterruptible native call still executing cannot be declared
finished. Stopping observation does not settle the task. Remaining work keeps
its owner; cleanup does not automatically detach work it creates.

Follow resource dependencies when stopping children and releasing resources they
use. This can require closing a channel to signal shutdown before joining, rather
than a blanket rule to join before any cleanup. Exact cleanup control/unwind
representation and native integration remain design work.

### Completion and cancellation race

**Accepted rule:** final outcome publication is the cutoff. After the body,
owned children, and required cleanup have settled, the owning domain checks
recorded failures and cancellation and publishes in one indivisible transition:

1. Genuine task or cleanup failure produces Failed, retaining failure evidence
   and any cancellation context.
2. Otherwise, cancellation recorded by the owner produces Cancelled, whether or
   not it has already been delivered to the task body.
3. Otherwise, publish Succeeded with the returned value.

Thus, body returns 42 -> cleanup starts -> cancellation is recorded -> cleanup
finishes produces Cancelled. If Succeeded(42) was published before cancellation
was recorded, it stays Succeeded(42). All published outcomes are permanent.
An ordinary returned Error object is still a value, not a task failure.

Protected cleanup finishes before publication; recording cancellation does not
grant permission to interrupt it. Cancelled does not imply no effects occurred,
rollback, or safe replay. Across domains, sending cancellation is not recording
it: the owning domain establishes the order, and a lost connection can leave
that outcome unknown. The implementation mechanism for this ordering remains
design work; the observable rule is now agreed.

### A synchronous native callback attempts to wait

An ordinary synchronous C return cannot carry a suspended Birnam task. Preserve
the existing entry/native barriers: reject an unsupported suspension rather than
parking with borrowed native state or blocking the owner thread to simulate a
task wait. An explicit resumable/asynchronous adapter can arrange a later owner-
domain notification. Existing fatal native-boundary errors do not become
recoverable language exceptions merely because the task model has outcomes.

## 3. Minimum checked facts versus replaceable policy

This is a proposed responsibility split, not a demand that every checked fact
be a new primitive or live in native code.

| Checked execution fact | Policy that can remain ordinary replaceable behavior |
| --- | --- |
| A Machine executes under its owner and cannot be resumed twice concurrently. | Which eligible task runs next; fairness and priorities. |
| A wait has one resolution and stale notifications cannot target a later wait. | Channel capacity, admission strategy, timeout choices. |
| Live execution retains roots and entered-code pins through waits and cleanup. | Service ownership arrangements and worker replacement policy. |
| Suspension/control transfer respects native barriers and protected regions. | Where application code establishes protection or requests cancellation. |
| Terminal publication is one-shot and requires real settlement of owned work. | Ordinary fail-fast scope versus an explicitly supervised scope. |

Policy objects may request valid execution transitions and inspect their evidence.
The underlying mechanism preserves owner-correct Machine execution, roots, valid
wait resolution, and faithful settlement records. Public methods and policy
objects remain replaceable: the runtime does not certify that a replacement
reports or uses those facts correctly, preserves the supplied protocol's
semantics, or makes progress. Users own the consequences of such replacements.
Do not freeze user methods, cache method identity as policy authority, or turn
scope failure policy into an unreplaceable VM rule. Each proposed check must
justify itself against a concrete execution invariant, not merely the risk of
a bad user policy or live edit.

Before invoking user policy, release runtime borrows that cannot survive reentry.
The accepted initial scheduling-decision contract is synchronous select-and-return.
Its concrete driver entry must respect existing native/control boundaries.
A replacement that fails to return can stall the domain; the minimum mechanism
does not promise progress under arbitrary replacement policy. The checked facts
above do not require a VM-level scope engine: supplied lifecycle and completion
protocols should live in Birnam over the minimum execution mechanisms described
in Design 80 section 10.

## 4. Existing invariants this must preserve

- **Malleability and fresh sends:** policy remains ordinary behavior where valid;
  subsequent sends resolve current local definitions.
- **Frame continuity:** an entered Frame retains its implementation and generation
  pin while suspended, cleaning up, or stopped for debugging.
- **Identity and GC:** a local channel does not copy local objects; registrations,
  queued values, results, and retained Frames have explicit tracing ownership.
- **Domain ownership:** cross-domain notifications transport admitted values,
  never borrowed Oop pointers or executable closures from another heap.
- **Native/ABI boundaries:** no arbitrary native-stack capture or unwind, and no
  presumed ABI/image change hidden inside a scheduler refactor.

These obligations follow the existing execution contracts in
[60A](60a-stackless-execution-semantics.md) and
[60B](60b-stackless-machine-and-frames.md). New cancellation cleanup requires its
own control design; current Machine failure/Abort is not evidence it exists.

## 5. What this exercise leaves to settle

The terminal publication rule, cleanup behavior, Birnam policy implementation,
and initial synchronous scheduling-decision contract are agreed. Concrete driver
entry and cleanup control representation remain engineering design questions;
reviewers can assess them without treating the high-level behavior as undecided.
Preserve redefinition and put responsibility for replacement-policy behavior on
the user.

The four bounded examples in [80H](80h-concurrency-review-edge-cases.md) now cover
channel closure, explicit task failure/native entry, worker loss, and paused-task
cancellation. Draft details are marked for adversarial review. They do not have
to become a distributed framework to settle this local-task exercise. No
implementation or rollout plan is part of this document.
