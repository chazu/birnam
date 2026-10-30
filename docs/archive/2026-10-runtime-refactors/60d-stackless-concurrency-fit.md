# 60D — Stackless Concurrency Fit and Boundaries

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

> **Status:** S0 concurrency compatibility record, accepted 2026-08-04 with
> the 60E entry seam; recognition-concurrency, same-execution-entry-scope, and
> resolve-and-pin follow-up and live-dispatch-timing amendments accepted
> 2026-08-05; Runtime ownership accepted 2026-08-06.
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
- one Machine may not be driven concurrently or recursively without an entry
  barrier; same-execution stacked drives are the explicit exception;
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
  the nested-drive and entry-scope-stack cases this record must cover.

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

### 3.1 Runtime is the ownership island

One Birnam Runtime owns one heap, Store, dispatch environment, Machine registry,
entry registry, and set of Runtime-local generation pins. Its host or reactor is
an executor: it enters the Runtime, drives one turn at a time, and retains queued
Machine Oops as roots. It does not own or duplicate Runtime execution state.

The first implementation has one owner thread per Runtime and many cooperatively
interleaved Machines. CPU parallelism uses isolated Runtimes with copied values,
initially as processes. Future in-process isolates may share immutable code
mappings through a process repository while retaining independent registrations,
heaps, Stores, dispatch, Machines, and pins. They do not share Oops or mutable
Runtime state.

The process code repository has physical custody of TCC/dylib mappings. Each
Runtime holds leases and supplies the safety verdict for its own execution; the
repository may dispose a mapping only after all Runtime leases have drained.
This is the only process-shared lifetime accounting implied by the design. It
does not make one Runtime safe for shared-memory execution.

Runtime entry is explicit at each top-level host or reactor turn. TLS may route
existing context-free C ABI shims to that entered Runtime, including the emitted
registration shim, but it is not semantic execution identity. Generated code
receives Machine explicitly, and fresh-versus-nested entry is decided by the
Runtime's same-execution scope rather than by thread activity.

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
in hand by construction—`send_abi` may classify a resumable target while
serving an unmigrated caller or a late-bound microcode send from a StepMethod,
and a directly invoked fallback thunk may do the same—so the Runtime owns an
entry-scope stack naming the Machine whose native activation is waiting. The
driver pushes that scope around StepMethod and synchronous-Method invocation.
Only the Runtime entry helper consults it, and the epoch
it validates against is the Machine's, not the stack's. Generated StepMethod
code and Machine microcode still receive Machine explicitly and must never read
it.

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
blocking wait inside one step. [60F](60f-resumable-compiler-lowering.md) makes
every migrated source send a driver boundary and defines the bounded CFG/PC
contract used by S2.

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
6. Push a same-execution entry scope, invoke generated StepMethod code, pop the
   scope, and retain no scope-stack borrow across the invocation.
7. Machine microcode sends independently borrow, validate, mutate, and release;
   a resumable replacement enters through the scope and completes above an
   entry-barrier delimiter on the same Machine.
8. Reacquire MachineState after StepMethod returns.
9. Validate and decode the returned StepAction, copying and rooting every Oop
   operand needed rather than re-reading the payload later.
10. For descriptor layout v2, validate the action against the publisher's
    pending-transition record, commit 60F's non-allocating edge-transfer plan,
    clear reassigned slots, advance the root-state PC to the semantic successor
    PC already installed by `continueAt:` or `callSite:`, and clear the pending
    record while the borrow is held.
11. Release the borrow before protected lookup, generated-code entry, or any
    other callback. Reborrow only for the next validated Runtime mutation.
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
Stacked drives are legal and are how a waiting native activation is crossed:
when a synchronous Method or StepMethod reached from inside a drive sends into
a resumable implementation, the entry seam uses the same-execution entry scope,
pushes above an entry-barrier delimiter, and runs a nested drive on the same Machine
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

The Runtime-qualified carrier registry of 60E §4.1 does not add a migration
restriction. Its shared logical resolve-and-pin operation protects ordinary
lookup through classification and generation-pin acquisition for both
`send_abi` and Machine calls; after a successful push the Frame owns that pin
independently of dispatch. Work stealing would transfer the Machine and its
acquired pins, not re-resolve active Frames by carrier address.

### 8.1 Required follow-up before concurrent evolution

The owner-thread S1 implementation uses a plain owner-thread epoch/depth token,
amortized across a Machine turn or top-level send, because metacircular lookup
may re-enter mutation even without another thread. It must keep resolution non-
allocating and free of per-call locks, atomic read-modify-write operations, and
reference-count traffic. It must still centralize the logical resolve-and-pin
API, direct-binding reference counts, retirement state, and pin-transfer cleanup
so concurrency does not require a new calling contract.

Before shared-Runtime parallelism, concurrent dispatch mutation, or concurrent
code unload is enabled, a follow-up record and implementation must add:

- a read-side epoch, RCU, immutable-snapshot, or equivalently reviewed protocol
  spanning lookup through classification and pin acquisition;
