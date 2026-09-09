# 74 — Malleable multicore execution and guarded optimization

> **Status: proposed for review, 2026-09-06.** The user requested this design
> inquiry and the description of a bounded execution experiment. No experiment,
> scheduler, parallel Runtime, or optimizer is implemented by this document.
> The user selected Self as the optimization direction on 2026-09-06, with
> arbitrary native execution through IdApi retained as a core requirement.
> Concurrency architecture, implementation mechanisms, and experiment thresholds
> remain proposed. Existing semantic contracts remain in force.
> **2026-09-09: E0 (section 7.2) was delivered as Design 74A and accepted by
> the user, who then authorized E1 (section 7.3): one guarded slot-access
> path in a development-only artifact under explicit selection. E2–E3, the
> 20% continuation threshold, and every concurrency mechanism remain
> proposed.** The timing acceptance that was pending closed the same day.

## 1. Decision criteria and direction

The user supplied two criteria:

1. Preserve Birnam's malleability and a path to self-hosting.
2. Get as close as practical to the execution speed of a system such as
   Objective-C.

These criteria apply independently to **single-core execution cost** and
**parallel execution**. More workers do not remove the cost of an individual
Birnam loop. Conversely, a faster loop does not supply a scheduler or a safe
shared-memory object model. Neither criterion establishes a numerical speedup
promise without comparable workloads and measurements.

The selected optimization direction is Self's approach: specialize native code
using observed behavior, guard its assumptions, and preserve live programming
through invalidation and, where needed, deoptimization. This selects an approach,
not Self's exact VM, object representation, scheduler, or foreign interface.
It does not select shared versus isolated or branching object worlds. Arbitrary
native methods using Birnam's IdApi remain a core feature; compiler recovery
metadata cannot become a prerequisite for ordinary native participation.

The recommended direction to investigate is:

- preserve ordinary live objects, reflection, and current lookup behavior;
- make execution cheaper through guarded optimizations with correct fallback;
- keep mutable state under an explicit execution owner, with independent owners
  able to run on different cores;
- keep local sends synchronous and communication between owners explicit; and
- retain a small mechanism boundary over which compiler, library, and scheduling
  policy can be authored in Birnam.

The first proposed deliverable is the single-core experiment in section 7.
Concurrency architecture is compared here, but is not selected irrevocably.
In particular, the scope of coherent live changes across several owners is an
open semantic decision, not an implementation detail to hide inside a pool.

## 2. What exists today

| Piece | Implemented behavior | Missing capability |
| --- | --- | --- |
| Stackless Runtime | Explicit Machines and Frames, resumable execution, suspension and one-shot continuation machinery, rooted execution state, code-generation lifetime protection | General task scheduling, readiness integration, fairness policy, and parallel driving of a shared Runtime |
| Process isolates | Clean worker process boot, copied-value wire protocol, blocking request/reply and bounded lifecycle handling | Submission to multiple workers before waiting, asynchronous completion, pool policy |
| Reactor | Design direction and some socket preparation | A complete general event loop integrated with runnable and suspended Machines |
| Multiple Runtime instances on threads | Internal Runtime ownership is explicit | Public API and native modules still retain process-wide runtime state |

The [isolate guide](../guide/06-isolates.md) describes the shipped blocking API.
In [the request implementation](../../rust/birnam-isolate/src/lib.rs),
`handle_request` writes and reads a complete exchange while holding the worker
registry mutex. Existing independent handles do not make sequential calls into
overlapping jobs. An asynchronous transport must change that ownership and wait
structure; calling the existing API from foreign threads is not a solution.

[Runtime](../../rust/birnam-id/src/runtime.rs) enforces owner-thread entry.
`run_stackless_loop` drives execution until a terminal result or explicit
suspension/capture; it does not provide a general timeslice budget. Machines
share their Runtime's heap, Store, and dispatch environment. They are not
separate isolated heaps. Native synchronous callbacks can establish entry
barriers that prevent suspension through their activations.

[The public IdApi](../../rust/birnam-id/src/public_api.rs) admits one
process-lifetime initialization. Native modules including
[Store](../../rust/birnam-store/src/lib.rs),
[collections](../../rust/birnam-collection/src/lib.rs), and
[portable code](../../rust/birnam-portable-code/src/runtime.rs) retain singleton
runtime/API state. Moving internal Machine state to an OS thread pool would not
make those modules safe for independent threaded Runtimes.

This is an incomplete concurrency product, not evidence that the delivered
suspension mechanism is malfunctioning. Design 48 contains historical constraints
that predate stackless execution; read its retained isolate contract together
with [60A](60a-stackless-execution-semantics.md) and
[60D](60d-stackless-concurrency-fit.md), rather than treating every historical
statement as current implementation status.

## 3. Properties to preserve

### 3.1 Malleability and authority

