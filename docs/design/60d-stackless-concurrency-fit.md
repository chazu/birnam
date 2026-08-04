# 60D — Stackless Concurrency Fit and Boundaries

> **Status:** S0 concurrency compatibility record, accepted 2026-08-04 with
> the 60E entry seam.
> This document evaluates the stackless Machine design against Birnam's existing
> reactor and isolate direction. It does not select a scheduler, introduce
> shared-memory threads, or make Machine transferable between runtimes.

## 1. Question

Does the stackless execution design constrain Birnam's future concurrency model,
and which implementation details must be correct now to avoid creating an
accidental obstacle later?

The short answer is:

- cooperative concurrency within one Runtime is a good fit;
- process-isolated parallelism composes without changing the StepMethod calling
  contract;
- multiple Machines may be interleaved on one owner thread;
- one Machine may not be driven concurrently or recursively;
- thread migration and work stealing are intentionally unavailable initially;
  and
- several OS threads sharing one Runtime remain blocked by broader runtime
  ownership and synchronization questions, not by the Method-shaped StepMethod
  contract.

The principal implementation hazard is reentrancy. A StepMethod is invoked by
the Machine driver and may synchronously send a microcode message back to the
same Machine. The driver must therefore release every MachineState borrow and
registry lock before invoking generated code.

## 2. Related contracts

This record reads the following documents together:

- [48 — Concurrency, reactors, and
  isolates](48-concurrency-reactors-and-isolates.md)
  prefers a single-threaded reactor for concurrent I/O and separate Runtime
  processes with copied messages for CPU parallelism;
- [60A — Execution semantics](60a-stackless-execution-semantics.md) defines
  owner-thread Machine semantics, suspension, native barriers, and one-shot
  continuation restoration;
- [60B — Machine, Frames, and GC](60b-stackless-machine-and-frames.md) defines
  private Frame storage, Machine lifecycle, traced roots, and generation pins;
  and
- [60C — StepMethod calling contract](60c-stepmethod-calling-abi-options.md)
  accepts the existing Method shape with Machine as receiver, a
  descriptor-declared argument buffer, and StepAction as result; and
- [60E — Resumable registration and the entry
  seam](60e-resumable-registration-and-entry.md)
  decides how an unmigrated send enters a resumable implementation, which adds
  the nested-drive and drive-stack cases this record must cover.

This document does not reopen Design 48's choice to avoid shared-memory
parallelism in the first concurrency architecture. It records whether Designs
60A–60C preserve that path and where stronger concurrency would require a later
decision.

## 3. Concurrency and parallelism are different requirements

Birnam's intended mechanisms solve two different problems:

| Requirement | Preferred mechanism | Stackless fit |
|---|---|---|
| Many pending I/O operations | One reactor, one active turn | Direct fit |
| Many suspended Birnam computations | Several Machines on one owner thread | Direct fit, but see 60E §5.3: while the callback-driving primitives are synchronous, an entry barrier sits below most real work and suspension is reachable only on a fully migrated probe |
| CPU parallelism | Process-isolated Runtimes with copied messages | Composes cleanly |
| In-process parallelism without shared state | Several isolated Runtimes | Possible later, existing module/runtime constraints apply |
| Shared-heap parallelism | Several threads sharing one Runtime | Not supported |
| Work-stealing fibers | Transfer Machines or continuations between threads | Not supported initially |

Stackless execution is necessary for suspending a Birnam computation without
retaining an unbounded native stack. It is not, by itself, a scheduler, a memory
model, or a thread-safety mechanism.

## 4. Why the design fits a reactor

A reactor needs to stop one logical computation at a safe point, retain its
state, perform another turn, and later resume the first computation. Machine,
Frame, StepAction, and Continuation provide exactly those pieces:

- Machine identifies one logical execution;
- private Frames retain resumable control and rooted values;
- SuspendAction returns control to the host without preserving a native stack;
- a suspended Machine or Continuation remains a process-local Oop;
- the reactor can retain that Oop as a queued root; and
- resume re-enters the same owner-thread driver.