- retirement epochs that let a reader which resolved before mutation acquire
  the old entry, while preventing readers begun after retirement from reviving
  it;
- process-shared mapped-generation lifetime accounting when independent Runtime
  registries share immutable code;
- explicit generation leases for native or custom-lookup caches that retain a
  raw Method carrier beyond one protected resolution;
- bounded retirement-queue behavior and diagnostics for stalled readers; and
- the uncontended, contended, and mutation-throughput evidence required by 60E
  §11.

The read-side mechanism may be amortized over a Machine turn or top-level host
send, and nested resolution should reuse it. A mutex, global atomic increment,
heap allocation, or `Arc` clone on every send is not an acceptable default
without evidence that it meets or beats the recorded budget. Metacircular
lookup may have effects, so a sequence-counter design may not silently retry it.

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
that pre-existing module boundary. Each Runtime has an independent recognition
map keyed by `(Runtime identity, thunk address)`, so sharing immutable mapped
code does not merge their ResumableEntry or Machine state.

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

For the owner-thread S1 Runtime, live dispatch changes occur only after the
current StepMethod has returned and its StepAction operands have been copied and
validated, while no StepMethod native activation is executing. A Frame may
remain active across many such turn boundaries. Its captured StepMethod and
descriptor stay pinned, while its next CallAction or TailCallAction performs a
fresh resolve-and-pin against dispatch current at that operation's
linearization point.

A reactor must not asynchronously rewrite a Running Frame when activating a
compiled unit. Activation may update future dispatch while old Frames remain
pinned. Before concurrent mutation is enabled, §8.1 must define how an executor
observes atomically either the old or new binding even when another Machine is
mid-turn. This preserves atomic native activations without requiring all active
Frames to share one generation.

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
   state. The Runtime entry-scope stack of §4 is the sole exception, is read
   only by the Runtime resumable-entry helper serving `send_abi` or a fallback
   thunk, and is never read by generated code or microcode.
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
14. The entry-scope stack is a distinct borrow domain from the machine registry
    and MachineState; entry acquires and releases it as a point operation, never
    retaining a stack borrow across generated code. The dynamic scope record
    itself remains installed until the native activation returns.
15. A Machine has one drive epoch, advancing on drive entry and on drive exit.
    Validation compares against that single current value, so an outer drive's
    action is stale during a nested drive and stays stale afterward rather than
    revalidating over rewritten scratch payload.
16. Fresh-versus-nested entry is decided by inherited same-execution entry
    scope, not by thread activity. A host or reactor callback beginning
    unrelated work suppresses that scope and begins a new Machine rather than
    joining an unrelated one.
17. Resolve-and-pin protects metacircular lookup through classification and
    generation-pin acquisition. It returns a pinned Runtime-local
    ResumableEntry, a lifetime-protected synchronous Method, or unbound; it
    never exposes a bare address across a retirement gap.
18. Direct binding aliases own independent references, borrowed custom-lookup
    results require a lease to outlive one protected resolution, and retirement
    cannot reclaim or reuse an address until binding references, readers,
    acquired entries, Frames, Continuations, and leases quiesce.

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
   mutex guard, or mutable Frame borrow causing panic or deadlock. Replacing one
   microcode method with a resumable implementation reuses the same Machine,
   preserves the waiting outer Frame, and cannot suspend or capture across its
   entry barrier.
6. **Queued rooting:** a suspended Machine retained only by a scheduler test
   queue survives forced collection with all live Frame Oops intact.
7. **Abandoned collection:** removing that final queue root permits Machine
   finalization and releases external Frame state exactly once.
8. **Generation overlap:** two Machines may run Frames from different pinned
   generations. On the owner-thread path, mutate dispatch after one StepMethod
   returns but while its Frame remains active; that Frame's next CallAction sees
   the new binding while its next step still uses its pinned implementation.
   The future concurrent-evolution version additionally proves resolve-and-pin
   observes atomically either the old or new binding while another Machine is
   mid-turn.
9. **Owner-thread rejection:** resume, restore, and destruction from a foreign
   thread fail without invoking generated code or mutating lifecycle state.
10. **Safe cancellation:** a cancellation request becomes visible at the next
    StepAction boundary and never mutates a Running Frame asynchronously.
11. **Blocking barrier evidence:** a deliberately blocking synchronous leaf is
    shown to block the single reactor thread, documenting the boundary rather
    than accidentally claiming preemption.
12. **Isolate boundary:** execution Oops are rejected by copied-value transport
    while ordinary durable/copyable values continue to cross successfully.
13. **Resolution handoff and isolation:** pause a deterministic reader after
    metacircular lookup returns a thunk but before classification, retire its
    final direct binding, and then prove the reader acquires the old entry and
    an observable generation pin rather than misclassifying it as synchronous.
    A reader begun after retirement cannot revive the entry. Two Runtime test
    harnesses may register the same mapped thunk address independently, and
    retiring it from one does not disturb the other.

Tests 3–5 and 13 are part of S1 even before a public scheduler exists. Tests 1,
2, 6, and 13 may use a deterministic host harness rather than a reactor API.
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
