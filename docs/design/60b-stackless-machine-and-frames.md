# 60B — Stackless Machine, Frames, and GC

> **Status:** S0 representation and memory-safety record, proposed for
> acceptance, revised 2026-08-01.
> This document defines the Birnam-object/private-Rust-storage boundary required
> by [60A](60a-stackless-execution-semantics.md). It does not freeze the public C
> ABI; the decision following
> [60C](60c-stepmethod-calling-abi-options.md) must preserve the boundary
> described here.

## 1. Representation boundary

Machine and StepAction are first-class Birnam Oops. Frame is private Rust arena
storage. FrameView is a process-local Birnam Oop created on demand for safe
inspection; it is not the Frame itself.

This is the metacircular boundary for the first implementation:

> Birnam owns the object identity and protocol; Rust temporarily owns the
> low-level storage and primitive operations behind it.

Concretely:

- Birnam owns the identity, vtables, methods, and reflective protocol of
  Machine, StepAction, and FrameView objects;
- Rust initially owns the outer trampoline, checked capability registry, frame
  arena, tracing integration, and native code pins;
- the object boundary is stable even when individual primitive operations move
  from Rust into Birnam;
- ordinary message dispatch is not used to implement the driver's decoding of
  a StepAction, because that would recurse through the mechanism being driven.

Generated native code receives the Machine Oop and uses a narrow helper surface
to operate on its current private Frame:

- read and write the program counter;
- read and write an Oop slot;
- read and write a raw slot;
- validate a slot index against the current descriptor;
- obtain a slot-indexed CallAction, TailCallAction, ReturnAction, or
  ContinueAction Oop;
- request the S3 suspension operation.

The calling boundary selected after
[60C](60c-stepmethod-calling-abi-options.md) may pass a short-lived invocation
token to make frame access efficient. Generated code must never dereference a
MachineState or Frame pointer directly, retain an invocation token after
returning from its StepMethod, or pass an Oop pointer as an action operand.

This resolves the important distinction between an object protocol and a
storage layout: generated code and Birnam code traffic in Oops while Rust
retains freedom to change frame allocation, validation, and tracing.

### Native-backed payload

In this record, a native-backed payload means an ordinary Birnam object whose
vtable and message protocol are Birnam-visible, but whose raw payload contains a
checked capability into Rust-owned state. It is not a raw Rust pointer.

The Machine payload contains at least a capability kind/version, registry
index, and registry generation. These are raw, non-Oop words. Rust validates
the canonical Machine vtable, payload size, kind/version, index, generation,
owner thread, and lifecycle before resolving the capability to MachineState.
Reusing a registry slot increments its generation, so an old Machine Oop fails
as stale rather than aliasing a new execution.

StepAction needs no separate registry entry or external Rust allocation: its
canonical vtable and raw object payload encode an ephemeral control token. The
canonical action vtables and their accessors remain visible to ordinary Birnam
reflection. FrameView contains a traced Machine Oop plus a raw frame-arena
identity and generation. Continuation uses the same checked-capability pattern
as Machine for its detached Rust-owned frame segment.

## 2. Machine lifecycle

A Machine Oop denotes MachineState with one owner thread, inherited from the
Runtime that created it. The first implementation is single-threaded and
owner-affine:

- Machine creation, drive, suspension, restore, and destruction all require the
  owner thread;
- Machine is not Send or Sync;
- a Machine cannot be driven recursively while its driver is already running;
- a synchronous Method may re-enter IdApi.send, but the nested send must return
  before the outer Method returns;
- a suspended Machine may be resumed only once per accepted restore operation.

The lifecycle states are:

| State | Allowed transition |
|---|---|
| New | Running or Failed |
| Running | Completed, Suspended, or Failed |
| Suspended | Running, Failed, or Destroyed |
| Completed | Destroyed |
| Failed | Destroyed |
| Destroyed | none |

The state transition is owned by the primitive Machine driver. Birnam methods
may request a StepAction, inspect state, or ask the primitive to drive/resume,
but cannot mutate MachineState directly.

## 3. Machine storage

The representation is conceptually:

    Machine Oop
      raw registry index
      raw registry generation
      raw capability kind/version
        |
        v
    Runtime machine registry
      MachineState
        owner thread token
        frame arena
        delimiter records
        suspension state
        active generation pins
        lifecycle state

The registry entry owns MachineState but does not make the Machine Oop a GC
root. Reachability flows in the other direction: a live Machine Oop traces the
Oops in its MachineState. Otherwise an unreachable Machine would keep itself
and every frame value alive forever.

The current runtime already has vtable trace hooks and per-object finalizers.
Before collectable Machine and Continuation Oops ship, S1 must install the
appropriate trace hooks and finalizers so collection invalidates each capability
and releases its registry entry. No new finalization ABI is required. Explicit
completion or close may release state early, but callers are not required to
close an unreachable object for memory safety. Finalization must obey the
existing no-allocation callback boundary and must not run Birnam code or
resurrect the object.

The frame arena uses stable chunks or segments. A Frame is not a separately
GC-allocated Birnam object and does not enter the Store. Chunk allocation and
reuse are Rust operations.

