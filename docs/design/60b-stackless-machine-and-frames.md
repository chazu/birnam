# 60B — Stackless Machine, Frames, and GC

> **Status:** S0 representation and memory-safety record, proposed for
> acceptance, 2026-07-29.
> This document defines the Rust-owned execution storage required by
> [60A](60a-stackless-execution-semantics.md). It does not freeze the public C
> ABI; the future ABI record must expose only the opaque boundary described here.

## 1. Representation boundary

Machine and Frame are private Rust runtime objects. Their Rust layout must not
become a C ABI promise and must not be embedded in authored artifacts.

Generated native code receives opaque handles and uses a narrow helper surface:

- read and write the program counter;
- read and write an Oop slot;
- read and write a raw slot;
- validate a slot index against the current descriptor;
- construct a slot-indexed Call, TailCall, Return, or Continue action;
- request the S3 suspension operation.

Generated code must never dereference a Machine or Frame pointer directly,
retain either handle after returning from its StepMethod, or pass an Oop pointer
as an action operand.

This resolves the important distinction between an opaque ABI and the
implementation's private frame layout: C code sees accessors, while Rust owns
storage, validation, allocation, and tracing.

## 2. Machine lifecycle

A Machine has one owner thread, inherited from the Runtime that created it.
The first implementation is single-threaded and owner-affine:

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

The state transition is owned by Rust. Generated code can request an action but
cannot directly change Machine state.

## 3. Machine storage

The private representation is conceptually:

    Machine
      owner thread token
      frame arena
      current action
      delimiter records
      suspension state
      active generation pins
      lifecycle state

The frame arena uses stable chunks or segments. A frame is not a separately
GC-allocated Birnam object and does not enter the Store. Chunk allocation and
reuse are Rust operations.

Frame handles are validated by an arena identity and generation counter. A
handle to a popped frame, released chunk, or destroyed Machine must fail before
reading memory. Raw pointers may be used inside one Rust driver step for
performance, but they cannot escape that step or cross a foreign callback.

The arena should retain capacity after ordinary returns when that is cheaper
than releasing chunks. Suspended Machines retain their active chunks. Destroyed
Machines release all chunks after their generation pins and Oop roots have been
removed from the runtime registry.

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

Actions refer to slots and ranges, not borrowed pointers:

| Logical action | Slot operands |
|---|---|
| Call | receiver slot, selector slot, argument start/count, result slot |
| TailCall | receiver slot, selector slot, argument start/count |
| Return | value slot |
| Continue | none |
| Suspend | suspension/result slot and delimiter identity |
| Capture | delimiter identity and result slot |
| Restore | continuation handle and supplied-value slot |

The exact C representation is an ABI decision, but it must be a fixed-size
tagged value with bounded integer operands. It must not contain a pointer to a
temporary argument array or an unvalidated Oop.

Before returning Call, generated code stores every receiver, selector, argument,
and live local in frame slots. The driver may then construct a temporary
argument buffer from those rooted slots while invoking a synchronous Method.
The buffer remains valid only for that call, preserving the existing IdApi
contract without making a C compound literal part of the continuation state.

An action is validated against the current descriptor before dispatch:

- each slot index is in range;
- each Oop operand is marked as an Oop slot;
- argument ranges do not overlap the result slot unexpectedly;
- the program counter is valid for the implementation;
- the action is legal in the current Machine state.

Invalid actions return invalidStepAction or invalidFrameSlot without
dereferencing generated memory.

## 6. Root model

The current collector remains hybrid during migration. Native-stack scanning
continues to protect synchronous Rust/C activations. Machine frames add a
precise root source.

Each Runtime owns one machine registry and registers one Rust root provider with
the existing GC metadata shell. The provider enumerates:

1. every New or Running Machine;
2. every Suspended Machine;
3. every live Continuation segment;
4. generation and descriptor metadata that is represented by Oops.

For each frame, the provider consults the immutable FrameDescriptor and marks
only Oop slots. Raw slots are never interpreted as Oop candidates.

The provider obeys the existing synchronous mark-callback rule:

- it does not allocate;
- it does not call IdApi.send;
- it does not retain the mark callback;
- it does not mutate the machine registry while enumerating;
- it copies the machine/continuation list before invoking the callback if the
  registry representation requires a borrow to be released.

The provider must be installed before any Machine can allocate a frame that may
survive a collection. Registration is idempotent and owner-thread checked.

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

During a synchronous Method call, the current Machine remains registered as a
root source. The native-stack scan covers temporary C/Rust locals; the machine
frame provider covers values that must survive after the call returns.

## 8. Delimiters and detached segments

A delimiter record identifies the frame boundary to which Capture and Abort may
return. The record is Rust-owned and contains:

- owning Machine identity;
- frame boundary handle;
- delimiter kind/version;
- result-slot policy;
- owner thread token.

Capture detaches the frames above the delimiter into a Continuation segment.
Detaching transfers ownership of those frames and their generation pins from the
Machine's active stack to the Continuation. The delimiter remains attached to
the Continuation until its one-shot restore or destruction.

Restore validates owner, Machine, delimiter, frame handles, and generation pins
before installation. Installation and the one-shot consumed transition are one
Rust-owned state change; no generated callback runs between validation and
commit.

Destroying a continuation drops its frame roots and generation pins. A
consumed continuation has no restorable frame segment.

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

Inspection returns copied, safe data:

- implementation and generation identity;
- selector or method identity;
- source-map location;
- program counter;
- slot kinds;
- selected Oop values where the caller has permission.

Inspection does not return native function pointers, frame addresses, arena
addresses, or mutable references to frame storage. It is owner-thread-only in
the first implementation.

## 11. Persistence boundary

Machine, Frame, delimiter, generation-pin, and Continuation values are
process-local. They are not valid Store facts, image records, or isolate wire
frames.

If a process-local continuation is reachable from a durable Store projection,
image validation must reject the image with continuationNotImageable before
writing or activating a durable image. The collector may keep the process-local
object alive while the process runs; that does not make it serializable.

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
10. durable image validation rejects process-local continuation reachability.