The scheduler does not need to know Frame layout or StepMethod calling details.
It only needs a host-visible drive/suspend/resume result and an ownership rule
for queued Machine or Continuation Oops. Those host interfaces remain a later
record.

The Method-shaped StepMethod calling contract is helpful here because Machine
is an explicit receiver. Generated code does not depend on a process-global or
thread-local current Machine. Every Frame lookup and microcode operation starts
from the supplied Machine Oop.

An implementation must preserve that property. A process-global or thread-local
`CURRENT_MACHINE` or `CURRENT_FRAME` would make nested Machines, deterministic
tests, and later reactor scheduling unnecessarily fragile. Temporary raw
pointers
inside one Rust operation are acceptable; ambient execution identity is not.

There is exactly one scoped exception, and it is not in generated code. The
entry seam of [60E §5.2](60e-resumable-registration-and-entry.md) has no Machine
in hand by construction — an entry thunk is reached by ordinary dispatch from an
unmigrated caller — so the Runtime owns a drive stack naming the active drive
whose synchronous barrier is dynamically in scope. Only the entry shim consults
it, and the epoch it validates against is the Machine's, not the stack's.
Generated StepMethod code and Machine microcode still receive Machine
explicitly and must never read it.

## 5. Scheduling boundary

The Machine driver regains control only when a StepMethod returns a StepAction.
That return is the first scheduling safe point.

A future cooperative scheduler may:

- count StepAction transitions against a turn budget;
- continue driving the same Machine;
- enqueue the Machine at the back of a ready queue;
- stop on SuspendAction and associate the suspended handle with I/O readiness;
- stop on completion or failure; or
- request cancellation at the next safe point.

It may not preempt arbitrary generated code in the middle of a StepMethod.
Therefore every StepMethod must execute a bounded amount of work. Compiler
lowering must not place an unbounded source loop, recursive send chain, or
blocking wait inside one step.

Synchronous Methods are larger atomic regions. A long computation or blocking
FFI call in a synchronous Method blocks the complete owner-thread reactor until
that Method returns. Such work must eventually be:

- lowered into resumable StepMethods;
- expressed through nonblocking I/O and SuspendAction;
- bounded explicitly; or
- submitted to an isolate for parallel execution.

This is not a new limitation introduced by 60C. It is the native/foreign barrier
already defined by 60A, now made visible as a scheduling and fairness boundary.

## 6. Reentrant MachineState access

### 6.1 The call shape

The Method-shaped contract creates this valid control path:

```text
Machine driver
  -> invoke StepMethod(machine, declared-buffer, declared-count)
       -> IdApi.send(machine, machine-microcode-selector, ...)
            -> synchronous Machine primitive
                 -> access the same MachineState
       <- StepAction Oop
  <- StepAction Oop
```

The driver and Machine primitive both need MachineState, but they may not hold
overlapping mutable Rust borrows or recursively acquire a non-reentrant lock.

### 6.2 Required borrow discipline

One step must follow this sequence:

```text
1. Borrow MachineState.
2. Validate ownership and lifecycle.
3. Mark the step active and advance the Machine's single drive epoch.
4. Copy the Method address and immutable descriptor identity needed for entry.
5. Release the MachineState borrow and every registry guard.
6. Invoke generated StepMethod code.
7. Machine microcode sends independently borrow, validate, mutate, and release.
8. Reacquire MachineState after StepMethod returns.
9. Validate and decode the returned StepAction, copying every operand needed
   into driver-local scalars rather than re-reading the payload later.
10. Commit the driver transition and release the borrow before callbacks.
```

No `&mut MachineState`, `RefCell` guard, registry guard, mutex guard, arena
borrow, or borrowed Frame pointer may survive step 5. The same rule applies
before:

- IdApi.send;
- allocation or collection;
- a synchronous Method call;
- a foreign callback;
- a trace or finalization callback; and
- host notification.

