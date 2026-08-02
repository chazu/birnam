# 60A — Stackless Execution Semantics

> **Status:** S0 decision record, proposed for acceptance, revised 2026-08-01.
> This document turns the stackless overview into an executable semantic
> contract. It deliberately defines the first implementation surface without
> committing Birnam to durable or multi-shot continuations.

## 1. Purpose

The stackless runtime needs one meaning for a language call whether the method
is currently implemented as:

- a synchronous Rust Method;
- a resumable native StepMethod; or
- a nested machine entered from a synchronous callback.

This record defines the behavior visible at those boundaries. Machine and
StepAction cross the compiler/runtime boundary as Birnam Oops. Frame storage
does not. [60B](60b-stackless-machine-and-frames.md) defines that object/storage
boundary. [60C](60c-stepmethod-calling-abi-options.md) proposes reusing the
existing Method calling shape and accessing private Frame state through a
primitive-backed Machine protocol rather than a second native API.

## 2. S0 decisions

These are the proposed first-delivery decisions.

| Area | First-delivery decision |
|---|---|
| Execution | A Machine is a first-class, process-local Birnam object backed by owner-thread Rust state. A drive either completes, suspends, or fails with a typed host result. |
| Continuations | Delimited and one-shot. A consumed continuation cannot be restored again. |
| Public surface | No public continuation syntax is required for S1 or S2. S3 begins with an explicit host/test suspension operation; language syntax is a later policy decision. |
| Capture | Capture is legal only inside a resumable machine and never crosses a native or foreign barrier. |
| Native methods | Existing Method implementations remain synchronous and atomic with respect to the machine. They may re-enter Birnam, but the machine must drive nested resumable work to completion before the native call returns. |
| Errors | Ordinary Birnam errors remain Oop values. Driver, ownership, malformed-action, and barrier failures are out-of-band typed runtime errors. |
| Code generations | A suspended frame continues in the generation that owns its step function and layout. Sends performed after restoration resolve through the current dispatch generation. |
| Durability | Machine, StepAction, FrameView, and Continuation Oops are process-local. Image and durable Store persistence reject them with a kinded error. |
| Scheduling | There is no scheduler in S1–S3. Suspension returns control to the host; a later reactor design may schedule resumptions. |
| Thread migration | A Machine and its continuations may only be driven and restored by their owner thread in the first implementation. |

These decisions narrow the proof. They do not choose whether the eventual public
surface is generators, coroutines, shift/reset, or a lower-level continuation
object.

## 3. Semantic objects

### Machine

A Machine is a first-class Birnam Oop representing one logical execution. Its
Birnam vtable owns the inspect/drive/resume protocol. Its raw object payload is
a checked capability for private Rust MachineState, which contains the frame
stack, delimiter state, suspension state, and references that keep active code
generations alive. The Oop does not embed a Rust pointer or expose the
MachineState layout.

A Machine has exactly one owner thread. The owner is inherited from the Rust
Runtime that created it. A Machine is not Send, Sync, or an image value.

### Frame

A Frame is one resumable language activation. It records an implementation
identity, program counter, receiver, arguments, live local slots, and a return
destination. Frames are private arena records, not separately allocated Oops.
Inspection may create a process-local FrameView Oop, but generated execution
does not receive a Frame object. The frame representation, view validity, and
slot tracing rules are defined in 60B.

### StepMethod

A StepMethod executes one bounded portion of the Machine's current Frame and
returns one StepAction Oop to the Machine driver. The calling contract proposed
in [60C](60c-stepmethod-calling-abi-options.md) reuses the existing Method shape:
Machine is the receiver, the initial driver call has zero ordinary arguments,
and the result is the StepAction. Generated code accesses current Frame state
through ordinary sends to a narrow synchronous, primitive-backed Machine
protocol. It receives neither a Frame Oop, Frame pointer, nor raw invocation
token.

### StepAction

A StepAction is a first-class but process-local Birnam Oop in the execution
protocol between a StepMethod and the Machine driver.

> Birnam owns the object identity and protocol; Rust temporarily owns the
> low-level storage and primitive operations behind it.

StepAction and its canonical CallAction, TailCallAction, ReturnAction,
ContinueAction, SuspendAction, CaptureAction, RestoreAction, and AbortAction
specializations are metacircularly visible through normal Birnam identity,
vtable, method, and reflection facilities. Their constructors and accessors may
initially be primitive-backed, but their protocol is not a Rust-only enum or an
opaque sink. A later Birnam implementation may replace those primitives without
changing the fact that a StepMethod returns an Oop implementing the StepAction
protocol.

This ownership split is frozen for the stackless path: a future action-layout
record may version the physical payload, but it must not replace the returned
Oop with a Rust-only enum, native handle, or action sink. Metacircular
visibility does not require the irreducible outer driver to dispatch a message
to the action.

The action operands are bounded raw slot indices, counts, delimiter identities,
and other validated process-local handles; live language values remain in
traced Frame slots.

The Rust driver recognizes these canonical vtables and decodes their declared
payload directly. It must not send `execute` or another ordinary Birnam message
to interpret an action: ordinary dispatch is the operation the trampoline is
in the process of implementing, so doing so would recurse through the same
boundary.

StepActions are ephemeral control tokens, not durable user values. Each action
identifies its owning Machine generation and drive epoch. The driver rejects an
action from another Machine or an earlier epoch before reading its operands.
This permits an implementation to reuse per-Machine scratch action objects.
There is no public `submitAction:` operation: validity comes from being returned
by the currently executing step in the current epoch, not from possession of the
Oop. Retaining an action does not retain a Frame or grant independent control of
the Machine.

### Continuation

