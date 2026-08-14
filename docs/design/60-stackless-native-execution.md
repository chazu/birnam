# 60 — Stackless Native Execution

> **Status:** Architecture overview and delivery index; S0 accepted 2026-08-04,
> with the recognition, late-bound-microcode, same-execution-entry-scope,
> binding-lifetime/resolve-and-pin, and live-dispatch-timing amendments accepted
> 2026-08-05; the Runtime-ownership and acceptance-staging amendments were
> accepted 2026-08-06, together with the code-disposition staging and S1-slicing
> amendments and the frozen S1 Machine-microcode protocol. S1.1-S1.5 were
> implemented and accepted against the repository gates on 2026-08-06; source
> compiler lowering remains S2. The lowering, liveness, root-map, slot-coloring,
> and edge-transfer portions of 60F were accepted 2026-08-07; 60F's physical
> carrier and Design 49 migration-event decisions, together with the review's
> fresh-Oop, retained-closure, and exact-eligibility amendments, were accepted
> 2026-08-14. The durable `ResumableManifest`, image-v5 block-vtable recovery,
> and no-IdApi-growth boundary were accepted 2026-08-14. The canonical
> `ExecutionSelection` carrier, relational materialization boundary, and
> live/AOT generation-provenance rule were accepted 2026-08-14. Design 60F's
> S2 pre-implementation gate is closed.
> This record describes the Rust-only path from synchronous native execution to
> explicit language frames. It is not itself an implementation commitment.
>
> The detailed S0 contracts live in:
>
> - [60a — Execution semantics](60a-stackless-execution-semantics.md)
> - [60b — Machine, frames, and GC](60b-stackless-machine-and-frames.md)
> - [60c — StepMethod calling contract](60c-stepmethod-calling-abi-options.md)
> - [60d — Concurrency fit and boundaries](60d-stackless-concurrency-fit.md)
> - [60e — Resumable registration and the entry seam](60e-resumable-registration-and-entry.md)
> - [60f — Resumable compiler lowering](60f-resumable-compiler-lowering.md)
>
> Future records will cover code-generation lifetime, host integration, and the
> acceptance matrix.

## 1. Scope

Birnam currently executes authored methods through the synchronous Rust Method
ABI. A send resolves a method address and invokes it to completion. Nested
Birnam sends therefore consume the host thread's native stack.

Stackless native execution changes the execution representation, not Birnam's
object model. The first target is:

- a first-class Birnam Machine Oop backed initially by private Rust state;
- an explicit private Frame stack owned by that Machine state;
- metacircularly visible StepAction Oops returned by resumable steps;
- native-compiled resumable method steps;
- a trampoline for call, return, tail-call, and later suspension;
- synchronous Rust methods retained as an adapter path;
- explicit barriers at unmigrated native and foreign boundaries;
- process-local, initially one-shot continuations that are not image state.

The design preserves Oop representation, vtables, Store semantics, relational
policy, the current image format, and the existing synchronous Method ABI.

## 2. Current Rust baseline

The current implementation is deliberately synchronous:

- birnam-abi defines Method as an unsafe C function taking receiver, argument
  buffer, and argument count, and IdApi exposes method installation and send.
- birnam-id performs lookup and immediately calls the resolved Method address.
- birnam-codegen emits nested birnam_send calls and synchronous compound-literal
  argument arrays.
- birnam-prelude keeps callback-driving control state in Rust activations,
  including Boolean loops and integer iteration.
- birnam-id collects through a hybrid native-stack scan plus registered root
  providers and vtable trace hooks. It has no MachineState trace/finalization
  integration yet.
- birnam-host and birnam-codegen-runtime retain process-lifetime runtime and
  generation authorities, but do not yet pin code specifically for suspended
  language frames.
- birnam-isolate Frame values are transport records, not execution frames.

The Rust floor is now the only production implementation. Historical benchmark
records may mention the former implementation, but no stackless contract may
depend on it.

## 3. Target invariants

The implementation must preserve these invariants:

1. Existing synchronous native modules continue to load and call IdApi.send.
2. A synchronous leaf may remain a direct Method call when it cannot suspend.
3. A migrated language call does not grow the native stack with Birnam depth.
4. Every live Oop held across a step boundary is in a machine-owned traced slot.
5. A suspended frame keeps its code generation and frame layout alive.
6. Native and foreign frames are never copied into a continuation.
7. Continuations are process-local until a separate durability design exists.
8. Transport frames and execution frames remain different types and protocols.
9. Old compiled units are rejected or executed through an explicit compatibility
   path; they are never silently interpreted as resumable units.
10. All new ABI surface is append-only and versioned.
11. Machine and StepAction cross the compiler/runtime boundary as Oops; Frame
    storage remains private and is exposed only through checked accessors and
    read-only FrameView Oops.
12. Birnam owns execution-object identity and protocol. Rust temporarily owns
    the low-level storage and primitive operations behind that protocol.
13. One Birnam Runtime is the sole semantic authority for its object universe
    and in-flight execution. Codegen and loaders may hold physical code
    resources, but cannot register, retire, or dispose them behind that
    Runtime's execution authority.

### 3.1 Runtime ownership

The production `birnam-id::Runtime` is the ownership island for one Birnam
object universe. Its private execution state owns Machine capabilities, Frame
arenas, ResumableEntry recognition, protected dispatch resolution, binding
retirement, entry scopes, and Runtime-local generation pins. This is mechanism
at the same floor as send, allocation, and tracing; it does not move scheduler,
compiler, or language policy into the floor.

The host or reactor is the **executor** of a Runtime, not its owner of semantic
state. The first implementation gives one Runtime one owner thread and
interleaves many Machines cooperatively. Parallel computation uses isolated
Runtimes with copied values, initially in separate processes. A later in-process
isolate implementation may give several Runtimes separate heaps, Stores,
dispatch, and execution registries; it may not reinterpret that model as several
threads sharing one mutable Runtime.

Codegen/TCC and artifact loaders have **physical custody** of mapped code. A
process code repository may share immutable mappings across isolated Runtimes,
but each Runtime registers its own ResumableEntries and holds its own generation
leases. Disposal is an effect performed by the code repository only after every
Runtime has released the mapping. The Runtime owns the safety verdict; the
loader owns `dlclose` or its equivalent.

Both the cold compiler and a future self-hosted compiler emit the same versioned
registration operation. Its host shim registers against the currently entered
Runtime and is invalid with no active Runtime. This is artifact ABI, not an
IdApi extension. The compiler describes executable entries; it never owns
Machine, dispatch, or entry-registry state.

## 4. Explicit non-goals for the first delivery

The first delivery does not include:

- multi-shot continuation restoration;
- durable continuations in images or Store facts;
- a general scheduler or reactor redesign;
- debugger-grade frame mutation;
- migration of every native primitive;
- removal or replacement of the synchronous Method ABI;
- an interpreter becoming the production execution engine.

These may use the same machinery later, but they must not expand the first
proof implicitly.

## 5. Document map

### 60A — Execution semantics

Defines the observable contract before implementation:

- machine ownership and reentrancy;
- call, return, tail-call, suspend, capture, restore, and abort;
- one-shot continuation behavior;
- native and FFI barriers;
- error values and driver failures;
- dispatch-generation behavior after restoration.

This record is the semantic gate for S1 and S2.

### 60B — Machine, frames, and GC

Defines the Birnam-object/private-Rust-storage boundary:

- Machine Oop and native-backed MachineState lifecycle;
- frame slots, descriptors, and arenas;
- metacircularly visible StepAction Oops using slot references rather than raw
  Oop pointers;
- checked registries, trace/finalization hooks, and forced collection;
- read-only FrameView Oops and stale-view behavior;
- continuation and frame teardown.

This record is the memory-safety gate for S1 and S3.

### 60C — StepMethod calling contract

Accepts reuse of the existing Method ABI with Machine as receiver, a
descriptor-declared argument buffer, and StepAction as result. It defines the
floor/malleability split for
the primitive-backed Machine protocol and defers stronger native interfaces
until measurements justify them. Its S1 probe validates that accepted minimal
calling contract and returns to design review if a required semantic cannot be
expressed.