The driver may retain copied scalar identities and pinned code-generation
handles whose contracts explicitly survive the call. It may not retain an
unvalidated pointer into mutable Frame storage.

### 6.3 Locking implication

Adding a mutex around MachineState does not make the design concurrent. If that
mutex is held across generated code, ordinary Machine protocol sends deadlock.
If it is released, lifecycle state and epochs still must reject another driver
that attempts to run the same Machine before the first StepMethod returns.

The correct invariant is exclusive logical drive with short physical borrows,
not one large lock around a complete Machine turn.

## 7. Exclusive drive and per-Machine scratch state

One Machine has at most one active driver invocation *per barrier delimiter*.
Stacked drives are legal and are how an unmigrated frame is crossed: when a
synchronous Method reached from inside a drive sends into a resumable
implementation, the entry seam pushes above an entry-barrier delimiter and runs
a nested drive on the same Machine
([60E §5.1](60e-resumable-registration-and-entry.md)). Those drives are stacked,
not concurrent — the outer one is parked inside a native activation at a known
point — and the nested drive may not pop below its delimiter.

What remains forbidden is a drive that does not establish a barrier. The
following operations must fail deterministically:

- recursive drive of a Running Machine that does not establish an entry barrier,
  including a host call to drive a Machine that is already driving;
- concurrent drive from another host call;
- drive or restore from a non-owner thread;
- restore while the Machine is Running;
- use of a StepAction from another Machine; and
- use of a StepAction issued in an expired drive epoch.

This exclusivity permits each Machine to reuse scratch StepAction Oops. Scratch
identity and payload are per-Machine, not process-global. The driver validates
the Machine generation and drive epoch before decoding the action.

Decoding must complete before any operation that can run generated code, not
merely before another Machine's turn: a stacked drive on the *same* Machine
rewrites the same scratch payload, so the cross-Machine framing of this rule is
the wrong axis. The driver copies the operands it needs into driver-local
scalars before releasing its borrow, and monotonic epochs
([60E §5.1](60e-resumable-registration-and-entry.md)) make a stale re-read fail
rather than decode.

Different Machines must not share:

- current Frame identity;
- drive epoch;
- scratch StepAction payloads;
- delimiter state;
- suspension state; or
- mutable driver-local argument buffers.

They may share immutable method code and FrameDescriptors while each holds the
appropriate code-generation pin.

## 8. Owner-thread affinity and work stealing

The first implementation binds Machine and Continuation to the owner thread of
their Runtime. Suspension does not remove that affinity. A reactor may resume a
Machine later, but only on the same owner thread.

This rules out work stealing and fiber migration in S1–S3. Supporting either
would require a separate review of:

- Runtime and heap ownership;
- GC stack-bottom and safepoint behavior;
- scoped root-provider ownership;
- native and foreign thread-local state;
- module and IdApi process globals;
- code-generation pin transfer;
- continuation and delimiter owner tokens; and
- callback entry from foreign threads.

The existing Method function shape does not encode a thread identifier and does
not itself prevent migration. Machine carries the ownership identity. A future
design can change Machine ownership without changing the StepMethod carrier if
the surrounding Runtime becomes capable of safe migration.

No current contract promises that this work will be desirable. Process isolates
remain the preferred source of parallel execution.

## 9. Shared-runtime parallelism

Several threads cannot safely drive different Machines against one current
Runtime merely because each Machine has private Frames. They still share:

- allocation and the object heap;
- interning;
- vtable and dispatch dictionaries;
- Store indexes and fact history;
- garbage collection;
- compiled-unit activation;
- code generation and loader state;
- module globals and the process-owned IdApi; and
- foreign libraries whose thread-safety contracts vary.

Those systems are currently unsynchronized or owner-thread-oriented. Adding
locks only to MachineState would leave the actual shared state unsafe.

If in-process parallelism is revisited, the preferred intermediate design is
several isolated Runtime instances with copied messages. That direction must
also address current native modules that retain a process-global IdApi pointer
or singleton state. The StepMethod calling contract neither creates nor solves
that pre-existing module boundary.

