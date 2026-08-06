# 60 — Stackless Native Execution

> **Status:** Architecture overview and delivery index; S0 accepted 2026-08-04,
> with the recognition, late-bound-microcode, same-execution-entry-scope,
> binding-lifetime/resolve-and-pin, and live-dispatch-timing amendments accepted
> 2026-08-05.
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
>
> Future records will cover compiler lowering, code-generation lifetime, host
> integration, and the acceptance matrix.

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
compiler lowering is the subject of Future 60F below.

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

### Future 60F — Compiler lowering

Will define CFG lowering, slot liveness, block captures, source maps, and
live/AOT parity against the calling contract and the 60E registration record.

### Future 60G — Lifetime and host integration

Will complete TCC/AOT lifetime and live replacement above S1's minimal
registration/Frame pins: binding retirement, continuation pin transfer,
disposal, image rejection, and recovery behavior. Before concurrent evolution
it must also supply 60D §8.1's read-side epochs, process-shared mapping lifetime,
retirement diagnostics, and contention evidence.

### Future 60H — Acceptance and rollout

Will define the implementation slices, compatibility matrix, stack bounds,
performance budgets, and release gates.

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
registration/Frame generation-pin transfer; do not add per-resolution locks,
atomic read-modify-write operations, or reference-count traffic. Do not change
generated code yet. An internal test StepMethod must be able to run to
completion and produce the same result as send, and Birnam reflection must see
the execution-object protocols.

### S2 — Minimal resumable compiler path

Lower a small subset—literals, locals, blocks, and ordinary sends—to basic
blocks with explicit slots and program counters. Prove deep mutual recursion
with bounded native-stack movement and live/AOT registration parity.

### S3 — Suspension and one-shot restoration

Add the explicitly defined host/test suspension operation, continuation
lifecycle, forced-GC coverage, continuation pin transfer, disposal enforcement,
and image rejection.

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
   against the 60E §11 budget whether or not it passes.

The S3 suspension proof adds:

8. suspension and resumption under the S0 contract;
9. one-shot restoration rejecting its second restore;
10. generation pins refusing code disposal while a frame or continuation holds
    them.