Frame handles are validated by an arena identity and generation counter. A
handle to a popped frame, released chunk, or destroyed Machine must fail before
reading memory. Raw pointers may be used inside one Rust driver step for
performance, but they cannot escape that step or cross a foreign callback.

The arena should retain capacity after ordinary returns when that is cheaper
than releasing chunks. Suspended Machines retain their active chunks. Destroyed
Machines release all chunks after their generation pins and Oop roots have been
removed from MachineState. The registry generation is advanced before the slot
can be reused.

## 4. Frame layout

Every resumable implementation has an immutable FrameDescriptor:

    FrameDescriptor
      implementation identity
      generation identity
      layout version
      program-counter range
      slot count
      Oop-slot bitmap
      raw-slot metadata
      return-slot policy
      source-map identity

The frame payload contains:

    Frame
      descriptor identity
      program counter
      receiver slot
      argument slots
      local and temporary slots
      return destination
      delimiter link where required

Program counters, slot indices, descriptor identifiers, arena handles, and
generation counters are raw values. Receiver, arguments, locals, temporaries,
selector references, and results are Oop slots when their descriptor marks them
as such.

The descriptor is immutable for the life of a frame. A new compiler layout
version creates a new descriptor and cannot reinterpret an old frame.

## 5. Slot and action rules

StepMethod returns a StepAction Oop. Its canonical vtable identifies the action
kind and its declared payload contains a common raw header:

- action schema version;
- owning Machine registry index and generation;
- issuing drive epoch.

The remaining fields refer to slots and ranges, not borrowed pointers:

| Canonical Oop | Slot operands |
|---|---|
| CallAction | receiver slot, selector slot, argument start/count, result slot |
| TailCallAction | receiver slot, selector slot, argument start/count |
| ReturnAction | value slot |
| ContinueAction | none |
| SuspendAction | suspension/result slot and delimiter identity |
| CaptureAction | delimiter identity and result slot |
| RestoreAction | continuation handle and supplied-value slot |
| AbortAction | delimiter identity and failure/reason slot where applicable |

The future ABI record defines the exact object layouts and their versioning.
Whatever layout it chooses must use bounded integer operands and must not
contain a pointer to a temporary argument array, a Frame pointer, or an
unvalidated Oop. The ABI result remains an Oop, not a Rust enum, tagged C value,
or opaque action sink.

The canonical action vtables and field protocol are metacircularly visible.
Birnam code can inspect an action's kind and operands through ordinary methods
and reflection. Those accessors and constructors may initially terminate in
Rust primitives; the outer driver alone uses trusted direct decoding so it does
not need a recursive send. Moving an accessor, constructor, or policy method
into Birnam does not change the StepMethod result type or object protocol.

A Machine may retain one or more scratch StepAction Oops and rewrite their raw
payloads before returning them from a step. Every issuance advances or records
the Machine's drive epoch. The driver validates canonical vtable, schema,
Machine capability, and epoch before interpreting variant fields. There is no
public action-submission operation: the driver accepts an action only as the
direct result of the active StepMethod. A retained alias therefore grants no
independent right to drive the Machine, and the language promises neither a
fresh StepAction identity nor snapshot payload contents for every step.

Before returning Call, generated code stores every receiver, selector, argument,
and live local in frame slots. The driver may then construct a temporary
argument buffer from those rooted slots while invoking a synchronous Method.
The buffer remains valid only for that call, preserving the existing IdApi
contract without making a C compound literal part of the continuation state.

An action is validated against its Machine and the current FrameDescriptor
before dispatch:

- its vtable and payload schema are canonical and supported;
- its Machine index and generation name the Machine being driven;
- its drive epoch is current;
- each slot index is in range;
- each Oop operand is marked as an Oop slot;
- argument ranges do not overlap the result slot unexpectedly;
- the program counter is valid for the implementation;
- the action is legal in the current Machine state.

Invalid actions return invalidStepAction, staleStepAction,
stepActionWrongMachine, or invalidFrameSlot without dereferencing generated
memory.

## 6. Root model

The current collector remains hybrid during migration. Native-stack scanning
continues to protect synchronous Rust/C activations. Machine frames add a
precise root source.

The registry is a capability lookup table, not a root set. Reachable native-
backed objects trace their Rust-owned state:

1. the Machine vtable trace hook resolves its checked capability and traces
   every live Oop slot in MachineState;
2. the Continuation vtable trace hook traces every live Oop slot in its detached
   segment;
3. Machine tracing also marks scratch StepAction Oops and any generation or
   descriptor metadata represented by Oops;
4. FrameView traces its owning Machine through its declared Oop field and does
   not trace frame storage independently.

For each Frame, tracing consults the immutable FrameDescriptor and marks only
Oop slots. Raw slots are never interpreted as Oop candidates.

The Runtime root provider is limited to scoped host ownership that is not
already represented by a reachable heap Oop: a Machine while a host drive is in
progress, a just-returned StepAction during validation, and host-retained
process-local handles. It must not enumerate every registry entry.

Trace hooks, the scoped provider, and the eventual finalization hook obey the
existing synchronous GC callback rules:

- it does not allocate;
- it does not call IdApi.send;
- it does not retain the mark callback;
- it does not mutate the machine registry while enumerating;
- it validates the capability before visiting external state;
- it releases any registry borrow before invoking callbacks if the registry
  representation requires that separation.

The trace/finalization hooks and scoped provider must be installed before any
Machine can allocate a Frame that may survive a collection. Registration is
idempotent and owner-thread checked.

## 7. Allocation and safepoints

A resumable step has one rule for Oop liveness: an Oop that may be live across
an allocation, synchronous Method call, or step return must already be in a
descriptor-marked frame slot.

The compiler and helper API must not leave an Oop only in a C local across:

- birnam allocation;
- a call to a synchronous Method;
- a GC-capable helper;
- a return to the Machine driver.

S1 can enforce this rule manually for the test StepMethod. S2 must validate it
through emitted frame metadata and targeted forced-collection tests.

During a synchronous Method call, the current Machine Oop remains in the
driver's scoped root set. The native-stack scan covers temporary C/Rust locals;
the Machine trace hook covers frame values that must survive after the call
returns.

## 8. Delimiters and detached segments

A delimiter record identifies the frame boundary to which Capture and Abort may
return. The record is Rust-owned and contains:

- owning Machine identity;
- frame boundary handle;
- delimiter kind/version;
- result-slot policy;
- owner thread token.

Capture returns a process-local Continuation Oop whose checked capability owns
the detached segment. Detaching transfers ownership of those Frames and their
generation pins from the Machine's active stack to the ContinuationState. The
delimiter remains attached to the Continuation until its one-shot restore or
destruction.

Restore validates owner, Machine, delimiter, frame handles, and generation pins
before installation. Installation and the one-shot consumed transition are one
Rust-owned state change; no generated callback runs between validation and
commit.

Destroying or finalizing a Continuation invalidates its capability and drops its
Frame roots and generation pins. A consumed Continuation has no restorable
Frame segment.

## 9. Code-generation pinning

Each resumable Frame holds a pin on the generation that owns its step address and
FrameDescriptor. The pin is acquired when the frame is pushed and released when
the frame is popped, detached into a Continuation, or destroyed.

The generation registry must keep both:

- the native code handle alive so the step address remains callable;
- the descriptor and source metadata alive so the frame remains interpretable.

Pinning is separate from the durable Store projection of the current dispatch
generation. A durable generation record does not by itself prove that a
process-local native code handle remains mapped.

No S3 operation may call dlclose or equivalent code disposal while a Frame or
Continuation retains that generation. Frame migration across generations is not
part of this contract.

## 10. Inspection boundary

Inspection creates a process-local FrameView Oop on demand. A FrameView contains
its owning Machine Oop as a traced field and a raw arena identity, Frame handle,
and Frame generation. It is a read-only capability, not the Frame and not an
execution argument. Creating two views of the same Frame need not return the
same Oop.

The FrameView Birnam protocol returns copied, safe data:

- implementation and generation identity;
- selector or method identity;
- source-map location;
- program counter;
- slot kinds;
- selected Oop values where the caller has permission.

Before every access, its primitive-backed reader validates the Machine
capability and Frame generation. A view of a popped Frame or destroyed Machine
answers a kinded staleFrameView failure; it never aliases a reused arena slot.

Inspection does not return native function pointers, Frame addresses, arena
addresses, invocation tokens, or mutable references to frame storage. It is
owner-thread-only in the first implementation. FrameView's identity and read
protocol are metacircularly visible even while its reads terminate in Rust
primitives.

## 11. Persistence boundary

Machine, StepAction, FrameView, delimiter, generation-pin, and Continuation
values are process-local. Frame is not an Oop at all. None of these capabilities
is a valid durable Store fact, image record, or isolate wire frame.

If any process-local execution capability is reachable from a durable Store
projection, image validation must reject the image with a kinded not-imageable
error before writing or activating a durable image. Continuation uses
continuationNotImageable; the other kinds receive corresponding errors. The
collector may keep a process-local object alive while the process runs; that
does not make it serializable.

The implementation must not encode raw native addresses as a substitute for a
continuation identity.

## 12. 60B acceptance tests

The machine/frame implementation is ready for S1 only when tests cover:

1. frame handles reject use-after-pop and use-after-destroy;
2. descriptors reject invalid program counters and slot kinds;
3. machine roots survive forced collection while Running;
4. suspended roots survive forced collection while Suspended;
5. continuation roots transfer correctly during Capture and Restore;
6. generation pins remain until the last frame/continuation release;
7. a synchronous Method can allocate and re-enter without losing machine roots;
8. invalid actions fail before generated pointer dereference;
9. owner-thread violations fail deterministically;
10. an action from another Machine or stale epoch is rejected before operand
    decoding;
11. a live Machine Oop traces external Frame Oops without the registry rooting
    every Machine;
12. Machine and Continuation finalization invalidates their capabilities and
    releases external state exactly once;
13. stale FrameViews reject reused arena slots;
14. Birnam reflection can observe the canonical Machine, StepAction, and
    FrameView protocols;
15. durable image validation rejects every process-local execution capability.