## 10. GC and scheduler roots

A scheduler queue that retains a suspended or ready Machine becomes a host root
for that Machine Oop. Reachability then flows through the Machine trace hook to
its active Frame Oop slots and scratch actions. A queued Continuation similarly
traces its detached segment.

The registry itself remains a capability lookup table, not a root set. Otherwise
an abandoned Machine would remain alive merely because it once had a registry
entry.

For the owner-thread reactor, collection occurs at existing synchronous
safepoints. The reactor must not mutate ready queues or resume Machines from a
foreign thread during tracing. A foreign I/O thread may signal readiness into a
thread-safe host queue, but entry into Birnam and Machine resumption occur on
the
owner thread.

Concurrent or moving collection would require a separate memory-model design.
The private Frame representation leaves room for that work, but this record does
not claim support for it.

## 11. Live code evolution across concurrent Machines

Several Machines may legally retain Frames from different code generations at
the same time. Every Frame pins the generation containing its StepMethod and
FrameDescriptor until the Frame is popped, destroyed, or detached and later
consumed.

Live dispatch changes occur at safe points between Machine turns. After a
Machine resumes, a new CallAction resolves its target through current dispatch;
an already active Frame continues through its pinned implementation generation.

A reactor must not asynchronously rewrite a Running Frame when activating a
compiled unit. Activation may update future dispatch while old Frames remain
pinned. This preserves atomic turns without requiring all Machines to reach the
same generation simultaneously.

## 12. Isolate composition

Process isolates remain independent of the stackless calling contract:

- each worker owns its Runtime, heap, IdApi, Machines, and reactor;
- copied values and explicit envelopes cross the process boundary;
- Machine, FrameView, StepAction, and Continuation Oops do not cross;
- suspension state remains process-local and non-durable; and
- worker crash or termination cannot leave a shared MachineState lock held in
  the coordinator.

A worker may later run one reactor and many Machines. Isolates then provide CPU
parallelism while each worker's reactor provides concurrent I/O and cooperative
task interleaving.

## 13. Cancellation and interruption

Cancellation is scheduler policy, not a new asynchronous entry into generated
code. The safe initial model is:

1. record a cancellation request in host or Machine state;
2. observe it when the driver regains control at a StepAction boundary;
3. convert it into a validated AbortAction or later language-level cancellation
   protocol; and
4. unwind or detach Frames according to an independently reviewed policy.

A signal handler or foreign thread must not mutate Frame slots, action payloads,
or continuation state directly. It may only set or enqueue a narrowly defined
notification whose effects are applied by the owner thread.

This is the same rule used for readiness, timers, debugger requests, and future
preemption counters.

## 14. Concurrency invariants

The first implementation must preserve all of the following:

1. Machine identity, not TLS or a process global, selects current execution
   state. The Runtime drive stack of §4 is the sole exception, is read only by
   the entry shim, and is never read by generated code or microcode.
2. One Machine has at most one active driver invocation per barrier delimiter;
   stacked drives are legal, unbarriered recursive drives are not.
3. Different Machines may alternate turns without sharing mutable execution
   state.
4. MachineState and registry borrows never cross generated code, IdApi.send,
   allocation, GC, foreign calls, or callbacks.
5. Scheduling and cancellation take effect only at StepAction boundaries.
6. StepMethods contain bounded work; synchronous Methods and FFI remain explicit
   atomic barriers.
7. A queued Machine or Continuation is rooted by the scheduler, not by the
   capability registry.
8. Machine and Continuation remain owner-thread-affine and process-local.
9. StepAction scratch state is per-Machine and drive-epoch validated.
10. Active Frames pin their code generation while new sends remain late-bound.
11. Foreign threads signal the owner; they never enter the Runtime directly.
12. Shared-runtime parallelism is not inferred from the existence of private
    Frames.
13. A vtable never holds a StepMethod address, so no scheduler, callback, or
    reflective send can enter generated resumable code except through the two
    paths of 60E §3.
