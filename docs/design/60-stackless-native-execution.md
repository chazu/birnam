# 60 — Stackless Native Execution

> **Status:** Architecture overview and delivery index, updated 2026-07-29.
> This record describes the Rust-only path from synchronous native execution to
> explicit language frames. It is not itself an implementation commitment.
>
> The detailed S0 contracts live in:
>
> - [60a — Execution semantics](60a-stackless-execution-semantics.md)
> - [60b — Machine, frames, and GC](60b-stackless-machine-and-frames.md)
>
> Future records will cover the compiler/ABI seam, code-generation lifetime,
> host integration, and the acceptance matrix.

## 1. Scope

Birnam currently executes authored methods through the synchronous Rust Method
ABI. A send resolves a method address and invokes it to completion. Nested
Birnam sends therefore consume the host thread's native stack.

Stackless native execution changes the execution representation, not Birnam's
object model. The first target is:

- an explicit frame stack owned by a Rust Machine;
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
  providers. It has no Machine frame-root provider yet.
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

Defines the Rust-owned representation:

- machine and fiber lifecycle;
- frame slots, descriptors, and arenas;
- action representation using slot references rather than raw Oop pointers;
- root-provider registration and forced collection;
- frame inspection boundaries;
- continuation and frame teardown.

This record is the memory-safety gate for S1 and S3.

### Future 60C — ABI and compiler

Will define the append-only C ABI, resumable registration, sync adaptation, CFG
lowering, liveness, block captures, source maps, and live/AOT parity.

### Future 60D — Lifetime and host integration

Will define TCC/AOT code pinning, live replacement, host drive/suspend/resume,
image rejection, and recovery behavior.

### Future 60E — Acceptance and rollout

Will define the implementation slices, compatibility matrix, stack bounds,
performance budgets, and release gates.

## 6. Delivery sequence

### S0 — Documentation and contract freeze

Accept 60A and 60B. Record the exact ABI decisions that future 60C must
implement. Establish synchronous benchmarks and current ABI fixtures.

### S1 — Parallel machine substrate

Add Rust-owned Machine and frame storage, an action loop, root tracing, and a
sync adapter. Do not change generated code yet. An internal test StepMethod
must be able to run to completion and produce the same result as send.

### S2 — Minimal resumable compiler path

Lower a small subset—literals, locals, blocks, and ordinary sends—to basic
blocks with explicit slots and program counters. Prove deep mutual recursion
with bounded native-stack movement and live/AOT registration parity.

### S3 — Suspension and one-shot restoration

Add the explicitly defined host/test suspension operation, continuation
lifecycle, forced-GC coverage, code-generation pinning, and image rejection.

### S4 — Callback-driving control primitives

Migrate Boolean branches, while loops, integer iteration, and the collection
callbacks required by real programs. Keep atomic leaves synchronous unless
measurement justifies otherwise.

### Later — Scheduling, inspection, multi-shot, durability

Treat scheduling, cancellation, debugger inspection, multi-shot restoration, and
durable continuation state as separately reviewed policy layers.

## 7. First implementation gate

Implementation may begin after S0 when all of the following are written down:

- the exact StepAction representation and ownership rules;
- the opaque-handle versus accessor boundary for generated native code;
- machine/fiber owner-thread and reentrancy rules;
- capture legality and native/FFI barrier behavior;
- frame slot and root-provider invariants;
- code-generation pinning and restored-generation dispatch;
- the minimal non-public suspend/resume test contract.

The first proof is successful only when it demonstrates:

1. two mutually recursive resumable methods;
2. at least one million calls without unbounded native-stack growth;
3. a synchronous leaf invoked through the adapter;
4. explicit frame roots surviving forced collection;
5. deterministic rejection across a native barrier;
6. suspension and resumption under the S0 contract;
7. measured overhead against the current Rust synchronous path.
