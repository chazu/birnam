# 80C — Adversarial review through an Alan Kay-inspired lens

> Independent review, 2026-09-16. This applies selected published ideas of
> Alan Kay; it is not impersonation, an endorsement, or a claim about what he
> would conclude. Recommendations below are the reviewer's judgment.

## Baseline and lens

Reviewed [80](80-concurrency-domains-and-structured-tasks.md) and
[80A](80a-concurrency-api-sketches.md), with SHA-256 values:

- 80: `0d7f0a217e3adf6c42dc1952327a126b901441e9cb14f942c7fd06ed9e242055`
- 80A: `06e79906abfc92e4cd3345e36993db63cd63a785fc08e2cb4108640fbe0f079e`

Kay's 2003 correspondence describes objects as communicating computers and
emphasizes messaging, protection of local state, and late binding. This supports
asking whether a receiver controls its own behavior and whether clients depend
on implementation details. It does **not** establish a preferred task scheduler
or require an actor for every Birnam object.
[Primary source: Kay's correspondence with Stefan Ram](https://www.purl.org/stefan_ram/pub/doc_kay_oop_en).

His account of Smalltalk connects protected communicating components with a
system people can learn and use interactively. That motivates evaluating the
concurrency model through its observable behavior and tools, not just its
execution machinery.
[Primary source: Kay, *The Early History of Smalltalk*](https://www.cs.tufts.edu/comp/150FP/archive/alan-kay/smalltalk-hopl-ii.pdf).

## Verdict

**Keep the direction; do not freeze the public contract yet.** Independent live
domains, explicit communication, retained active Frames, and fresh later sends
fit this lens well. Keeping local sends synchronous also avoids pretending
transport failure is ordinary local invocation.

The main weakness is that the examples demonstrate concurrent computations more
clearly than cooperating, stateful objects. Five bounded clarifications would
make the model substantially easier to trust. These are design risks and open
contracts, not evidence of an implemented invariant violation.

## 1. High — Old worker failures and new task outcomes need an explicit adapter

**Affected:** 80 §§7, 8, 10; 80A §§1–2.

The proposal says returning an Error object is successful task completion and
declaring failure is explicit. The [shipped isolate guide](../guide/06-isolates.md#failures-and-deadlines)
says a handler returning a Birnam error becomes an `#application` failure.
The proposal also retains that blocking contract while initially reusing the
process substrate.

**Counterexample:** a worker returns an Error value intentionally. A local
`fork:` succeeds with that value. A new asynchronous wrapper around the existing
isolate response could instead mark the child failed and cancel its scope.
Moving work across an explicit boundary already changes sharing; it should not
silently change whether an application result counts as failure.

The copied-value set also excludes arbitrary Error objects. Thus promising that
every local result can cross unchanged would be wrong in the opposite direction.

**Bounded remedy:** specify the conversion table for new remote operations:
successful transferable value, unsupported result, explicit task failure,
legacy application failure, and uncertain transport failure. Preserve the old
blocking API separately. If Error values need portable representation, define
one explicitly; otherwise report unsupported transfer without interpreting the
object as an explicit task-failure request. This is a missing compatibility
bridge, not a reason to weaken ordinary Error-value semantics.

## 2. High — A domain does not yet give a stateful receiver a predictable turn

**Affected:** 80 §§3, 6, 8; 80A §§2–3.

The document correctly distinguishes single ownership from atomic execution,
but the CPU example sends work to a named object without defining whether its
requests execute serially or as interleaving tasks.

**Counterexample:** two requests to an inventory object each read one remaining
item, then both reserve it. The receiver has one domain and ordinary synchronous
methods; neither fact protects the reservation. Moving the object into its own
domain has not necessarily made it an independent state owner in the useful
application sense.

The mutex example handles cooperating callers, but asks every caller to know
which state and lock belong together. A later caller can forget the lock.

**Bounded remedy:** add one stateful-service sketch where coordination belongs
inside the receiver or its explicit endpoint. Choose whether a serial endpoint
holds its turn across a wait, permits reentry, or requires the receiver to
protect only its state transition. Include the self-call case: a serial handler
awaiting another request to itself can deadlock. This can be a library protocol;
it need not change ordinary objects or prohibit reflection. Atomicity is already
an acknowledged open decision; this example exposes the decision's practical
meaning.

## 3. High — Releasing a lock on cancellation is not enough to preserve state

**Affected:** 80 §§6–7; 80A §3.

**Counterexample:** a method holds a mutex, debits one account, then credits
another. Automatic cancellation arrives between those updates. Cleanup releases
the mutex correctly, exposing an inconsistent object state to the next task.

The documents honestly disclaim rollback. Nevertheless, the pleasant
`during:` example can teach that mutex protection plus cleanup is sufficient
for maintaining an invariant under the proposed cancellation model. It is not.
This concerns application invariants; it is not proof that the VM invariants
have been violated.

**Bounded remedy:** distinguish mutual exclusion from cancellation protection.
Give one short-update recipe: prepare fallible work first, then perform a
bounded protected state transition, with cancellation deferred until the
transition exits. State what happens if that region waits or enters arbitrary
user code. If such protection is rejected, show the recovery obligation instead.
Do not smuggle a transaction guarantee into mutex cleanup or silently make all
message sends atomic.

## 4. Medium — Debugger suspension needs an observation path that does not wait

**Affected:** 80 §7, especially “Live debugging before terminal failure.”

The document requires waiters to distinguish a debugger-paused task from an
opaque hang, but both public observation operations wait for settlement.

**Counterexample:** a child pauses while holding a mutex. Its sibling waits for
that mutex; the parent waits in `await`; the inspector asks for `outcome` and
also waits. All documented lifecycle observations point toward a result that
cannot exist until somebody diagnoses the dependency.

**Bounded remedy:** specify a nonblocking observation protocol for task state,
owner, current wait reason, and debugger suspension, plus notification of
changes. Distinguish a snapshot from stable terminal evidence. Allow inspection
of the dependency without granting arbitrary cross-owner execution or foreign
Oop access. The peer-pause and debugger-disconnect policies are already open;
the missing observation operation should be explicit beside them.

## 5. Medium — The remote example exposes deployment before defining a service

**Affected:** 80 §§3, 8–9; 80A §2.

The caller chooses an image, a global receiver name, and a selector. This is
reasonable bootstrap machinery, but it is a weak everyday abstraction for a
replaceable component.

**Counterexample:** two independent render services inside one domain need
different configuration. A named global receiver does not explain how a caller
obtains the particular service, what authority its endpoint carries, or what
happens when that service is replaced while requests are queued.

**Bounded remedy:** retain the low-level startup example and add a proposed
service endpoint obtained from it. Define endpoint identity, accepted message
protocol, and replacement/expiry behavior. Sending through an endpoint should
still be visibly asynchronous. It should not cache a method implementation or
bypass receiver-side ordinary lookup; queued work needs the already-required
live-edit compatibility rule. This is an API direction, not a demand to add
remote object references or transfer arbitrary endpoints in the first increment.

## Questions to settle next

1. What is the smallest example of a stateful object that safely receives two
   concurrent requests, including a wait and cancellation?
2. Which outcomes can cross domains, and how does the new protocol coexist with
   the shipped isolate failure classification?
3. Can a user inspect why a scope is unfinished without awaiting that scope?
4. What remains stable about a service endpoint when its implementation changes?

Answer these with a few message traces and revised examples. No new scheduler
implementation or broad qualification matrix is needed to resolve this review.