14. The drive stack is a distinct borrow domain from the machine registry and
    MachineState; entry acquires and releases it as a point operation, never
    across generated code.
15. A Machine has one drive epoch, advancing on drive entry and on drive exit.
    Validation compares against that single current value, so an outer drive's
    action is stale during a nested drive and stays stale afterward rather than
    revalidating over rewritten scratch payload.
16. Fresh-versus-nested entry is decided by synchronous-barrier scope, not by
    thread activity, so a host or reactor callback taken while some Machine is
    driving begins a new Machine rather than joining an unrelated one.

## 15. Acceptance tests

The stackless substrate is concurrency-compatible only when tests demonstrate:

1. **Alternating Machines:** two Machines alternate at least one million
   StepAction transitions on one owner thread without native-stack growth or
   cross-Machine Frame state.
2. **Scratch isolation:** each Machine can reuse scratch StepActions without the
   other Machine observing payload or epoch changes.
3. **Recursive-drive rejection:** a synchronous callback that attempts to drive
   its already Running Machine directly — without establishing an entry barrier
   — receives the specified deterministic failure. The barriered entry path of
   60E §5.1 must succeed in the same test, so the two cases are distinguished
   rather than conflated.
4. **Concurrent-drive rejection:** two host attempts to drive one Machine cannot
   both enter generated code; the non-owner or losing attempt fails before
   touching Frame storage.
5. **Reentrant microcode:** a StepMethod can send every primitive Machine
   microcode message without a retained registry borrow, `RefCell` guard,
   mutex guard, or mutable Frame borrow causing panic or deadlock.
6. **Queued rooting:** a suspended Machine retained only by a scheduler test
   queue survives forced collection with all live Frame Oops intact.
7. **Abandoned collection:** removing that final queue root permits Machine
   finalization and releases external Frame state exactly once.
8. **Generation overlap:** two Machines may run Frames from different pinned
   generations while new sends resolve through current dispatch.
9. **Owner-thread rejection:** resume, restore, and destruction from a foreign
   thread fail without invoking generated code or mutating lifecycle state.
10. **Safe cancellation:** a cancellation request becomes visible at the next
    StepAction boundary and never mutates a Running Frame asynchronously.
11. **Blocking barrier evidence:** a deliberately blocking synchronous leaf is
    shown to block the single reactor thread, documenting the boundary rather
    than accidentally claiming preemption.
12. **Isolate boundary:** execution Oops are rejected by copied-value transport
    while ordinary durable/copyable values continue to cross successfully.

Tests 3–5 are part of S1 even before a public scheduler exists. Tests 1, 2, and
6 may use a deterministic two-Machine host harness rather than a reactor API.
The remaining tests gate the corresponding scheduling, live-evolution, or
isolate integration slices.

## 16. Deferred decisions

This record does not decide:

- scheduler object model or Birnam selectors;
- ready-queue discipline, fairness, priorities, or quotas;
- timer and I/O adapter APIs;
- structured cancellation or nursery policy;
- multi-shot continuations;
- work stealing or Machine migration;
- shared-memory threads;
- concurrent or moving GC;
- in-process multi-Runtime module loading; or
- durable representation of in-flight execution.

These policies may be added above the Machine driver or reviewed as explicit
changes to Runtime ownership. None is required to validate the Method-shaped
StepMethod contract.

## 17. Assessment

The stackless design does not pose a particular obstacle to Birnam's intended
reactor-plus-isolate concurrency model. It improves that fit by making suspended
execution explicit, rooted, and independent of native-stack depth.

Its real constraints are deliberate:

- cooperative rather than asynchronous preemption;
- one owner thread and one active drive per Machine;
- synchronous native and foreign barriers; and
- no assumption that private Frames make the rest of Runtime thread-safe.

The implementation-sensitive risk is narrower: ordinary Machine protocol sends
re-enter MachineState while the driver is active. Strict short-borrow discipline
and deterministic exclusive-drive checks are therefore part of the architecture,
not optional Rust cleanup.
