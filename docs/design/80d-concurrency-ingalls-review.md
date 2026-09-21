# 80D — Concurrency review through an Ingalls-inspired lens

> **Independent adversarial review, 2026-09-16.** This interprets published
> principles; it does not speak for Dan Ingalls or imply his endorsement.
> Findings are proposals, not accepted design amendments or implementation work.

Reviewed frozen SHA-256 inputs:

- Design 80: `0d7f0a217e3adf6c42dc1952327a126b901441e9cb14f942c7fd06ed9e242055`
- Design 80A: `06e79906abfc92e4cd3345e36993db63cd63a785fc08e2cb4108640fbe0f079e`

## Sources and lens

In [*Design Principles Behind Smalltalk*](https://www.cs.virginia.edu/~evans/cs655/readings/smalltalk.html)
(1981), Ingalls connects individual comprehension, a small set of fixed
mechanisms, uniform message protocols, automatic storage management, and
meaningful observation/manipulation of system components. These principles
motivate asking whether scheduler behavior can be understood and inspected
through the ordinary environment.

[*Back to the Future: The Story of Squeak*](https://smalltalkzoo.computerhistory.org/papers/Back%20to%20the%20Future.PDF)
(Ingalls, Kaehler, Maloney, Wallace, and Kay, 1997), especially “The Interpreter,”
describes debugging the interpreter inside Smalltalk and maintaining an interface
to object memory across implementation changes. This motivates a practical,
inspectable scheduling model with a narrow native boundary.

The following scenarios and remedies are this reviewer's judgments. Neither
paper establishes Birnam's concurrency semantics or chooses its domain model.

## Verdict

**Continue design exploration; do not freeze the public task API yet.** Owned
domains, ordinary local sends, explicit copied communication, retained active
implementations, and language-authored policy fit this lens well. The principal
risk is a system that looks uniform in examples but requires hidden knowledge
of scheduler, debugger, native-stack, and cleanup state to use safely.

No unconditional contradiction was established in the frozen proposal: it
expressly leaves the dangerous bridges open. The high-priority findings below
identify decisions required before implementation. An implementation that takes
the shortcuts described would violate existing contracts.

Severity: **P1** blocks the affected task/debugger mechanism; **P2** blocks a
credible inspectable user experience. These are design risks, not shipped bugs.

## 1. P1 — Define where scheduling policy itself executes

**Sections:** 80 §§5–6, 10; 80A §§1, 3. **Classification:** open mechanism with
an invariant-breaking shortcut.

Consider a language-authored scheduler selecting the next task. Its queue method
has been replaced with code that performs an ordinary send, reenters resumable
execution, and eventually waits. If selection runs inside the previous task's
native activation, it inherits that task's execution scope. If it runs while
holding the queue or Machine registry borrow, even an innocent callback can
reenter the borrowed state. Uniform sends do not make either situation safe.

[60D §§4–7](60d-stackless-concurrency-fit.md) requires short physical borrows and
distinct entry scopes for unrelated work. The existing
[`run_stackless_loop`](../../rust/birnam-runtime/src/runtime.rs) establishes an entry
scope around each native step; a Rust loop exit alone is not a new task context.

**Bounded remedy:** specify one drive-to-scheduler handoff with decoded,
committed, rooted state and released borrows. Define whether policy runs in a
dedicated control Machine and which operations are legal there. A policy wait,
failure, or recursive scheduling attempt needs a deterministic answer. Publish
an inspectable blocked/overrun reason without claiming arbitrary policy code can
be preempted. Exercise replacing a ready-queue method with a reentrant method
before adding priorities or pluggable schedulers.

## 2. P1 — Failure propagation needs its own control protocol

**Sections:** 80 §7; 80A §§1, 3–4. **Classification:** acknowledged open bridge;
an existing failure channel cannot safely implement it unchanged.

A child fails while its parent awaits inside a synchronous native callback.
The parent holds a Mutex and owes cleanup. Returning an Error value permits
ordinary execution to continue; throwing away Frames skips cleanup; unwinding
the C caller violates its ABI. Merely representing the failure as an outcome
does not choose a legal control transfer.

[60A §9](60a-stackless-execution-semantics.md) makes listed out-of-band runtime
failures fatal through ordinary synchronous sends/direct thunks, which lack an
error channel. A recoverable subset requires an explicit taxonomy amendment.
Current `DecodedAction::Abort` handling in
[`runtime.rs`](../../rust/birnam-runtime/src/runtime.rs) marks the Machine failed and
returns its reason; [`MachineState::fail`](../../rust/birnam-runtime/src/execution.rs)
clears drive/delimiter state. This is not the proposed language cleanup protocol.

**Bounded remedy:** enumerate application-declared task failure, cancellation,
recoverable debugger stops, and fatal runtime violations separately. Specify
legal entry contexts and rejection before a forbidden wait acquires resources.
Give cleanup an explicit state transition, including failure during cleanup.
Demonstrate one native-callback boundary and one cancelled lock holder before
promising that `await` propagates safely. Returning an ordinary Error must remain
successful value return throughout.

## 3. P1 — Debugger inspection can deadlock the computation it observes

**Sections:** 80 §7, “Live debugging”; 80A §3. **Classification:** explicitly open
policy with a concrete user-visible failure mode.

Task A stops for debugging while holding a Mutex. Task B waits on that Mutex.
The inspector asks an object for `printString`; that method also acquires the
Mutex or awaits B. The debugger now waits for the task it deliberately paused.
Allowing peers to run does not resolve this cycle. Conversely, evaluating
arbitrary inspection methods can mutate the supposedly stopped computation's
objects. A deadline or debugger disconnect adds another possible owner of the
next transition.

**Bounded remedy:** separate non-executing structural inspection from explicitly
requested evaluation. Show the stopped task's held resources, wait dependencies,
pending cancellation, and retained implementation. Debugger evaluation must be
owned separately, with visible blocking and cancellation behavior. Choose one
initial policy for deadline/disconnect while stopped; never silently release a
task's lock or replay its last effect. Validate the A/B/inspector cycle as one
small interactive example. A debugger-suspended state label alone is insufficient.

## 4. P1 — Root ownership must cover the handoff, not just stable queues

**Sections:** 80 §§5, 7–8. **Classification:** obligation stated; transition
protocol still missing.

A task is removed from a wait queue as readiness arrives. Before it enters the
ready queue, cancellation completes its scope and GC runs. An unrooted Machine
can disappear during that gap. Keeping every registry entry permanently strong
avoids the gap by leaking abandoned tasks and their code generations. A late
readiness event can then target a reused operation identity.

[60D §14](60d-stackless-concurrency-fit.md) explicitly assigns queued rooting to
the scheduler, not the capability registry. Existing scoped drive roots and
Machine tracing in [`runtime.rs`](../../rust/birnam-runtime/src/runtime.rs) do not
automatically cover new queues, result objects, or debugger retention.

**Bounded remedy:** document the strong owner before and after each transfer,
including a temporary transfer root; invalidate completed wait registrations by
generation. Keep execution retention separate from terminal result/evidence
retention so observing old outcomes need not pin whole execution histories.
An inspectable “retained by” chain should explain a task that cannot disappear.
Use one readiness/cancellation/collection race and one abandoned-handle case to
check this design, rather than introducing a general scheduler matrix.

## 5. P2 — Domain locality needs an everyday inspection protocol

**Sections:** 80 §§3, 8–9; 80A §§2, 5. **Classification:** user-experience choice,
not a demand for shared heaps.

The two render domains load the same image. An editor updates one `render:`
method while a request is queued in the other. Identical selector names and
argument values now produce different behavior. The proposal correctly permits
this, but a task handle showing only “running” leaves users unable to discover
which environment received their request or why their edit had no effect.

**Bounded remedy:** give local task/request inspection a domain-qualified
identity, owning scope, submission/acceptance/execution status, and observable
code provenance when execution begins. Display what is unknown after transport
loss. Remote inspection should use explicit endpoints and copied descriptions,
with observation times; it must not invent transparent foreign Oops or claim a
simultaneous global snapshot. Start with the two-domain sketch and explain a
mixed-generation result entirely through these objects.

## Decisions to bring back to the API sketches

1. Where does scheduling policy execute, and what happens if it waits or fails?
2. Which failures can stop before unwind, and which remain fatal at native entry?
3. Can inspection execute user methods, and how does that evaluation get unstuck?
4. Who strongly owns each task through queue transfer, debugging, and settlement?
5. What can a programmer inspect to explain a queued request's actual domain and
   implementation?

Answer these with a small executable state model and the existing three program
sketches. Preserve process-first delivery and independent bootstrap; this review
does not require rewriting the VM in Birnam before useful concurrency can ship.
