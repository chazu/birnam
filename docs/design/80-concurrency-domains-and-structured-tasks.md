# 80 — Concurrency domains and structured tasks

> **Status: agreed direction; cooperative structured tasks implemented, 2026-09-17.** The user agreed
> with the general direction and requested this design document followed by
> further exploration. The user subsequently favored automatic timeslicing,
> subject to preserving the project's vision and invariants and producing an
> idiomatic, enjoyable concurrency API. Detailed scheduling, lifecycle, and
> cross-domain semantics remain open. Waiting rules must follow the principle
> of least astonishment: the model must make sense first. The subsequent
> discussion favors completion after cleanup, automatic scope joining, explicit
> outcomes, and debugger inspection before terminal failure. The user requested
> four adversarial reviews of this revision. This records a target model, not
> delivered capability. The original proposal was prepared against
> `03312562`; review findings are separate from accepted design decisions.
> **Scope clarification after review, 2026-09-16:** worker code updates may
> require stopping and recreating the domain. Cross-domain live-update machinery
> is excluded. Application/module shared state is the user's responsibility;
> native-library qualification is not a concurrency feature. Cancellation needs
> a distinct control path. The user subsequently accepted protected updates,
> debugger defaults, local child cancellation, failure propagation after cleanup,
> buffered/unbuffered channels, serial workers, and conservative transport failure
> handling. Channel closure, old-worker handle identity, and recording
> cancellation without silently resuming a debugger-paused task are also agreed.
> [80H](80h-concurrency-review-edge-cases.md) supplies four edge-case traces and
> explicitly marked draft details for the second adversarial review.
> Selectors and detailed mechanisms remain design work.
> [80F](80f-concurrency-review-synthesis.md) records this disposition and links
> the exact archived review inputs.
> **Implementation update, 2026-09-16:** the user accepted the third cleanup-control
> review's four follow-ups and authorized implementation to begin. [80N, sections
> 9–10](80n-cancellation-to-cleanup-transition.md#9-accepted-third-review-resolutions-and-first-implementation)
> records the concrete protocol and compatibility evidence plan. [80T](80t-cleanup-control-implementation-evidence.md)
> tracks the first implemented control slice. [80U](80u-birnam-task-cleanup-slice.md)
> now implements source-authored cancellation, cleanup, cooperative ready queues,
> child ownership/joining, and outcome waits. Automatic timeslicing, channels, and
> parallel domains remain work ahead.
> [80V](80v-bounded-machine-drives.md) adds opt-in bounded Machine drives and
> owner-side cleanup entry. Task-executor integration still requires protected
> multi-send transitions before automatic timeslicing can be enabled.

## 1. Direction

Use independently owned, mutable Runtimes for parallel execution and lightweight
structured tasks for concurrency within each Runtime. Communication between
Runtimes is explicit and asynchronous. Independent Runtimes can execute Birnam
code simultaneously; there is no global execution lock.

Call an independently owned Runtime a **domain**. Each domain contains an
ordinary live object environment, with its own heap, Store, dispatch state,
Machines, and code-generation ownership. Its mutable state has one execution
owner. Several tasks can share objects within that domain.

The long-term target is thread-hosted domains as the ordinary backend, with
process-hosted domains available for stronger fault containment and native
libraries that cannot safely coexist. Start with the existing process substrate;
thread hosting requires substantial ownership work and measured justification.

The user chooses and coordinates application/module shared state, including
native-library statics and static/global data in Birnam modules. The concurrency
implementation will not infer thread safety, add automatic synchronization,
certify libraries, or enforce a native-module admission scheme. A process backend
is available when the user wants separate native process state.

Birnam must still make its own execution machinery domain-correct: heaps,
GC roots, Machines, dispatch ownership, code lifetimes, and callback routing
cannot accidentally belong to the wrong Runtime. This requirement does not
promise safe application-level sharing. Ordinary Birnam globals belong to their
domain's environment; user code remains responsible for coordinating tasks that
share them or native state deliberately shared across domains.

Keep the language-facing model independent of OS placement where its semantics
really are equivalent. Expose meaningful differences, especially native-crash
containment, termination, and resource cost.

### Design priorities

Preserving Birnam's vision and invariants is the governing constraint. Within
that constraint, prefer an idiomatic, understandable API with useful automatic
scheduling. Similarity to goroutines is a useful intuition, not a requirement to
copy Go's sharing model, primitives, or syntax. Implementation convenience must
not silently reduce malleability or force routine callers to manage scheduler
internals.

Waiting and lifecycle rules must follow the principle of least astonishment.
Give each operation a small, stable meaning that survives changes of scheduler
or hosting backend. Make exceptions visible at the relevant API boundary rather
than relying on users to understand runtime implementation details.

The user explicitly prefers responsibility over protection from malleability.
The supplied concurrency protocols should have clear, useful contracts; users
who replace their methods or policies take responsibility for the resulting
behavior. Runtime checks preserve concrete execution invariants such as ownership,
roots, entered-code lifetime, and valid native/control transitions. They do not
certify that a replacement policy is fair, terminates, avoids deadlock, preserves
application invariants, or behaves like the supplied policy.

Treat the public contracts below as contracts of the supplied protocols, not a
requirement to prevent users from redefining those protocols. Preserve ordinary
lookup and replacement. Any proposed restriction must identify the concrete
execution invariant it protects; preventing a user from making a bad live edit
is not sufficient justification. Diagnostics should expose the consequences
without expanding this work into policy certification or compatibility policing.

## 2. Relationship to existing designs

- [48](48-concurrency-reactors-and-isolates.md) supplies the retained blocking
  process-isolate contract and earlier reactor exploration. Its pre-stackless
  implementation constraints are historical. This proposal does not revive the
  withdrawn worker-replacement or rollout policy.
- [60A](60a-stackless-execution-semantics.md) and
  [60D](60d-stackless-concurrency-fit.md) supply owner-affine Machines, rooted
  resumable Frames, native barriers, and candidate scheduling boundaries.
- [74](74-malleable-multicore-execution-and-optimization.md) compares parallel
  owners, shared mutable execution, and transactional sessions. This document
  develops the parallel-owner candidate; it does not change optimization rules.
- [The isolate guide](../guide/06-isolates.md) remains the shipped public API.

Today, the Runtime drives a Machine to completion or explicit suspension rather
than enforcing a general scheduling budget. Isolate requests block through the
complete exchange while holding the worker registry mutex. The public IdApi and
several native modules retain process-wide Runtime state. None of these facts
is resolved merely by introducing the terminology below.

## 3. Vocabulary and ownership

| Term | Proposed meaning |
| --- | --- |
| Domain | One independently owned live Runtime and its mutable environment. |
| Task | A schedulable logical computation within a domain, using existing Machine/Frame machinery. |
| Scope | Owns child task lifetimes, result collection, cancellation, and cleanup. |
| Endpoint | A capability for explicit asynchronous communication with another domain. It does not expose a foreign Oop. |
| Reactor | Readiness and timer machinery that makes waiting tasks runnable. |
| Scheduler | Selects runnable tasks and assigns execution budgets. |

A domain can contain many objects and tasks. Creating a task does not require
creating an isolated heap or OS process. A task is a lifecycle abstraction over
execution; its exact relationship to nested Machines needs specification.

Local message sends retain ordinary synchronous Birnam semantics. A task may
be suspended while a result is pending, but its caller resumes with the normal
result. Cross-domain requests are explicit operations; an ordinary object send
must not silently acquire remote failure and transport semantics.

Initially, a domain remains pinned to its owner thread. Balance independent jobs
between domains through messages. Migrating live Machines or stealing tasks
across heaps is outside the initial design.

### Root domain and parallel execution

The accepted model has one independently driven scheduler per domain. Multiple
thread-hosted domains can execute simultaneously on available CPU cores; multiple
process-hosted domains provide the same form of parallelism through processes.
Tasks in a single domain take turns and do not automatically spread across cores.
The root domain does not drive worker schedulers or mediate every message.

The root domain is the initial application environment, using the same execution
model as other domains. It hosts startup, the application/session owner, and
normally the interactive live environment. In a standalone application, place it
on the main OS thread for host integration. Root status does not grant access to
foreign heaps. Application ownership accounts for tasks and worker domains during
shutdown; supervision remains ordinary policy rather than a special runtime.

Distinguish the root domain from its initial task. A CLI application normally
shuts down after its application scope settles. An interactive session can own
many successive evaluation scopes and remain alive after one evaluation finishes
or fails. Root-domain scheduling and worker scheduling remain independent.

## 4. Why this direction

Birnam permits effectful lookup, method replacement, reflection, mutable object
structure, and native participation. Putting parallel threads inside one mutable
Runtime would require a concurrent memory model for all of those operations,
plus collection, Store access, callback entry, and executable reclamation.

Domain ownership preserves the local object model while concentrating parallel
coordination at explicit communication boundaries. It also allows a small native
mechanism with substantial policy authored in Birnam.

The costs are explicit: copied messages, per-domain runtime memory, communication
latency, and restarting workers when updating their code. Programs requiring
frequent shared mutation across cores may fit this model poorly. This proposal
makes no throughput or memory-efficiency claim before representative measurement.

An independently mutable domain is not a transactional view of one shared image.
If a universally shared live object world is a requirement, reconsider this
choice before implementing its public surface.

## 5. Malleability and invariant obligations

The following are requirements for a later implementation:

1. **Preserve local sends.** Keep evaluation order, effectful lookup, DNU,
   inheritance, supported MOP changes, identity, shared Cells, and current
   mutation visibility. Scheduling must not justify stale dispatch or replaying
   an effectful lookup.
2. **Preserve active execution.** An active Frame retains its entered
   implementation. Later sends resolve current dispatch, including after a
   suspended task resumes. Do not pin an entire task to an obsolete dispatch
   world merely because it started there.
3. **Preserve roots and code lifetime.** Ready queues, waits, task results,
   cancellation, and cleanup must retain and release the necessary object roots
   and generation pins. A stale readiness event must not resume completed or
   cancelled execution. Suspended Frames remain tied to their owner.
4. **Preserve native boundaries.** Do not suspend through arbitrary synchronous
   native activations or migrate their callbacks to another owner. Arbitrary
   native Methods remain supported without compiler recovery metadata.
5. **Preserve the malleable execution protocol.** Machines and StepActions remain
   observable Birnam objects under their existing contracts. Fairness cannot
   silently require sealed canonical implementations.
6. **Keep persistence honest.** Tasks, continuations, endpoints, scheduler queues,
   OS resources, and pending operations remain process-local. Persist explicit
   descriptions of durable work and recover new execution from them.
7. **Keep the bootstrap and authority model.** Retain independent cold bootstrap,
   compiled-unit authority, and the Store's role as data and reflection mirror.
   Scheduler or transport caches must not become competing dispatch authorities.

Single-owner execution prevents simultaneous mutation of a domain's memory; it
does not imply deterministic results or atomicity across a sequence of sends.
New task interleavings need an explicit semantic contract. Existing serial
programs must retain their behavior when no concurrent work is introduced.

This proposal does not amend the public IdApi, Method ABI, canonical descriptors,
or image formats. Thread hosting must first determine which changes are possible
within existing contracts and separately justify any necessary revisions.

## 6. Scheduling and shared state within a domain

### Preferred default: automatic timeslicing

The user's preference is automatic timeslicing rather than relying exclusively
on explicit waits/yields. Its precise safe-point and atomicity contracts still
need design; the preference does not waive existing native or runtime barriers.

Give resumable execution a bounded scheduling budget. When the budget expires,
return to the scheduler at an eligible Machine boundary and let another runnable
task proceed. I/O and explicit waits suspend the task and release its execution
opportunity immediately.

This provides automatic timeslicing for ordinary resumable code without
interrupting arbitrary native instructions. Counting StepAction transitions is
a starting mechanism, not a wall-clock latency guarantee.

The analogy to goroutines is lightweight execution with runtime-managed
scheduling: programmers do not insert a yield into every CPU loop. Go multiplexes
goroutines across OS threads and supports asynchronous preemption; see the
[Go FAQ](https://go.dev/doc/faq#goroutines) and
[runtime preemption implementation](https://go.dev/src/runtime/preempt.go).
Birnam's proposed tasks instead interleave within their owning domain at eligible
Machine boundaries, while independent domains supply multicore parallelism.
This proposal does not adopt Go's shared-heap execution model or promise its
ability to move other runnable work away from a blocked OS thread.

An eligible boundary must have rooted state, valid code pins, and no native
barrier, live host borrow, or runtime critical operation that forbids suspension.
The driver regaining control is necessary but not sufficient. Resuming another
task must also establish its own execution scope rather than inheriting an
unrelated synchronous callback's scope.

Long synchronous native Methods, blocking FFI calls, and custom implementations
that do unbounded work can still monopolize a domain. Use nonblocking adapters,
bounded operations, or another domain. Fairness guarantees must name the
execution paths they cover and expose overruns; they cannot make arbitrary
foreign code interruptible.

### Protected updates: accepted direction, detailed contract still open

Automatic timeslicing introduces interleavings even without an explicit wait.
For example, two tasks can both read the same counter before either writes its
incremented value. A single OS thread does not prevent this lost update.

The user accepted short protected sections combining task-aware mutual
exclusion, deferred cancellation, and no waiting inside. Acquiring the lock may
wait before entering the protected body. Once entered, cancellation must not
interrupt the update; release and restoration of the enclosing cancellation
state must happen on normal exit or unwind. This provides no rollback if the
body itself fails after a partial mutation.

Automatic timeslicing may continue, and the lock protects only cooperating
accesses. This does not freeze the object graph or bypass ordinary live dispatch.
The exact selector, reentrancy, acquisition/cancellation races, enforcement of
the no-wait rule, and treatment of native calls remain design work. Existing
native barriers and runtime lifetime invariants still apply; an ordinary wrapper
cannot make arbitrary blocking C code safe or interruptible.

## 7. Structured task lifecycle

Every task has an identifiable lifecycle owner. A scope owns its child tasks and
does not report successful completion while those children remain unresolved.
Long-lived services need an explicit service owner or supervisor rather than
accidentally escaping a short-lived request scope.

### Results and completion

A task is complete when its body, owned children, and required cleanup have
settled. There is one published terminal outcome: success with a value, failure
with evidence, or cancellation with a reason. Terminal publication is one-shot;
repeated observation does not execute the task again. Retaining the same result
value does not freeze or copy a mutable object returned within a domain.

The supplied owner strongly traces every unfinished task, its Machine/suspended
execution, and registered cleanup state through waiting, ready, running, cleanup,
and debugger pause. The owner is itself retained through the domain's rooted
application/service ownership tree. Queue entries are not the sole lifetime
owners. Establish this ownership before exposing a task or removing temporary
construction roots. After settlement, release execution-specific references while
retaining outcomes and explicitly retained diagnostics; no permanent registry
entry is required. This is an ordinary traced-object ownership arrangement, not
certification of arbitrary replacement policies.

The proposed public surface is:

| Operation | Proposed meaning |
| --- | --- |
| `scope fork: block` | Create a child task owned by the scope. |
| `task await` | Wait for settlement, return a successful value, otherwise propagate task failure or cancellation. |
| `task outcome` | Wait for settlement and return an inspectable success, failure, or cancellation outcome. |
| `task cancel` | Request cancellation; do not claim settlement or rollback. |

Scopes join their children automatically. Initially omit a separate public
`join`: both waiting operations already wait for lifecycle completion. An
operation that emits values before it finishes should expose an explicit stream
or channel rather than report a premature terminal result.

Ordinary Birnam Error objects remain values. Returning an Error object is a
successful return of that object; declaring task failure requires an explicit
operation whose spelling remains open. `await` failure propagation is a new
task-control operation, not permission to reinterpret existing Error returns as
exceptions. Its unwind, native-barrier, and reentry behavior must be specified
before implementation. `outcome` supplies observation without that propagation.
Unrecoverable VM corruption is not made recoverable by wrapping it in an outcome.

### Scope failure policy

For an ordinary scope, a terminal child failure requests cancellation of the
remaining scope work, including the scope body. The scope reports failure after
owned work and required cleanup settle. Retain independent child failures and
cleanup failures; do not let induced sibling cancellations obscure their cause.
If cleanup fails during cancellation, retain both that failure and the original
cancellation context rather than reporting clean cancellation.

An explicitly supervised scope may allow children to fail independently. Design
the ordinary scope first. Expected failures can be handled inside a child before
it declares terminal failure, or under a supervised policy. Merely observing a
failed child through `outcome` does not erase the ordinary scope's failure or
reverse cancellation already requested. Observation, recovery, and ownership
policy are distinct operations.

Deliberately cancelling a child cancels its owned descendants without cancelling
siblings or failing an otherwise successful scope. Cancelling the scope or its
body requests cancellation of all its children. A body failure likewise cancels
the remaining children and is reported after settlement. These rules are accepted
direction, along with the failure policy above.

`await` on a cancelled child propagates cancellation into the awaiting task;
`outcome` instead returns the child's cancellation as data after settlement.
If the awaiting task is the scope body, its resulting cancellation cancels the
scope's remaining children. `outcome` is still an ordinary cancellable wait: it
does not shield the observer from its own cancellation.

Open details include aggregate outcome representation and precise control
transitions. [80G](80g-concurrency-task-lifecycle-exercise.md) begins the concrete
state-transition exercise before selectors or mechanisms are frozen.

### Cancellation and deadlines

The user agrees with giving cancellation its own control path. Do not implement
it by treating an ordinary returned Error as cancellation or by reusing the
current fatal Machine failure/Abort path as though it ran language cleanup.
The precise control transitions and native-boundary behavior remain to be designed.

Cancellation is an idempotent request propagated through owned work. It takes
effect at eligible execution boundaries and uses the reviewed unwind/cleanup
path rather than discarding Frames or roots. Once delivered, it remains pending
through ordinary execution. Required cleanup is shielded from repeated enclosing
cancellation so resource release can finish. Cleanup needing I/O may have an
explicit deadline; deadline expiry must not imply release or settlement when
work remains unresolved. The mechanism and its interaction with malleable
control remain open.

The accepted completion/cancellation cutoff is final outcome publication, after
the body, owned children, and required cleanup have settled. The owning domain
checks recorded failures and cancellation and publishes the outcome in one
indivisible transition: genuine task/cleanup failure takes precedence (retaining
cancellation context), otherwise recorded cancellation yields Cancelled,
otherwise publish success. Cancellation recorded before publication counts even
if it has not yet been delivered to the task body. Recording cancellation while
cleanup runs therefore prevents success without interrupting protected cleanup.
Published outcomes never change; later cancellation is a no-op for that task.

For another domain, sending a cancellation request does not mean its owner has
recorded it. The owner's ordering determines the outcome; transport loss can
leave it unknown. Cancellation never proves that external effects were undone
or that retrying is safe. A blocked native call can delay settlement indefinitely.

A wait timeout bounds observation, leaving ownership intact. A scope deadline
requests cancellation of the scope's work and then accounts for cleanup; it does
not promise cleanup finishes at the deadline. Domain termination has backend-
specific consequences and cannot be implemented as arbitrary thread unwinding.

A transport failure can leave remote execution uncertain. A local request handle
may settle with failure describing that uncertainty; it must not claim the remote
task or its cleanup completed. Ownership of outstanding remote activity and
reconciliation after disconnect still need an explicit protocol.

### Cleanup: accepted behavior

Cleanup is ordinary code with protected execution and explicit ownership. It may
wait, suspending its task while other tasks remain runnable. Shield it from
repeated enclosing cancellation; an explicit cleanup deadline uses its own
budget rather than immediately inheriting an already-expired enclosing deadline.
This differs from short protected updates, which prohibit waiting.

Use ensure-style reverse nesting order: attempt registered cleanup on normal
return, task failure, or cancellation, once per exit. A failing cleanup block
does not prevent attempts at remaining outer cleanup blocks. Retain the original
failure and genuine cleanup failures. Ordinary Error return values do not become
failures implicitly. Resource APIs must establish cleanup registration before
cancellation can interrupt the acquisition/ownership handoff. Attempting cleanup
does not guarantee successful release or survival of a process crash.

An unhandled cleanup timeout is a cleanup failure. A resource protocol may
explicitly handle an expected graceful-shutdown timeout and fall back to local
release. Attempt remaining cleanup either way. There is no universal hidden
cleanup timeout; resource APIs can supply documented budgets. Finished-but-failed
cleanup can settle once all owned execution has settled. Cleanup still executing
in an uninterruptible native call cannot. Ending an observer's wait changes
neither fact. Successful local release makes no promise about remote effects.

Shutdown ordering follows resource dependencies. Signal children to stop and
join them before releasing resources they still use; a protocol may need to
close a channel first to make those children stop. Cleanup-created work remains
owned and joined. Settled means no outstanding owned execution, with failures
reported honestly; it does not mean every resource release succeeded.

Cleanup methods retain normal live dispatch. The supplied protocol tracks which
cleanup remains due and retains Frames, roots, and code pins through suspension.
Users remain responsible for the consequences of replacing that behavior.
The concrete control extension and subsequent implementation authorization are
recorded in [80N](80n-cancellation-to-cleanup-transition.md). The supplied
task/cleanup protocol is implemented in [80U](80u-birnam-task-cleanup-slice.md),
with automatic timeslicing added in [80W](80w-timesliced-task-executor.md).
The final public convenience API remains implementation/design work.

### Waiting rules: least astonishment

The following are proposed API requirements, prompted by the user's explicit
priority that the model make sense first:

- **An ordinary task wait suspends the caller.** Other runnable tasks in its
  domain remain eligible to execute. Foreign blocking operations that cannot
  provide this behavior need an explicit adapter or a documented blocking
  boundary; they must not masquerade as ordinary task waits.
- **Waiting does not grant exclusive access.** Other tasks may change shared
  state while the caller is suspended. With automatic timeslicing, absence of
  an explicit wait does not itself grant exclusive access either.
- **Waiting does not silently release synchronization.** If locks are offered,
  an ordinary wait must not implicitly unlock them. A specialized operation
  that releases and reacquires a lock must state that behavior. Lock-across-wait
  restrictions and deadlock handling still need specification.
- **Acceptance is not completion.** Submission success is not execution success.
  A task's terminal result is published only after its owned work and required
  cleanup settle. Intermediate outputs use a distinct stream/channel contract.
  Local request settlement after transport failure does not assert remote
  completion.
- **A wait timeout does not prove cancellation.** State whether a deadline
  merely ends waiting or also requests cancellation. Cancellation request,
  acknowledgement, and completed cleanup are distinct. None implies rollback
  of effects that already occurred.
- **Scope exit accounts for children.** Deadline expiry must not silently
  detach remaining work. If cleanup cannot settle within a requested bound,
  expose that unresolved state and its continuing owner. Do not promise both
  guaranteed bounded return and completed cleanup for arbitrary native calls.
- **Failure remains inspectable.** Preserve the distinction between an
  application result, runtime failure, remote failure, and an unknown outcome.
  Waiting must not trigger an implicit retry that can duplicate effects.

These requirements guide naming and examples. The proposed `await` and `outcome`
selectors remain reviewable and do not make waits safe in every synchronous
native calling context.

### Live debugging before terminal failure

The model should preserve Self-like inspectable execution inside a domain.
Self documents automatically scheduled processes with suspension, stepping,
stack inspection, and continuation through its
[process API](https://handbook.selflanguage.org/2024.1/processes.html) and
[debugger](https://handbook.selflanguage.org/2024.1/textdebug.html). Parallel
domains and structured scope lifetimes are additions proposed here, not claims
about Self's documented concurrency model.

An explicit debugger policy may intercept a recoverable failure before terminal
publication and irreversible unwinding. The task enters a visible debugger-
suspended state with its inspectable execution retained. It is unfinished; its
scope stays open, and waiters must be able to distinguish debugger suspension
from an opaque hang. The debugger can inspect, step, perform supported live edits,
and request supported continuation/recovery or terminal failure. Supported live
edits here do not require adding worker hot-reload; section 9 permits worker
updates to require restart.

This is not automatic recovery from every error, arbitrary native activation, or
partially performed effect. Recovery capabilities depend on the actual stopped
state and existing control/native barriers. Active Frames retain their entered
implementation even if later sends observe a live edit. Do not silently replay
effectful operations or promise an already-unwound task can resume.

The ordinary scope's fail-fast rule starts when failure becomes terminal. The
user accepted peers continuing by default, structural inspection that does not
execute user methods, and visibly unfinished paused tasks. Explicit evaluation
of user code is a separate debugger action. Do not freeze the whole domain
implicitly. A paused task may hold locks, roots, and code pins. Expose peer
dependencies. Ordinary cancellation and scope deadlines record cancellation
without silently resuming a deliberately paused task or releasing its locks.
The task and its owning scope remain unfinished. An explicit debugger action
can resume into pending cancellation and cleanup when the stopped state supports
it. The draft disconnect default in 80H retains that visible pause under the
existing owner, permitting reattachment; it does not fabricate release.
Production scopes must not silently enter an indefinite interactive debugging
wait because the editor happened to be attached.

## 8. Messages and backpressure

The user accepted buffered and unbuffered value channels. A tentative
`Channel capacity: 0` denotes a sender/receiver rendezvous; positive capacity
permits that many queued values and waits when full. Waiting suspends the task,
not the domain. End-of-stream must be distinguishable from an ordinary `nil`
message. Local channels may carry ordinary objects within their domain; crossing
domains still requires copied values. Send acceptance never means processing
has finished. Close stops new sends, permits already accepted buffered values to
drain, then produces end-of-stream. Blocked senders whose values were not accepted
receive a closed-channel result. A committed transfer is not undone by subsequent
cancellation. 80H records ordering examples; exact selectors remain tentative.

Start with the existing admitted copied-value set and wire validation. Arbitrary
objects, Blocks, shared Cells, continuations, Aliens, and native pointers do not
cross the boundary. Domain identity can cross as an explicit value key; copied
objects do not retain cross-domain pointer identity.

Requests can be submitted independently of waiting for their results. Queues
must have bounded admission, with an explicit wait or rejection result when
full. Capacity must account for message bytes as well as item count.

The protocol must distinguish acceptance, delivery, completion, cancellation,
and an uncertain outcome. Do not promise exactly-once effects or automatically
retry requests whose external effects are unknown. Final completion versus
cancellation uses section 7's owner-recorded publication rule. Channel and remote
admission ordering are described separately in 80H; a local ordering does not
establish remote agreement.

The user explicitly requires assuming unreliable networks. A lost connection
alone proves neither that a request did not run nor that its worker stopped.
Local transport failure must not be presented as confirmed remote cancellation
or cleanup. On detecting owner loss, a worker should stop accepting work and
request cancellation of owned outstanding work. Detection is not immediate or
certain under partitions; ownership and eventual cleanup still need a protocol.

Requests and handles stay bound to their worker instance. Replacing a worker
does not redirect old handles or replay outstanding requests. Keep unresolved
remote activity under its existing lifecycle owner even if a local request
observation settles as transport failure with unknown remote outcome. An owning
scope cannot treat that observation as confirmed domain shutdown. 80H distinguishes
request settlement, domain lifetime, and backend termination evidence.

New task endpoints use explicit success/failure/cancellation envelopes separate
from transport status; they do not reuse the legacy worker's `isError` heuristic.
Preserve the shipped isolate API through an explicit compatibility boundary.
Ordinary Error objects are not automatically transferable: an application adapter
may encode them as admitted copied values. An unsupported result is a result-
transfer failure, not an implicit declaration that the task returned a failure.

A worker is a request-handling task/service, serial by default: receive, handle
the request completely (including waits), then receive the next. A domain may
host multiple tasks, so one domain thread does not itself guarantee serial
whole-request handling. Overlapping requests require an explicit policy. Multiple
domains can execute in parallel on CPU cores without SIMD or GPU execution.
Graceful shutdown stops admission and drains accepted work; cancellation requests
interruption. Neither may claim completion before owned work and cleanup settle.

### Accepted serial service contract

Each request has its own task and scope, separate from the long-lived dispatcher.
The service owner collects each request's settled outcome before starting the
next. This request-isolating policy is authored in Birnam; placing requests under
an ordinary fail-fast service scope and merely observing `outcome` would not
provide it. Descendants inside an individual request use ordinary scope rules.

| Event | Supplied service behavior |
| --- | --- |
| Request success | Return its value; begin the next request after settlement. |
| Request failure | Return its failure; begin the next request after cleanup/settlement. |
| Request cancellation | Cancel only that request's owned work, settle cleanup, then continue. A queued request can be cancelled without executing its body. |
| Service cancellation | Stop admission, cancel active work, and cancel queued requests without executing them. Retain ownership through settlement. |
| Service failure | Stop admission and account for all outstanding requests. Report service failure for known unstarted requests; active/remote effects require honest evidence. |
| Graceful shutdown | Stop admission and drain accepted requests. |

The initial `domainFrom:` convenience handle addresses one default serial
dispatcher. Its named-receiver submissions share that queue. Serialization covers
that queue, not direct local sends or other access to the same receiver. Submitting
to one's own serial queue and awaiting the result can deadlock; detection and
automatic reentrant servicing are not part of the supplied contract.

### Lifecycle control admission

Domain ingress distinguishes application requests from lifecycle control.
Cancellation/shutdown reaches owner-domain lifecycle handling independently of
application queue capacity and the active request's completion. It can use the
same connection with demultiplexing; a separate socket or thread is not required.
Recording control does not bypass eligible execution boundaries, debugger pause,
native blocking, or network uncertainty. Repeated cancellation can coalesce into
one pending state; this does not require an unbounded control queue. Transport
implementation must continue to admit control while application admission is
backpressured, rather than blocking its ingress loop on the full service queue.

Shared immutable buffers, ownership transfer, and remote references are possible
extensions. Each needs its own identity, lifetime, and failure rules. Persistent
collections alone do not prove that their reachable object graph can be shared
safely across domains.

## 9. Worker code updates: restart is sufficient

The user explicitly prefers the simpler contract for this concurrency work:

- Preserve existing live editing and method-redefinition behavior in the main
  domain and the existing local dispatch/Frame lifetime invariants.
- Start worker domains from an explicit image or code environment. An edit in
  the main domain does not update existing workers.
- To run new code in a worker, stop that domain and create a new one from the
  updated code. In-place worker redefinition is not a promised facility of this
  concurrency design; requiring restart is acceptable.
- Do not add automatic propagation, coordinated publication, rolling upgrades,
  cross-domain edit transactions, or group deployment tooling.

This is a scope reduction, not a requirement to actively disable local
redefinition mechanisms that already work. Ordinary local lookup still uses the
domain's current dispatch, and retained Frames still own their entered code.
No new worker hot-reload support is required to ship concurrency.

Stopping and replacing a domain must account for pending work and retained
resources through the normal lifecycle rules. Old handles must not silently
refer to a replacement domain, and uncertain requests must not be replayed
automatically. These are execution-lifecycle obligations, not a rollout system.
For thread-hosted domains, stopping means a supported shutdown path; it does
not authorize forcibly unwinding an arbitrary native thread.

The user is responsible for deploying workers with suitable application
protocols. No automatic compatibility negotiation or adapters for live worker
upgrades are required. Basic message validation and honest request outcomes
remain necessary independently of live editing.

## 10. Implementation direction

Three bounded increments can establish the model:

1. **Usable parallel process workers.** Separate submission from waiting, remove
   the registry-lock-across-I/O bottleneck, and provide bounded outstanding work
   and explicit results. Preserve the shipped blocking API's failure contract.
2. **Structured tasks within a Runtime.** Add readiness integration, task
   ownership, fair driving, cancellation, and cleanup over existing Machines.
   Choose the interleaving/atomicity contract first. This work can begin alongside
   the transport increment; both must agree on waiting and completion semantics.
3. **Thread-hosted domains.** Make Birnam-owned runtime state domain-correct:
   IdApi routing, roots, callback entry, built-in runtime-module state,
   compiler/loader ownership, and executable lifetime. Initially retain
   owner-thread affinity and copied messages. Application/native-library global
   state remains the user's responsibility; no library certification, automatic
   hardening, or placement-admission system is part of this increment.

The process backend remains useful after thread hosting. A fatal native error
can take down every domain in one process; logical heap isolation is not process
fault isolation or a security sandbox. Current isolate timeouts can terminate a
worker process; that behavior cannot be silently reproduced by killing a thread.

### Birnam implementation and the minimum execution substrate

The accepted direction is to implement concurrency behavior in ordinary Birnam:
schedulers, task/scope protocols, channels, cancellation/shielding/deadline policy,
cleanup ordering, outcome collection, worker pools, and root supervision. Native
starting implementations must expose enough mechanism for real Birnam replacements,
rather than permanently hiding policy behind wrappers.

The candidate execution substrate supplies bounded Machine driving, suspension
and valid resumption, control transfer supporting language cleanup, and external
event delivery into the owner domain. Existing ownership, roots, code pins, and
native barriers still apply. Exact primitive selectors and cleanup representation
need design; a checked fact need not introduce a new primitive or VM-level scope.
For example, the supplied completion protocol may perform check-and-publication
using a general protected execution mechanism instead of a native scope engine.

Drive each domain by collecting events, asking its ordinary scheduler object for
a task and budget, driving that Machine, then reporting the resulting execution
event. The initial scheduling-decision contract is synchronous: select and return.
Release incompatible runtime borrows before invoking policy. A replacement that
does not return can stall its domain; no separate policy-certification or progress
supervisor is required. Suspending policy requires a suitable execution context,
not retaining an unsafe native activation.

The long-term target is Birnam code over execution/platform primitives, not the
elimination of platform dependencies or the independent source bootstrap. The
[Machine object boundary](60b-stackless-machine-and-frames.md) permits primitive
implementations to move from Rust to Birnam without changing their object
interfaces. Such migrations must preserve the
[independent bootstrap](44-toward-self-hosting.md) and existing execution contracts.

## 11. Evidence needed to choose and refine the design

Use a small set of representative scenarios rather than a release-sized matrix:

- Submit independent CPU jobs to two domains before awaiting either. Observe
  real execution overlap and measure elapsed time against sequential execution.
- Keep a timer or interactive request responsive while another task runs a
  resumable CPU loop. Measure scheduler latency and single-task overhead.
- Cancel waiting and runnable tasks; verify cleanup, roots, and code pins settle
  once, including races with completion and domain failure.
- In the main domain, edit a method while an older Frame is suspended. Verify
  the retained Frame resumes correctly and a subsequent send observes the new
  method. Worker code updates may instead be demonstrated by stop/recreate.
- Saturate a message queue and show bounded memory/admission behavior; inject
  failure after delivery and report uncertainty without implicit replay.

A responsive I/O service with parallel CPU work is the first product exercise.
Thread hosting additionally needs correct separation of Birnam-owned runtime
state and comparative startup, memory, transport, and throughput measurements.
Early process-worker success does not qualify threaded hosting. Coordinated
worker live editing and third-party native-library qualification are excluded.

## 12. Exploration order

1. **Interleaving and atomicity:** with automatic timeslicing preferred, define
   eligible boundaries and how mutable code protects a multi-send invariant.
2. **Domain lifecycle:** account for pending work and handles when stopping and
   recreating a worker. Restart is sufficient for code updates; live propagation
   is no longer an open requirement of this design.
3. **Task outcomes:** joining, cancellation, ordinary Error values, cleanup, and
   service supervision under failure.
4. **Communication:** bounded admission, ordering, result ownership, transferable
   values, and uncertain effects.
5. **Hosting:** the minimum native mechanism and costs of separating Birnam-owned
   runtime state while retaining existing ABI and native participation where
   possible. Application/module global-state safety belongs to the user.

The user accepted the semantic recommendations recorded above. Scheduler-policy
execution rules, checked completion mechanisms, exact cancellation/communication
races, and API selectors remain design work. The user subsequently authorized
starting with the reviewed cleanup-control mechanism. [80G](80g-concurrency-task-lifecycle-exercise.md) records
the lifecycle exercise; [80N — From a cancelled wait to Birnam cleanup](80n-cancellation-to-cleanup-transition.md)
works through the wait-adapter and delimited-exit mechanism, the accepted third-review
fixes, and the bounded compatibility evidence plan. [80T](80t-cleanup-control-implementation-evidence.md)
records the first implemented control slice. [80U](80u-birnam-task-cleanup-slice.md)
records the cooperative task executor, automatic joining, cancellation, and
cleanup library. [80V](80v-bounded-machine-drives.md) adds bounded Machine
drives, and [80W](80w-timesliced-task-executor.md) enables automatic timeslicing
with protected Birnam executor transitions. Parallel domains remain future work.
The accepted policies are not reopened by that work.
For that mechanism, the user explicitly requires preserving the public IdApi,
Method/StepMethod ABI, descriptor V2, and image format. A separately specified
Machine/object-protocol extension may add the operation while preserving existing
meanings. 80N records the proposed additive approach and required compatibility
evidence; no incompatible change is authorized by this design.

Domain, task, and scope are working names. Public selectors and syntax should
follow these decisions and a few small program examples rather than precede them.

### API exercises

[80A — Concurrency API sketches](80a-concurrency-api-sketches.md) works through
these exercises with tentative message-send APIs and an explicit timeout example.

Explore the surface through three small programs before choosing selectors:

1. Fetch two independent resources concurrently, combine their results, and
   ensure neither operation outlives the owning scope unintentionally.
2. Submit CPU work to another domain while the caller remains responsive;
   clearly show which input values cross the ownership boundary.
3. Update shared state from two local tasks while preserving a multi-send
   invariant; show the synchronization and cancellation behavior explicitly.

Prefer sequential-looking task bodies whose waits suspend only the task, clear
ownership of child work, and result/cancellation operations that compose through
ordinary Birnam protocols. A suspension-capable operation must still respect its
calling context and native barriers; sequential-looking syntax does not make
every native caller suspendable. Keep domain boundaries visible where they
change sharing or failure semantics. Evaluate the examples for live inspection,
method replacement, cleanup, and comprehensible failure behavior as well as
brevity. These are API goals, not an accepted syntax or a decision to introduce
an async function category.