The following are requirements for this investigation:

- Objects and supported meta-level operations remain accessible through Birnam.
  Method replacement, removal, inheritance, DNU, and custom lookup keep their
  ordinary semantics. An optimization cannot silently skip effectful lookup.
- Receiver and argument evaluation order, Error values, capture identity, global
  and Store reads, and mutation visibility at each semantic send remain intact.
- An active Frame retains the implementation it entered. Later sends resolve
  current dispatch. Optimizer invalidation must not substitute the newest method
  body for an already-active older body.
- Supported post-bootstrap MOP changes retain Store arbitration and an honest
  mirror of the canonical live object dictionaries. There is no second dispatch
  authority inside a compiler cache.
- Identity remains meaningful, including distinct boxed values where the current
  system preserves them. Numeric equality is not permission to merge objects.

These follow [42](42-object-canonical-relational-derived.md),
[60F section 4](60f-resumable-compiler-lowering.md#4-semantic-preservation-and-late-binding),
and [60O](60o-image-v8-identity-preservation.md). Design 60F currently prohibits
call-site target caching and defines no optimization tier. This proposal does
not override that rule. A future reviewed optimization contract must explain its
equivalence argument, invalidation, and lifetime protection before such a tier
is enabled.

The current Machine microcode protocol is also observable. Treating it as
unconditionally private would narrow an accepted contract. Investigate preserving
its overridability through guards before proposing that reduction in malleability.

### 3.2 Execution, ownership, and persistence

- Root all live values across allocation, callbacks, GC, suspension, and fallback.
  No mutable Runtime/Frame borrow or registry lock spans an ordinary send.
- Keep native-stack use bounded for resumable recursion and control. Optimized
  execution must not turn an unbounded loop into an indivisible native activation.
- Respect native entry barriers. A timer, signal, or worker thread cannot unwind
  an arbitrary FFI activation or resume a Machine on the wrong owner.
- Preserve code lifetime through resolution, execution, fallback, suspension, and
  retirement. Invalidating a cache is not permission to free active code.
- Machine state, continuations, OS handles, and optimizer assumptions are
  process-local. Images recover portable executable meaning and rebuild native
  machinery; they do not claim to resume old OS activity.

### 3.3 Self-hosting

Keep the independent source-only cold bootstrap with the declared non-Birnam
toolchain, currently Rust. Historical Zig references in
[44](44-toward-self-hosting.md) describe earlier implementations, not a permanent
language requirement; [62](62-rust-native-rungs-migration-plan.md) records the
native-rung and cold-compiler migration.

A Birnam-authored compiler may emit efficient native operations. A Birnam-authored
scheduler may use primitive frame and OS operations. Neither requires a second
permanent copy of library semantics in Rust. Conversely, merely moving a function
from Rust to Birnam is not useful if it creates unacceptable execution overhead
or leaves two competing authorities indefinitely.

## 4. Concurrency approaches

| Approach | Multicore execution | Malleability and sharing | Principal costs and risks |
| --- | --- | --- | --- |
| Owner-thread reactor with many Machines | No, within one Runtime | One live object environment; tasks interleave | Scheduler roots, fairness, cancellation, and blocking native boundaries |
| Process-isolated Runtimes | Yes | Each worker remains a live Birnam environment; values cross the wire | Startup, copied data, aggregate memory, transport/lifecycle policy, coordinated live changes |
| Thread-hosted isolated Runtimes | Yes | Same logical isolation; local mutation remains serialized | Remove API/module singletons, qualify roots and FFI ownership, coordinate shared code lifetimes; copying remains part of the current isolate contract |
| Parallel sessions over a transactional repository | Yes | Shared persistent identities with session views and explicit commits | Object-version visibility, conflicts/retries, retention, code activation and transaction integration |
| Threads sharing one live Runtime/heap | Yes | Closest to unrestricted shared live objects, after defining concurrent semantics | Concurrent GC, object/Store synchronization, effectful lookup, atomic publication, code reclamation, deadlocks and contention |

The reactor is useful in combination with a parallel backend, but cannot by
itself satisfy the multicore criterion. A global lock around a shared Runtime
would serialize Birnam execution; finer locking needs a reviewed memory model.
Locks around Machine alone do not protect the rest of the Runtime.

GemStone is a concrete precedent for separate Smalltalk sessions sharing a
transactional object repository. Its sessions see consistent views and coordinate
changes at commit. This is relevant to Birnam's Store, but the existence of a
historied Store does not make Birnam a transactional multi-session object system.
[GemStone architecture](https://downloads.gemtalksystems.com/docs/GemStone64/3.7.x/GS64-ProgGuide-3.7/1-IntroToGemStone.htm)

### 4.1 Preferred candidate: parallel owners, local ordinary execution

Start the design with a Runtime as the ownership boundary. One Runtime may hold
many objects and computations; do not require one Runtime or OS process per
object. Each owner runs ordinary local sends without per-object cross-thread
locking. Independent owners may run simultaneously.

Process isolates are the shortest route from the current implementation to a
parallel-job proof. Thread-hosted Runtimes are a possible later backend if their
memory and transport benefits justify the module/API ownership work. They are
not automatically faster for a particular workload.

Several Machines interleaving within one owner can still observe each other's
mutations. Owner-thread serialization prevents simultaneous memory access; it
does not make a multi-send operation or a suspended request transactional. The
scheduler design must define yield points, critical operations, and reentry
behavior without implying that an entire request remains atomic across waits.

Continue to transfer only admitted copied values. Arbitrary Blocks, Oops,
continuations, and native pointers do not become transferable because their code
has a portable representation. Shared immutable data, ownership transfer, and
remote object references would each need an explicit protocol and lifetime design.

### 4.2 Live changes across owners remain a decision

Each owner must support inspection and supported live changes without requiring
worker restart. Requests to mutate an owner execute at its safe publication
boundary; existing active Frames retain their implementations and later sends
observe its current dispatch.

Two cross-owner policies are possible:

- **Independent activation:** each owner applies a change separately. Requests
  identify the intended code/data contract; mixed generations are explicit.
- **Coordinated activation:** prepare compatible changes, quiesce or fence new
  work in participating owners, publish under a defined group protocol, and
  handle partial failure. Pausing or acknowledging owners adds latency and a
  failure model. Process crashes cannot be wished away by a shared version number.

Neither is silently equivalent to one global image changing instantaneously.
If that stronger experience is required, its atomicity and availability rules
must be accepted before isolation is chosen as the final language model.

An initial immutable-image worker pool can prove parallel CPU execution, but
does not qualify live cross-owner evolution. Cancellation likewise needs an
explicit policy: a timeout does not prove that external effects did not happen,
and retrying an uncertain request may duplicate them.

## 5. Approaches to cheaper execution

### 5.1 What the Objective-C comparison means

Objective-C combines compiled method bodies and primitive operations with dynamic
message dispatch. Its runtime caches method lookup results. This is a useful
reference for dispatch and native execution, but not proof that arbitrary
Birnam metacircular behavior has the same cost.
[Apple's messaging guide](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/ObjCRuntimeGuide/Articles/ocrtHowMessagingWorks.html)

Use two separate comparisons: dynamically messaged object workloads, and primitive
numeric loops. A Birnam boxed arithmetic send and an Objective-C machine-integer
addition have different semantic obligations. Report the distance to both; do
not label the primitive comparison semantic equivalence or promise parity.

### 5.2 Alternatives and their disposition

| Approach | Opportunity | Review boundary |
| --- | --- | --- |
| Improve the existing execution path | Better indexes, validated metadata reuse, reduced private scratch allocation | Preserve semantics; useful independently of concurrency |
| Guarded native execution of canonical Machine operations | Reduce dispatch/bookkeeping while retaining an ordinary fallback | Prove the full lookup path and operation behavior; do not freeze the Machine protocol |
| Guarded method caches and later inlining | Reduce ordinary dispatch and expose optimization opportunities | Invalidation, effectful lookup, active-code protection, and recovery maps; requires a new optimization contract |
| Eliminate or unbox temporary values | Reduce allocation and improve arithmetic | Preserve identity, aliasing, reflection, failures, and GC-visible state; not part of the first experiment |
| Improve native backend optimization | Optimize eligible native regions | Does not remove opaque semantic calls on its own; measure compile latency, cold bootstrap, and portability |
| Declare frame mechanics permanently private or restrict live mutation | Simplify native access and optimization proofs | Explicit semantic narrowing; not the recommendation under the user's criteria |

The [current emitter](../../rust/birnam-codegen/src/resumable_c.rs) generates
ordinary Machine sends for slot reads/writes and action publication, including
boxed slot operands. [Design 66](66-stackless-throughput-recovery.md) explains
this cost model and distinguishes historical measurements from later recovery.
The initial work preceding Design 73 already improves indexes and immutable
metadata. The experiment must use a fixed current baseline, not attribute all
historical v0.14 costs to the current checkout.

Self provides precedent for inline caches and adaptive optimization, and for
recovering source-level execution when debugging or live changes invalidate
optimized code. These demonstrate techniques, not a Birnam speed estimate or a
proof that Self's semantic assumptions apply here.
[Polymorphic inline caches](https://bibliography.selflanguage.org/pics.html),
[dynamic deoptimization](https://bibliography.selflanguage.org/dynamic-deoptimization.html)

### 5.3 Native execution and the IdApi requirement

Self's VM compiles Self methods into real CPU machine code. The VM supplies the
compiler, object management, dispatch support, and execution services; "VM"
does not imply that method bodies are interpreted. Self also supports separately
compiled C/C++ routines through glue code, dynamic linking, and foreign function
proxies. That interface is precedent for coexistence, not a replacement for
Birnam's IdApi.
[Self compilation](https://handbook.selflanguage.org/2024.1/intro.html),
[foreign routines](https://handbook.selflanguage.org/2024.1/vmref.html#interfacing-with-other-languages)

Birnam must continue to admit native methods through the existing C-compatible
[Method and IdApi ABI](../../rust/birnam-abi/src/lib.rs). Native code may install
methods, allocate objects, and send messages under the existing ownership,
rooting, and lifetime contracts. It need not have been produced by the Birnam
compiler. Optimized implementations may use private internal conventions, but
must preserve ordinary entry through the public ABI.

Both compiler-generated methods and externally supplied methods are native
code. The relevant distinction is what the Runtime knows about their execution:
compiler-generated recovery metadata can describe locals and inlined frames;
an arbitrary native function need not provide that information. Such a function
remains callable without becoming deoptimizable or suspendable. Its supported
callbacks through IdApi remain ordinary Birnam execution, and their effects
must be visible to optimized callers and other reentrant Birnam code.

Initially, an unknown native call must be treated as potentially allocating,
mutating, and reentering Birnam. Publish required roots and recoverable caller
state before the call; invalidate or recheck affected assumptions before they
are reused, including inside callbacks. Supported mutations through IdApi must
participate in the same invalidation mechanism as source-level mutations. Any
existing permitted raw mutation that cannot be tracked must limit optimization
eligibility or require conservative revalidation, rather than silently changing
the native contract. Do not replay a native call during recovery. Arbitrary
native activations retain the existing suspension barrier; the Runtime cannot
reconstruct or migrate them merely because an optimized caller has metadata.

This boundary is compatible with a future compiler authored in Birnam: recovery
metadata is a compiler output contract, not a requirement that the compiler be
implemented in Rust. Linux support must preserve the public ABI and implement
the target's native calling, loading, and root-discovery mechanics.

### 5.4 Practical consequences of the native boundary

The operating system can preempt a native thread and run other threads. The
current limitation is that Birnam cannot save an arbitrary unfinished native
activation as a Machine, release its thread, and resume it later. Independent
workers could still run on other cores, subject to their ownership contracts.
This boundary does not itself impose a global interpreter lock.

The consequences are observable in ordinary programs:

- **Responsiveness:** a native function that computes for ten seconds or waits
  for blocking I/O occupies its execution thread throughout. A reactor on that
  thread cannot schedule other Machines during that call; timers and cancellation
  requests can be delayed. Being stackless alone supplies no latency bound.
- **Callback suspension:** consider `Birnam -> native sort -> Birnam comparator
  -> wait for I/O`. The native sort still expects a comparison result on its C
  stack. Saving only Birnam Frames cannot preserve that unfinished call. Under
  the current contract, suspension through that callback is rejected even when
  the comparator itself has a resumable implementation. A live replacement that
  introduces suspension can therefore expose a previously dormant boundary.
- **Cancellation:** stopping the wait for an answer does not prove that native
  work stopped or its external effects were undone. Safe cancellation requires
  cooperation or a suitable library mechanism. Forcibly canceling an arbitrary
  thread can leave locks or allocator state inconsistent. Killing an isolated
  process contains its local failure but still does not undo external effects.
  [Linux cancellation documentation](https://man7.org/linux/man-pages/man3/pthread_setcancelstate.3.html)
- **Native stack growth:** fully resumable call chains can keep native stack use
  bounded. Repeated recursion through synchronous native methods and Birnam
  callbacks still adds native activations and can overflow the native stack.
  [Design 60E's boundary analysis](60e-resumable-registration-and-entry.md#53-what-migration-does-and-does-not-preserve)
- **Live editing and recovery:** future ordinary sends must observe supported
  method changes, while active implementations retain their existing lifetime
  semantics. Birnam cannot reconstruct or rewrite the internal execution of an
  arbitrary running C function using compiler metadata for its Birnam caller.
  Richer recovery requires metadata for the affected execution itself.

The current failure behavior needs explicit review. The
[Machine suspension checks](../../rust/birnam-id/src/execution.rs) reject an
entry-barrier crossing with `ContinuationBarrier`. If that failure escapes
through an ordinary synchronous `IdApi.send`, the
[send boundary](../../rust/birnam-id/src/dispatch.rs) calls `fatal_invariant`,
which [aborts the process](../../rust/birnam-id/src/boundary.rs). This is an
implemented enforcement rule, not a graceful asynchronous error. Preserving the
native stack is necessary under the current mechanism; retaining process abort
as the user-facing outcome is a separate design question. Any replacement must
define error propagation through the existing ABI without treating a failed
suspension as successful completion or silently unwinding foreign frames.

### 5.5 Options for native calls that need to yield

Short synchronous native primitives fit the current boundary. Long native work
and native functions that invoke arbitrary Birnam callbacks need a deliberate
integration strategy:

| Approach | What it permits | Main obligations and limits |
| --- | --- | --- |
| Resumable implementation | Express control and saved state in Birnam Frames, returning at supported boundaries | Migrate callback-driving control as well as the callback; native leaves can remain synchronous |
| Asynchronous worker adapter | Run blocking work separately and resume Birnam when a result arrives | Define data ownership, cancellation, and result delivery; a worker cannot call IdApi against another thread's Runtime under today's contract |
| Retained native stacks, such as stackful fibers | Park a complete native call chain at cooperating switch points and later resume it | Account for stack storage and roots, foreign-library assumptions, locks, thread-local state, code lifetime, and platform context switching |

[Boost.Context](https://www.boost.org/doc/libs/latest/libs/context/doc/html/context/overview.html)
demonstrates the retained-stack mechanism. It does not automatically make a
blocking library call yield, make arbitrary callbacks safe to switch within, or
make migration between threads compatible with Birnam's current owner checks.
Retaining a stack also does not provide source-level deoptimization metadata.

These are options to evaluate, not selected implementations. The completed
stackless refactor and the selected Self optimization direction do not settle
this choice. A retained-stack approach would require explicit revision of the
relevant execution contracts, with Linux target support and GC integration
included in its cost. Each option must preserve ordinary native IdApi entry and
leave compiler and scheduling policy open to implementation in Birnam.

## 6. Deoptimization in Birnam

### 6.1 Three different levels

| Level | Behavior | State recovery needed |
| --- | --- | --- |
| Guarded operation fallback | Before one operation, check its assumptions; if they fail, execute that operation through the ordinary path | Already-evaluated operands and the current valid Frame state |
| Same-frame deoptimization | Leave an optimized region and resume baseline execution of the same active implementation | Exact continuation point, locals, roots, pending state, and code ownership |
| Inlined/virtualized execution recovery | Recreate baseline Frames and objects represented implicitly by optimized code | Inline call chain, identities and aliases, captures/cells, return destinations, root maps, and generation pins |

The first level is the experiment candidate. It is not a claim to have a general
deoptimizer. The latter levels are potential follow-ups only if simpler changes
leave a measured need.

Explicit Frames, logical values, descriptors, source maps, and portable CodeObjects
give Birnam useful recovery ingredients. They do not already provide maps from
arbitrary optimized registers, eliminated objects, or inlined calls back to a
baseline execution. Those maps and a checked transition would be new work.

### 6.2 Guards must cover semantics, not just method addresses

A guard for a canonical operation must establish the relevant Runtime, receiver
and vtable identities, inherited lookup dependencies, current lookup protocol,
operation implementation, and descriptor/lifecycle validity. Checking only the
resulting method pointer misses a custom lookup that returns the same method
while logging, mutating, or failing.

Keep guard reads private, non-allocating, and non-effectful. Unknown or overridden
lookup takes the ordinary path; never execute an effectful lookup speculatively
and repeat it on a miss. Receiver and argument evaluation completes in its normal
order before dispatch assumptions are consumed. Revalidate after a callback,
allocation/finalizer boundary, or any other operation that can invalidate them.

The initial owner-thread design can investigate a coarse monotonic mutation epoch
plus exact eligibility checks. A complete mutation audit must cover replacement,
removal, inherited changes, custom lookup changes, rollback, recovery, and any
supported raw mutation path. Untracked mutation makes the optimization ineligible.
Conservative invalidation is acceptable; missing invalidation is not. Epoch wrap,
identity reuse, and repeated invalidation must fail safely and remain bounded.

A dependency epoch is not code-lifetime protection. Retained methods and
descriptors still need the Runtime's protected resolution and appropriate leases.
Shared-Runtime readers would additionally require the concurrent publication and
retirement protocol deferred by Design 60D; owner-thread guards do not supply it.

### 6.3 Recovery must not replay effects

Fallback resumes the next operation that has not yet occurred. It must not
restart a method, re-evaluate an argument, rerun custom lookup, or repeat an I/O
effect. Deoptimization reconstructs the logical execution already in progress.
It is not transaction rollback.

For example, after an argument redefines the target method, the following send
must see that replacement. If the caller was optimized, recover the caller's
existing implementation and current arguments, then perform the new lookup.
Do not restart the caller or replace its remaining body with a newer definition.

Before any recoverable boundary, define a checked mapping for the logical PC,
live Oops, raw values, captures, cells, caller result destination, delimiter state,
and implementation/generation identity. Design 60B/60F's pending-transition and
root-state rules still apply: a half-published action is not a resumable checkpoint.
Validate recovery metadata before publishing recovered state, retain old roots
and pins until transfer completes, and handle failure without partial mutation.

Recovery itself uses checked Runtime machinery. Reconstructing a Frame must not
invoke a newly overridden Machine setter and thereby introduce user effects that
the baseline execution never performed. Once recovery completes, subsequent
semantic operations again use the current ordinary protocol.

Unboxing is especially constrained. If a box can escape to user code, custom
lookup, reflection, an image, or a closure, preserve its identity and aliases.
Materializing a numerically equal replacement later may be wrong. Reflective
enumeration and allocation/finalizer observations must be audited too. If the
required behavior cannot be reconstructed, do not eliminate that allocation.

### 6.4 Suspension, inspection, and artifacts

The first experiment retains existing StepMethod boundaries and canonical Frame
storage. Later region optimizations need bounded safepoints with complete roots
and recovery metadata. A native FFI activation still cannot be deoptimized or
suspended merely because its caller has recovery metadata.

Current FrameView inspection must remain valid under its existing rules. Full
debugger-driven editing of optimized locals or arbitrary frame replacement would
be additional functionality, not something this proposal claims Birnam already
supports.

Optimization dependencies and native recovery addresses are disposable state.
Portable executable meaning remains authoritative for rematerialization. A
production tier needs explicit artifact selection, exact admission, source/AOT
and image parity, rollback, and bounded retirement. It may require versioned
private metadata, reviewed separately; do not silently reinterpret an existing
descriptor, image, or frozen IdApi field.

## 7. Bounded execution experiment

### 7.1 Question and scope

**Can guarded execution of generated Machine slot access reduce the cost of a
representative source-authored loop while preserving the current semantic and
lifetime contracts?**

Use one workload family: production resumable `to:do:` control, with an empty
body and a companion body that sends `step:` to a receiver and accumulates a
checked result. Keep the same Birnam source for baseline and candidate.

The candidate specializes only the `oopAt:`/`oopAt:put:` frame-access family.
Keep ordinary application sends, arithmetic, lookup on ineligible paths,
StepAction publication, and existing control boundaries. Preserve operand box
creation and rooting initially. This isolates dispatch/control savings from
the harder question of allocation elision.

Do not introduce a general JIT, arbitrary inlining, new scheduler, work stealing,
threaded Runtimes, different GC, numeric canonicalization, native sorting, or a
new image format. Use a development-only generated artifact and explicit
selection; ordinary builds retain the baseline path. No experimental artifact
may masquerade as an existing admitted production artifact or be published or
persisted through an unsupported format.

### 7.2 E0 — establish whether the optimization is legal

Before changing execution, record:

1. The exact canonical slot-operation implementations and complete lookup path.
2. Every mutation or observation that can invalidate equivalence, including
   metacircular lookup, private vtables, GC/finalizers, and root-provider behavior.
3. The guard's dependency and lifetime rules, and where guards must be repeated.
4. The exact fallback boundary and the existing operands/state retained there.
5. A code-generation sketch showing how checked native access can be reached
   without a persistent raw Frame pointer or a borrow across a send.

Direct execution must perform the same slot, owner, lifecycle, pending-state,
and root-state checks that are semantically required. Validation may be moved
only when the descriptor and lifetime proof establishes equivalent behavior.
Do not obtain a faster number by removing malformed-state rejection.

If the proof requires silently narrowing a current contract, stop this candidate
and report the conflict. Writing this design does not authorize that narrowing.
An already-reviewed proof is required before an experimental bypass is enabled.
The E0 proof is [Design 74A](74a-e0-guarded-slot-access-legality.md)
(2026-09-09): legal under a four-condition guard plus a dictionary-mutation
epoch, with two documentation/code discrepancies reported for separate review.

### 7.3 E1 — baseline and one guarded path

Retain [the existing control probe](../../bench/control-throughput-probe.birnam)
unchanged as diagnostic context. Its disabled automatic GC is intentional and
must not become the only measurement of application memory or performance.

Add a separate resumable fixture with runtime-supplied iteration count and an
observable checksum. Its companion body performs ordinary `step:` sends and
updates a captured accumulator; all callback entries must be selected resumably.
Characterize empty input and the loop's receiver return value as well as the
checksum. Initial sizes are 0, 1, 32, 1,024, and 16,384 iterations; retain the
existing 100,000-iteration probe separately.

The guard executes before the selected slot operation. On failure, use the same
evaluated operands to perform the ordinary operation exactly once. On success,
use a short checked access to canonical Frame storage. Counters distinguish
eligible operations, guard hits/misses, fallback sends, allocations, and driver
transitions. Collect counters separately from uninstrumented timing runs.

Use the existing fully resumable host-drive test seam for suspension cases.
Entering only through a synchronous native thunk and then asking its callback
to suspend would test the entry barrier, not successful task suspension.

### 7.4 E2 — adversarial semantic and lifetime matrix

Compare candidate and baseline for these cases before accepting speed evidence:

| Case | Required evidence |
| --- | --- |
| Ordinary execution | Same loop answer, checksum, ordinary send effects, and error behavior |
| Replace `step:` during execution, including argument evaluation | Each subsequent semantic send observes the expected current definition; no duplicated effects |
| Override Machine slot access before and during execution | Eligibility fails before the affected operation and its ordinary override runs |
| Custom lookup returns the same target but logs/mutates | Lookup effects remain present and occur exactly once; target equality does not enable bypass |
| Remove/restore a binding, change inheritance, activate/roll back code | Correct eligibility and invalidation after every change; no stale target or identity reuse |
| Force GC and nested callback entry | Live boxes, operands, Frame slots, captures, and code remain reachable; no borrow crosses reentry |
| Suspend and resume on a fully resumable path | Resume from the same logical state on the owner thread, preserving roots and generation pins |
| Deliberate native barrier and foreign-owner attempt | Preserve existing rejection without corrupting the outer execution |
| Invalid slot/action/lifecycle input | Same required failure boundary; no partial mutation or reduced validation |
| Repeated invalidation, retirement, and abandoned execution | Registrations, dependencies, roots, and code ownership remain bounded by live state |

Record which events are semantic and which counters are diagnostic. Machine
dispatch counters may differ on a proved canonical fast path; observable custom
lookup or override effects may not. Passing final-result checks alone is not
equivalence evidence.

The prototype does not qualify a production artifact tier. Production promotion
would additionally require the existing full verification, language, source/AOT,
image recovery, cold-bootstrap, relocation, and code-retirement gates for its
reviewed representation.

### 7.5 E3 — measure execution cost and Objective-C distance

Build independent baseline and candidate prefixes from fixed source states with
the same Rust/native backend, optimization profile, inputs, and GC settings.
Record revisions, changes, artifact hashes, compiler versions/flags, host/target,
and invocation commands. Alternate baseline/candidate order over at least five
fresh-process pairs, report all samples and medians, and separate startup,
compilation, and warm execution. Use runtime inputs and consume results; inspect
generated code so an eliminated loop cannot be mistaken for faster dispatch.

Run the default-GC fixture, explicit-GC correctness variants, and the retained
no-auto-GC diagnostic separately. Record allocations/bytes, collections, peak
memory, native-stack depth, frame occupancy, guard/fallback counts, and repeated
invalidation cost. Report instrumentation overhead independently.

Build two Objective-C references on the same machine:

- an object/message version with per-iteration method sends, matching work and
  results as closely as possible, with representation differences stated; and
- an optimized primitive-integer loop as a lower-overhead reference.

Record Clang optimization and ARC settings, runtime version, and actual generated
dispatch. Do not manually retain an IMP to bypass dispatch and call that the
ordinary messaging reference. Objective-C need not emulate Birnam's arbitrary
Machine/custom-lookup semantics: baseline Birnam owns that correctness oracle.
Apple's runtime is a comparison implementation, not a dependency for Birnam or
its Linux port.

**Proposed continuation criterion:** all E2 cases pass, and the guarded candidate
reduces median warm time by at least 20% on both the empty and message-body
1,024/16,384-iteration cases, with the improvement exceeding measured run-to-run
variation. Small, ineligible, and mutation-heavy cases must be reported; an
unexplained regression above the existing applicable benchmark tolerance blocks
promotion. Freeze this criterion before measurement if the experiment is
accepted. It is an experiment selection threshold, not an Objective-C parity
promise or a replacement for release gates.

**Amended 2026-09-09 by operator decision, after measurement.** The 20%
figure was frozen in [74C §2](74c-e2-matrix-and-e3-measurement.md) and
measured at −17.1% / −20.3% (1,024 iterations, empty / `step:` body) and
−13.2% / −13.7% (16,384), every improvement exceeding its run-to-run spread
and no reported case regressing. The operator withdrew the percentage
threshold entirely: a change whose equivalence is proven (E0) and exercised
(E2), that regresses nothing, and whose improvement exceeds run-to-run
variation, is accepted on that improvement, whatever its size. Percentage
thresholds remain appropriate for choosing between competing hypotheses and
for release regression gates (Design 46); they are not a bar a zero-downside
change has to clear. Under the amended criterion E3 is met and §7.6's winning
branch applies. The operator sequenced the box-elision hypothesis
([74D](74d-e4-slot-index-box-elision.md)) ahead of the production slot-access
slice proposal, so that proposal is deferred, not waived. The frozen 74C record
keeps its original verdict.

### 7.6 Stop and review

Deliver the E0 proof, source/command receipt, semantic matrix, raw measurements,
Objective-C comparison, and a short complexity/cost assessment. Distinguish
operation fallback demonstrated by E1/E2 from the unimplemented general
deoptimization levels in section 6.

If the candidate wins, propose one production slot-access slice with its artifact
and bootstrap obligations. If guards erase the gain, or allocation remains the
dominant cost, report that outcome and review the next hypothesis separately.
Do not turn a negative result into automatic authorization for unboxing,
inlining, weakened reflection, or a new compiler backend.

## 8. Concurrency and Linux work after the experiment

Single-core results inform execution design; they do not gate every independent
concurrency investigation. A separate multicore proof can extend isolates with
asynchronous submission/completion, bounded queues, one request per worker, and
end-to-end deadlines. Use independent CPU jobs and 1/2/4 persistent workers;
prove simultaneous useful computation and report throughput and aggregate memory.
Sleeping workers or serial request/reply calls are not CPU-parallelism evidence.

Reactor integration then needs a ready queue, root ownership for queued Machines,
readiness/timer adapters, cancellation, and a bounded drive contract. Foreign I/O
threads may deliver owned bytes or readiness notifications; only the owner enters
Birnam. Review long native operations individually. Sort's synchronous boundary
remains a limitation even if its String comparator is resumable. Sections 5.4
and 5.5 describe its consequences and the native integration choices; native
callback suspension and its current fatal failure surface need explicit review
before promising that arbitrary library callbacks can yield.

Thread-hosted Runtimes require their own API/module ownership work. Shared heaps
require the much larger synchronization and memory-model design in section 4.
Neither is a hidden extension of a worker-pool ticket.

Linux work must identify its CPU/libc targets and prove native ABI/root discovery,
loader/libtcc behavior, artifact materialization, foreign layouts, and OS services.
The current [native-root adapter](../../rust/birnam-id/src/gc.rs) is specifically
implemented for Darwin arm64. Explicit Frames do not eliminate roots held by
native calls. Optimizer guards and recovery maps must be target-qualified and
must not embed Darwin assumptions into portable executable meaning.

## 9. Review decisions and relationship to existing work

| Decision | Disposition |
| --- | --- |
| Preserve existing malleability and cold bootstrap | Requirement supplied by the user; no silent reduction |
| Self optimization direction | Selected by the user on 2026-09-06; individual mechanisms still require design and proof |
| Arbitrary native methods through IdApi | Core requirement reaffirmed by the user; optimizer metadata is optional for ordinary native methods |
| Long native work and callbacks that need to yield | Open: evaluate resumable implementations, asynchronous adapters, and retained native stacks |
| Fatal failure when suspension escapes through synchronous IdApi.send | Current behavior; review the failure contract separately from the need to preserve native activations |
| Parallel owners with local synchronous sends | Preferred candidate, subject to workload and cross-owner live-change semantics |
| Cross-owner activation scope | Open: independent versus coordinated publication |
| Guarded execution before restricting Machine reflection | Recommended investigation |
| E0 legality proof for guarded slot access | Delivered as [Design 74A](74a-e0-guarded-slot-access-legality.md); reviewed and accepted by the user on 2026-09-09 |
| E1 baseline and one guarded path | Authorized and delivered 2026-09-09 as [Design 74B](74b-e1-guarded-slot-access-implementation.md): feature-gated dev prefix, explicit emission selection, fixture and driver; first diagnostic −15% to −23% with every guard hit |
| E2–E3 and the 20% continuation threshold | Delivered 2026-09-09 as [Design 74C](74c-e2-matrix-and-e3-measurement.md): E2 identical in every row; E3 criterion frozen then measured and **not met at 20%** (13–20% real gain, allocation dominant). Same day the operator **withdrew the percentage threshold** (§7.5 amendment: proven-equivalent, non-regressing, above-spread improvements are accepted at any size): E3 is met as amended, the production slice proposal is deferred behind the box-elision hypothesis ([74D](74d-e4-slot-index-box-elision.md)), which is written up and under adversarial review, not yet authorized |
| General deoptimization, inlining, or unboxing | Future possibilities requiring separate proof and implementation review |
| Objective-C execution comparison | Comparative evidence, not promised parity or a runtime dependency |

This inquiry pauses implementation selection from the earlier performance
conversation. [Design 73](73-scalable-ordering-contract.md) remains proposed.
Existing Beads dependencies and acceptance states are unchanged. The
[performance execution map](../misc/SELF-HOSTING-PERFORMANCE.md) remains the
record of that epic; its allocation/control and Linux design gates overlap with
questions raised here but are not implicitly closed or replaced.

Writing this document does not begin the experiment. Review should first settle
the experiment's scope and the meaning of malleability across execution owners.
The result can then justify a bounded implementation instead of another broad
runtime refactor selected on architectural intuition alone.