Resumable registration and the ordinary-send entry seam are decided in 60E;
compiler lowering is the subject of 60F below.

### 60D — Concurrency fit and boundaries

Defines how owner-thread Machines compose with a cooperative reactor and
process isolates. It makes StepAction boundaries the scheduling safe points,
requires short MachineState borrows across generated code and callbacks, and
records the limits on work stealing and shared-runtime parallelism.

This record is the concurrency-compatibility gate for S1 and later scheduler
work. It does not select a scheduler or reopen the reactor/isolate decisions in
Design 48.

### 60E — Resumable registration and the entry seam

Decides what a vtable holds for a resumable implementation, how an ordinary send
reaches one through an entry thunk, how the driver recognizes a resumable
implementation without aliasing recycled code, and how an unmigrated caller
crosses into resumable code without allocating a Machine per crossing. It makes
registration an emitted artifact operation so image boot re-registers rather
than restoring installs without them, keeps the Machine microcode ordinarily
late-bound, amends the 60C driver argument count, states what migration does and
does not preserve, and sets the provisional performance budget 60C leaves open.

This record is the correctness gate for S1: without it, an installed StepMethod
is directly callable by an ordinary send.

### 60F — Resumable compiler lowering

[60F](60f-resumable-compiler-lowering.md) defines explicit per-implementation
migration, receiver-first/argument-left-to-right evaluation followed by fresh
lookup, one Machine boundary per source send, deterministic CFG and PC
assignment, Oop liveness, descriptor-v2 per-PC root maps, interference-based
slot coloring, per-edge transfer plans, a contiguous reusable call-staging
window, logical-value source maps, and live/AOT parity.

Its lowering and layout semantics were accepted 2026-08-07. The review
amendments, canonical pointer-free descriptor blob plus native registration
envelope, and carrier-only Design 49 successor-generation projection were
accepted 2026-08-14. The same review cycle then accepted one canonical
pointer-free `ResumableManifest`, its required image-v5 projection, an
idempotent lazy block-vtable resolver and Runtime image catalog, exact
post-initializer/final-binding checks, and the boundary that none of these grow
IdApi or change lookup. It then freezes a revision-qualified canonical
`ExecutionSelection`: relational policy is materialized before compilation,
artifacts and DispatchGenerations retain the accepted total table, and image/AOT
compilation replays that provenance rather than choosing again. 60F has no
remaining pre-implementation decision; its evidence gates still govern
acceptance of the implementation.

### Future 60G — Lifetime and host integration

Will complete TCC/AOT lifetime and live replacement above S1's minimal
registration/Frame pins: binding retirement, continuation pin transfer, actual
reclamation of method-bearing mappings, image rejection, and recovery behavior.
Before concurrent evolution it must also supply 60D §8.1's read-side epochs,
process-shared mapping lifetime, retirement diagnostics, and contention
evidence. Continuous self-hosted compilation is not production-ready until this
record either closes reclamation or establishes an accepted bound on retained
code.

### Future 60H — Acceptance and rollout

Will define the compatibility matrix, release gates, and rollout policy. The S1
implementation slices and their local exit evidence are part of this record;
they cannot be deferred until rollout planning.

## 6. Delivery sequence

### S0 — Documentation and contract freeze

**Accepted 2026-08-04.** 60A, 60B, 60C, 60D, and 60E are the accepted contract
set. The Method-shaped StepMethod contract includes the three amendments in 60E
§9. The frozen IdApi v1 fixtures remain the compatibility floor, and
`sends.compiled-unary` is the synchronous-send constant `S` against which 60E
§11 states the provisional transition budgets.

Acceptance authorizes S1 implementation of the parallel Machine substrate. It
does not claim the S1 proof already passes: the hand-written StepMethod probe is
the first validation of the accepted calling contract, and a concrete semantic
failure returns to design review rather than silently changing the contract.

Acceptance lists are cross-stage contracts. S0 acceptance freezes the complete
list; an implementation stage gates only on items labelled for that stage, and
later stages add their labelled evidence cumulatively. A compound obligation is
split when its Machine/Frame portion first runs in S1 and its suspension or
Continuation portion first runs in S3.

Acceptance evidence on 2026-08-04:

- the 29-case micro suite passed against an isolated ReleaseSafe Rust prefix;
  `sends.compiled-unary` measured 377 ns/op in quick harness mode;
- the frozen IdApi v1 native client, native module, mapped image, and rebuilt
  image all passed against that prefix; and
- the acceptance is a contract decision, not runtime-completion evidence. The
  million-transition, GC, registration, nested-entry, and suspension proofs
  remain owned by S1–S3 exactly where the companion records label them.

### S1 — Parallel machine substrate

Add the Machine Oop/native MachineState capability boundary, private Frame
storage, canonical StepAction Oops, an action loop, root tracing/finalization,
and a sync adapter. Add the owner-thread, non-allocating resolve-and-pin shape,
direct-binding reference accounting, retirement state, and the minimal
registration/Frame generation-pin transfer. Add the conservative physical-
disposition gate and retained-generation count/byte telemetry; do not add per-
resolution locks, atomic read-modify-write operations, or reference-count
traffic. Do not change generated code yet. An internal test StepMethod must be
able to run to completion and produce the same result as send, and Birnam
reflection must see the execution-object protocols.

Questions 2 and 3 in 60C §15 are resolved by the frozen Machine-microcode
protocol in 60B §1 and 60C §5. S1 is implemented as the following dependency-
ordered vertical slices.

#### S1.1 — Leaf Machine

Add the audited Runtime execution module, Machine capability and lifecycle,
private Frame arena and descriptor validation, canonical ContinueAction and
ReturnAction objects, checked action decoding, and Machine tracing and
finalization. Freeze the initial arena policy: initialize every live Oop slot
before collection, trace no retained free capacity, retain reusable chunks
until Machine destruction, and publish live-frame high-water and reserved-byte
diagnostics. A hand-written StepMethod must execute through the existing Method
ABI and answer the expected Oop; forced collection must preserve its live Frame
Oops and later finalize an unreachable Machine exactly once.

#### S1.2 — Stackless calls

Add CallAction, TailCallAction, the synchronous-Method adapter, and same-
execution entry barriers. Two hand-written StepMethods must mutually recurse for
at least one million calls with bounded native-stack movement, TailCall must not
grow Frame depth, synchronous re-entry must preserve the waiting Frame, and
owner or exclusive-drive violations must fail before touching Frame storage.
Publish the current and high-water synchronous-boundary depth for each drive,
without imposing an artificial depth limit or adding locks, atomics, or
allocation to the owner-thread path.

#### S1.3 — Registered ordinary send

Add Runtime-local ResumableEntry recognition, the versioned registration shim,
the semantic v1 registration carrier, the unique entry thunk, and resolve-and-
pin classification. The carrier freezes its required fields, version/size
validation, lifetime transfer, and failure-before-publication rules while
leaving descriptor blob encoding to 60F §16. An ordinary send from
unmigrated code must enter the Machine, execute the registered hand-written
StepMethod, and answer the same result as its synchronous predecessor. This
slice changes no source compiler lowering and exposes registration only to its
test artifact while physical unloading remains disabled.

#### S1.4 — Mutation and lifetime

Add direct-binding references, retirement state, registration and Frame pins,
deterministic read-side protection tests, active-Frame redefinition behavior,
independent Runtime registrations over a shared immutable mapping, conservative
physical disposition, the mandatory logical-discard receipt, and retained-
generation count/byte telemetry. The receipt identifies the generation and
records both successful logical discard and `Disposed | Retained(reason)`; this
slice must land before registration or disposal is exposed to ordinary
artifacts. Alias, inheritance, custom-lookup, failed-push, logical-discard, and
multi-Runtime lease tests must exercise the integrated path.

#### S1.5 — Malleability and acceptance

Complete the canonical Machine, StepAction, and FrameView reflective protocols;
checked primitive replacement; malformed, stale, wrong-Machine, and wrong-slot-
kind rejection; process-local image rejection; and every S1-labelled acceptance
item in 60A, 60B, 60D, and 60E. Publish the 60C/60E measurements and enforce the
zero-allocation/lock/atomic/refcount structural fast-path checks.