A Continuation is a first-class, process-local Birnam Oop that owns a detached,
delimited segment of a Machine frame stack plus the delimiter and generation
pins needed to restore it. It is an identity object, not a value-equivalent
copy. Like Machine, its Birnam-visible protocol is initially backed by a checked
capability to private Rust state.

The first implementation gives every Continuation exactly one successful
restore. A failed restore does not consume it unless the failure occurs after
the runtime has installed the segment; the implementation must make that
transition atomic.

## 4. Machine actions

The logical action set is:

| Action | Meaning |
|---|---|
| Call | Resolve a receiver and selector, then push a resumable frame or invoke a synchronous Method. |
| TailCall | Resolve a receiver and selector, then replace the current frame. |
| Return | Place one frame slot in the caller's result slot and pop the current frame. |
| Continue | Re-enter the current frame at its updated program counter. |
| Suspend | Detach the current drive from the host and return a process-local suspension. |
| Capture | Detach a delimited one-shot continuation. |
| Restore | Install an unconsumed continuation with one supplied answer. |
| Abort | Unwind to an explicit delimiter or return a typed driver failure. |

S1 implements Call, TailCall, Return, and Continue. S3 adds Suspend, Capture,
and Restore. Abort is initially limited to malformed actions, barrier
violations, and explicit driver failures; language conditions and restarts are
later policy.

Action operands are slot references or other validated process-local indices,
never borrowed Oop pointers or pointers into a generated C compound literal.
This lets the driver keep all live values rooted while it invokes a synchronous
leaf.

## 5. Call and return behavior

When a resumable frame returns a CallAction Oop:

1. The driver validates the referenced receiver, selector, argument range, and
   result slot against the frame descriptor.
2. It resolves the current method address.
3. If the address is registered as resumable, it pushes a frame for that
   implementation and generation.
4. Otherwise it enters the synchronous Method barrier, invokes the Method with
   a driver-owned argument buffer, and writes the returned Oop to the result
   slot.
5. The caller resumes at its saved program counter.

TailCall performs the same resolution but reuses the current frame storage when
the new descriptor fits, or replaces it when it does not. It never retains the
current return destination after the tail call.

Return with no caller completes the Machine. Return with a caller writes only
the declared result slot before popping the frame.

A synchronous Method may call IdApi.send recursively. Such sends may enter
resumable methods, but they must drive them to completion before returning to
the synchronous Method. The active synchronous Method remains a barrier for
capture.

## 6. Suspension and continuation behavior

### Host suspension

S3 first exposes suspension through an explicit host/test operation. The
operation returns:

- Completed(value);
- Suspended(process-local handle); or
- Failed(driver error).

No ordinary Birnam method can accidentally suspend merely because it calls a
resumable method. Suspension must be requested by the selected S3 operation or
later language policy.

### Delimited capture

Capture detaches the frames above the nearest active delimiter. It fails with
continuationBarrier when no legal delimiter exists before a native or foreign
frame. The captured segment retains its frame descriptors and code generations.

Restore requires:

- the same owner thread;
- an unconsumed continuation;
- a compatible delimiter;
- a supplied answer represented as a rooted Oop slot.

Restore installs the segment, supplies the answer to the capture result slot,
and resumes at the saved program counter. A second restore returns
continuationConsumed and does not re-enter generated code.

Nested delimiters are permitted. A continuation cannot restore across a
different Machine in S3.

## 7. Native and foreign barriers

The runtime enters a synchronous barrier before invoking any Method that is not
registered as resumable. The barrier is dynamically scoped and is visible to
capture validation.

While a synchronous barrier is active:

- nested resumable sends may run to completion;
- the native activation may retain Oops on its own stack;
- capture that would include that activation fails deterministically;
- the runtime does not copy, switch, or expose the native stack.

Foreign calls and C callbacks are barriers for the duration of the foreign
activation. A C callback may enter a nested Machine only if it drives that
Machine to completion before returning its declared C result. Asynchronous
foreign work must use an explicit completion operation later; it cannot retain
a Birnam continuation through an unaware C caller.

## 8. Code-generation behavior after restoration

The current Frame's step address and descriptor remain pinned to the generation
that created it. A step resumed from that frame therefore uses the old layout
and program-counter interpretation.

Any new send performed by that step is an ordinary current-generation lookup.
If it resolves to a new resumable implementation, the new frame pins the new
generation. If it resolves to a synchronous Method, the new frame enters the
current synchronous barrier.

There is no suspended-frame migration across recompilation in S3. A future
migration feature would need explicit frame maps and a conversion policy.

## 9. Failure taxonomy

The first driver must distinguish at least:

- invalidMachineOwner;
- machineAlreadyRunning;
- machineNotSuspended;
- continuationConsumed;
- continuationBarrier;
- continuationWrongMachine;
- invalidStepAction;
- staleStepAction;
- stepActionWrongMachine;
- invalidFrameSlot;
- missingImplementation;
- codeGenerationUnavailable;
- continuationNotImageable.

These are host/runtime failures. Language-level errors continue to be ordinary
Birnam Oop results and must pass through the same Return path as any other
value.

## 10. S0 acceptance tests

Before S1 implementation is accepted, tests must specify:

1. a synchronous send still returns its existing Oop result;
2. a resumable Call can invoke a synchronous leaf and continue;
3. a TailCall does not grow frame depth;
4. a nested Machine remains owner-thread constrained;
5. a barrier rejects capture without copying native state;
6. one-shot restore rejects the second restore;
7. generation pins survive a suspended frame and release after consumption;
8. a continuation cannot be serialized into an image or durable Store fact;
9. malformed action and slot references fail before dereferencing generated
   pointers;
10. an action from another Machine or an expired drive epoch is rejected;
11. the driver decodes canonical StepAction objects without recursively sending
    to them.
The public control surface and scheduler remain intentionally undecided after
these tests pass.