Every slice preserves the synchronous path and frozen IdApi, keeps unsafe
physical unloading disabled, passes the repository verification gate, and adds
executable integrated evidence rather than only dormant types. Source compiler
lowering remains S2. A slice may use test-only hand-written StepMethods, but it
may not expose a partially safe registration or disposal path to ordinary
artifacts.

**S1 accepted 2026-08-06.** The integrated evidence includes one-million
Continue and mutual-TailCall runs, synchronous and registered-entry adapters,
forced collection, nested and suppressed entry scopes, late-bound microcode
mutation, independent Runtime registration, direct-binding retirement and pin
transfer, FrameView staleness, kinded fatal boundary failures, process-local
image rejection, and logical/physical discard receipts. The release-mode S1
probe measured Continue at `0.155 x S` and synchronous resolve/classify plus
leaf invocation at `0.795 x S`, with one microcode send per hand-written step;
60E records the same-run raw measurements. The 456-case Birnam suite and the
workspace/native-prefix/cold-archive/relocation verification gates passed.
This acceptance does not claim S2 compiler lowering, S3 Continuations, or the
actual reclamation owned by Future 60G.

### S2 — Minimal resumable compiler path

Under [60F](60f-resumable-compiler-lowering.md), explicitly select individual
method and block-body implementations and lower the accepted subset—literals,
locals, blocks, and ordinary sends—to deterministic basic blocks. Emit colored
Oop slots, explicit PCs, per-PC root maps, per-edge transfers, source locations,
and registered descriptor-v2 artifacts without changing lookup semantics or the
IdApi floor. Prove deep mutual recursion with bounded native-stack movement,
forced-GC safety at generated boundaries, live/AOT registration parity, and
mapped/rebuilt image parity.

### S3 — Suspension and one-shot restoration

Add the explicitly defined host/test suspension operation, continuation
lifecycle, forced-GC coverage, continuation pin transfer, extension of the S1
disposition gate to Continuation holders, and image rejection.

### S4 — Callback-driving control primitives

Migrate Boolean branches, while loops, integer iteration, and the collection
callbacks required by real programs. Keep atomic leaves synchronous unless
measurement justifies otherwise.

### Later — Scheduling, inspection, multi-shot, durability

Treat scheduling, cancellation, debugger inspection, multi-shot restoration, and
durable continuation state as separately reviewed policy layers.

## 7. First implementation gate

S0 acceptance records the following implementation gate as complete:

- the exact StepAction Oop layouts, reflective protocol, and ownership rules;
- the exact StepMethod calling contract accepted in 60C;
- the resumable registration record, the entry-thunk contract, the driver
  resolve-and-pin, binding-retirement, and address-reuse ordering rules, and the
  artifact/image registration surface from 60E;
- the Machine Oop capability and private-Frame accessor boundary for generated
  native code;
- registry generation, trace, and finalization rules for native-backed state;
- machine/fiber owner-thread and reentrancy rules;
- short-borrow and exclusive-drive rules from 60D;
- capture legality and native/FFI barrier behavior;
- frame slot, trace-hook, and scoped-root invariants;
- code-generation pinning and restored-generation dispatch;
- the minimal non-public suspend/resume test contract.

Two proofs follow, and they are not the same milestone.

The S1/S2 execution proof is successful only when it demonstrates:

1. two mutually recursive resumable methods;
2. at least one million calls without unbounded native-stack growth;
3. a synchronous leaf invoked through the adapter;
4. a resumable method reached by an ordinary send from unmigrated code, with no
   observable change to that send;
5. explicit frame roots surviving forced collection;
6. deterministic rejection across a native barrier;
7. measured overhead against the current Rust synchronous path, published
   against the 60E §11 budget whether or not it passes;
8. directly observed registration and Frame pins, plus conservative physical
   disposition and retained-generation count/byte telemetry.

The S3 suspension proof adds:

9. suspension and resumption under the S0 contract;
10. one-shot restoration rejecting its second restore;
11. directly observed Continuation pin transfer while the S1 disposition gate
    continues to retain the mapping.

Future 60G owns proof that a formerly method-bearing mapping can actually be
reclaimed without leaving a Runtime registry or reachable vtable naming its
address range.
