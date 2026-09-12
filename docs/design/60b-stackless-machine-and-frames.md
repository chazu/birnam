# 60B — Stackless Machine, Frames, and GC

> **Status:** S0 representation and memory-safety record, accepted 2026-08-04;
> resolve-and-pin ownership clarification accepted 2026-08-05; Runtime
> ownership, acceptance staging, and code-disposition staging accepted
> 2026-08-06, together with the S1 Machine-microcode protocol; descriptor-v2
> per-PC roots, colored-slot ownership, and edge-transfer amendments accepted
> 2026-08-07 with 60F; S3 detached segments, Continuation tracing/finalization,
> and direct continuation-pin accounting implemented and accepted 2026-08-23.
> This document defines the Birnam-object/private-Rust-storage boundary required
> by [60A](60a-stackless-execution-semantics.md). It does not own the StepMethod
> calling contract; the accepted decision in
> [60C](60c-stepmethod-calling-abi-options.md) preserves the boundary described
> here. [60F](60f-resumable-compiler-lowering.md) owns the S2 compiler analysis
> and physical Frame-layout construction that satisfy this memory contract;
> [60I](60i-stackless-artifact-byte-grammar.md) owns the exact descriptor-v2
> bytes.

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

Generated native code receives the Machine Oop as its existing Method receiver
and uses ordinary IdApi sends to this frozen, primitive-backed S1 protocol:

| Selector | S1 meaning |
|---|---|
| `oopAt:` | Read one descriptor-validated Oop slot. |
| `oopAt:put:` | Write one descriptor-validated Oop slot and answer the stored Oop. |
| `continueAt:` | Select an exact target from the current PC's descriptor edge group, validate and install it, then publish ContinueAction. |
| `callSite:` | Validate an immutable Call call-site descriptor, install its caller continuation PC, then publish its expanded CallAction. |
| `tailCallSite:` | Validate an immutable TailCall call-site descriptor, then publish its expanded TailCallAction. |
| `returnAt:` | Validate an Oop result slot, then publish ReturnAction. |
| `suspendAt:continueAt:` | Stage the matching result-bearing Call edge and publish SuspendAction; commit when resumed with its answer (60P). |
| `captureAt:continueAt:` | Stage the matching Call edge and publish CaptureAction; restore supplies its answer and commits the edge (60P). |
| `abortAt:` | Validate an Oop reason slot, then publish AbortAction. |

Descriptor layout v2 preserves these publisher meanings. Because Continue and
Call install their successor PC before the driver has copied slot-referenced
action operands or committed colored-slot ownership, their checked publishers
also create a short Runtime-private pending-transition record. The Frame keeps
tracing its predecessor root-state PC until the driver validates the exact
returned action and commits 60F's edge plan. A second edge cannot be staged
while one is pending. [60P](60p-v2-control-completion.md) permits converting the
same pending Call to Suspend or Capture with the exact same result slot and
successor PC; resume/restore commits it once. No StepAction field is added.
[60F §10](60f-resumable-compiler-lowering.md) defines the transfer ordering.

A Continue PC may have multiple statically admitted successors. Its descriptor
edges form one contiguous group ordered by target PC; `continueAt:` must select
exactly one member by the requested next PC. A missing target rejects before
the Frame, pending-transition state, or semantic PC changes. A Call PC still
has exactly one continuation edge selected by its immutable call site.

Slot, call-site, and program-counter operands are nonnegative SmallInts. The
StepMethod receives its current program counter as the descriptor-declared
resume token, so S1 exposes no separate program-counter getter or setter. Each
operation performs its own complete validation before mutation, so there is no
standalone `validateSlot:` message. Frame slots are all Oops; layout V1 and its
private raw Frame slots were removed by 60P. A descriptor declaring no resume-token argument is
therefore valid only for a single-entry StepMethod that never needs to inspect
its current PC; a multi-PC StepMethod declares the one-argument form.

Machine alone publishes drive-valid actions because it owns the current
capability and drive epoch. Canonical StepAction prototypes own vtable identity
and reflection, but do not stamp actions. Immutable call-site descriptors own
static slot and continuation-PC metadata, but are not actions or execution
authority. The Machine uses a rooted scratch-action pool so the steady-state S1
transition path allocates no action objects. Scratch pool shape is private, and
the language promises no fresh identity or stable retained payload.

S3 extends this Machine protocol with its suspension, capture, restore, and
abort-to-delimiter operations without changing the S1 selectors.

The calling contract accepted in
[60C](60c-stepmethod-calling-abi-options.md) adds no invocation token, helper
table, or separate native slot API. Generated code must never dereference a
MachineState or Frame pointer directly or pass an Oop pointer as an action
operand.

This resolves the important distinction between an object protocol and a
storage layout: generated code and Birnam code traffic in Oops while Rust
retains freedom to change frame allocation, validation, and tracing.

### Runtime and code ownership

The production `birnam-id::Runtime` owns one object universe and its private
execution state. That execution state owns:

- the Machine and Continuation capability registries;
- Frame arenas and descriptors held by active execution;
- Runtime-local ResumableEntry recognition and binding retirement;
- protected dispatch-read epochs and entry scopes; and
- Runtime-local registration, Frame, and Continuation physical-code pins.

These concerns should form an audited execution module inside `birnam-id`; they
must not be scattered through dispatch, GC, Codegen, or host code. The module may
later be factored mechanically, but doing so does not create a second semantic
Runtime or a second object floor.

Codegen/TCC and artifact loading remain outside this state. A process code
repository owns physical mapping handles and lends opaque `CodeGeneration`
leases to Runtimes. A Runtime decides whether its execution may still reach a
physical code generation;
the repository performs disposal only after every Runtime has released it. This
keeps `birnam-id` independent of compiler and loader implementation while
preventing either from unloading live Frame code.

The Runtime entry-scope stack is a distinct dynamic borrow domain from the heap,
dispatch dictionaries, Machine registry, MachineState, and Frame arena. It names
the Runtime and Machine for same-execution nested entry but is never the
authority for either identity. TLS may route a host shim to the currently entered
Runtime; it may not supply an ambient current Machine to generated code.

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
- a Machine cannot be driven recursively while its driver is already running,
  except by a stacked drive that establishes an entry-barrier delimiter
  ([60E §5.1](60e-resumable-registration-and-entry.md)); such drives are nested
  within one another, never concurrent, and a nested drive may not pop below its
  barrier;
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

MachineState carries a drive depth, and Running to Completed requires depth
zero. A Return at an entry-barrier frame ends that drive and decrements the
depth; it does not complete the Machine, whose outer drives are still parked in
live native activations and whose outer frames still hold chunks and generation
pins.

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
        active physical-code pins
        lifecycle state

The registry entry owns MachineState but does not make the Machine Oop a GC
root. Reachability flows in the other direction: a live Machine Oop traces the
Oops in its MachineState. Otherwise an unreachable Machine would keep itself
and every frame value alive forever.

The current runtime already has vtable trace hooks and per-object finalizers. S1
must install the appropriate Machine hooks before a collectable Machine Oop
ships; S3 has the same gate for Continuation Oops. Collection invalidates each
capability and releases its registry entry. No new finalization ABI is required.
Explicit completion or close may release state early, but callers are not
required to close an unreachable object for memory safety. Finalization must
obey the existing no-allocation callback boundary and must not run Birnam code
or resurrect the object.

The frame arena uses stable chunks or segments. A Frame is not a separately
GC-allocated Birnam object and does not enter the Store. Chunk allocation and
reuse are Rust operations.

Frame handles are validated by an arena identity and generation counter. A
handle to a popped frame, released chunk, or destroyed Machine must fail before
reading memory. The slot generation advances on every pop, not only on chunk
release: stacked drives ([60E §5.1](60e-resumable-registration-and-entry.md))
push and pop repeatedly at the same arena offsets, which makes slot reuse the
common case and leaves the generation counter as the only discriminator for a
retained FrameView. Raw pointers may be used inside one Rust driver step for
performance, but they cannot escape that step or cross a foreign callback.

S1.1 uses a simple retention baseline rather than putting allocator policy on
the execution fast path. Ordinary pops retain arena chunks for reuse; retained
free capacity is not live and the trace hook never scans it. Suspended Machines
retain every chunk containing an active frame. Destroyed Machines release all
chunks after their generation pins and Oop roots have been removed from
MachineState. The registry generation is advanced before the slot can be
reused.

Frame initialization is part of the GC contract. Before a pushed frame can be
observed by collection or generated code, every descriptor-live Oop slot is
either populated from an already rooted input or initialized to nil. Pop first
makes the frame non-live and advances its slot generation, then releases its Oop
roots and generation pin. S1.1 does not zero whole returned chunks on the
ordinary path; a reused frame initializes its live Oop slots again before a
collection point. Debug poisoning or eager clearing is permitted, but cannot be
required for correctness.

S1.1 exposes current live-frame count, live-frame high-water count, and reserved
arena bytes per Machine, with Runtime totals available to the host diagnostics
surface. Automatic trimming is deferred until measurements justify a policy;
the retained-capacity baseline and its telemetry make that a performance choice
rather than an unspecified lifetime. A later trim policy may release only free
chunks and cannot change Frame, FrameView, tracing, or pin semantics.

## 4. Frame layout

Every resumable implementation has an immutable FrameDescriptor:

    FrameDescriptor
      portable implementation identity
      layout version
      program-counter range
      slot count
      Oop-slot bitmap
      return-slot policy
      call-site descriptor table
      source-map identity

As of [60P](60p-v2-control-completion.md), descriptor layout v2 is the only
Frame representation, including handwritten StepMethods. Layout v1 and raw
Frame slots have been removed. V2 provides:

    entry receiver slot and contiguous entry-argument range
    per-PC root bitmap table
    per-edge transfer and clear plans
    reusable call-staging window description
    logical-value/source-location table reference

The layout-v2 root map for a PC is the safepoint-live union for the complete
StepMethod execution at that PC, not only its live-in set. An edge plan records
logical ownership across physical slot reuse: preserved values, parallel copies,
clears, pending Call-result initialization, and the successor PC. Root-map bit
differences alone cannot describe a transition because slot coloring may assign
different logical values to the same rooted physical slot on adjacent PCs.

The exact descriptor-v2 blob encoding is the 60I pre-implementation gate. Its
semantic contents and validation rules are frozen here and in
[60F §§7–10](60f-resumable-compiler-lowering.md); its normative offsets, record
layouts, bounds, and fixtures belong only to
[60I](60i-stackless-artifact-byte-grammar.md).

Each call-site entry is immutably tagged Call or TailCall. It contains the
receiver and selector Oop slots, argument start and count, and, for Call, the
result slot and caller continuation PC. `callSite:` and `tailCallSite:` require
the matching tag and expand the entry into the corresponding frozen StepAction
payload only after validating every slot, range, and PC.

The frame payload contains:

    Frame
      descriptor identity
      program counter
      root-state program counter (layout v2)
      pending action/edge identity where present (layout v2)
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

StepMethod returns a StepAction Oop. S0 freezes action payload schema v1 as
pointer-sized raw words. Each canonical action is allocated with exactly the
payload size listed below, and its vtable uses the existing all-raw layout form
(`count = 0`, `rest = SLOT_RAW`). The raw rest kind is not permission for a
different payload size: the driver checks `objectPayloadSize` before reading the
common header. The canonical vtable is the action-kind tag; no redundant kind
word appears in the payload.

Words 0–3 are the common header for every action:

| Word | Field | Rule |
|---:|---|---|
| 0 | `schema_version` | Exactly `1` for this layout. |
| 1 | `machine_index` | Registry index of the issuing Machine. |
| 2 | `machine_generation` | Generation paired with `machine_index`. |
| 3 | `drive_epoch` | Current Machine drive epoch. |

The remaining words are fixed by canonical vtable:

| Canonical Oop | Payload words after the common header | Total words |
|---|---|---:|
| CallAction | 4 `receiver_slot`, 5 `selector_slot`, 6 `argument_start`, 7 `argument_count`, 8 `result_slot` | 9 |
| TailCallAction | 4 `receiver_slot`, 5 `selector_slot`, 6 `argument_start`, 7 `argument_count` | 8 |
| ReturnAction | 4 `value_slot` | 5 |
| ContinueAction | none | 4 |
| SuspendAction | 4 `result_slot`, 5 `delimiter_index`, 6 `delimiter_generation` | 7 |
| CaptureAction | 4 `delimiter_index`, 5 `delimiter_generation`, 6 `result_slot` | 7 |
| RestoreAction | 4 `continuation_slot`, 5 `supplied_value_slot` | 6 |
| AbortAction | 4 `delimiter_index`, 5 `delimiter_generation`, 6 `reason_slot` | 7 |

Every name ending in `_slot` is an index into the current descriptor and must
match the slot kind required by the operation. In particular,
`continuation_slot` names an Oop slot containing the Continuation; the action
never embeds that Oop. An argument range is the half-open interval
`argument_start .. argument_start + argument_count`, checked for overflow before
either bound is used. A delimiter identity is an arena index plus generation.
For an Abort with no explicit delimiter, `delimiter_index` is `usize::MAX` and
`delimiter_generation` is zero; no other field admits a sentinel.

The reflective v1 protocol is also frozen. Every action answers `actionKind`,
`schemaVersion`, `machineIndex`, `machineGeneration`, and `driveEpoch`.
Specializations answer the camel-case form of each field listed above:
`receiverSlot`, `selectorSlot`, `argumentStart`, `argumentCount`, `resultSlot`,
`valueSlot`, `delimiterIndex`, `delimiterGeneration`, `continuationSlot`,
`suppliedValueSlot`, and `reasonSlot` where applicable. `actionKind` answers one
of `#call`, `#tailCall`, `#return`, `#continue`, `#suspend`, `#capture`,
`#restore`, or `#abort`. AbortAction additionally answers `hasDelimiter`; its
delimiter accessors answer nil when the raw sentinel is present. Numeric
accessors answer nonnegative Integer Oops. Registry generations and drive epochs
fail deterministically before wrapping beyond that representable range.

A later schema may add a new canonical vtable/layout version, but cannot
reinterpret a live v1 object. No schema may contain a borrowed argument-array
pointer, Frame pointer, or unvalidated Oop. The StepMethod result remains an
Oop, not a Rust enum, tagged C value, or opaque action sink.

The canonical action vtables and field protocol are metacircularly visible.
Birnam code can inspect an action's kind and operands through ordinary methods
and reflection. Those accessors and the Machine publication methods may
initially terminate in Rust primitives; only Machine can stamp a drive-valid
action. The outer driver uses trusted direct decoding so it does not need a
recursive send. Moving an accessor, publisher, or policy method into Birnam does
not change the StepMethod result type or object protocol.

An S1 Machine retains a rooted scratch-action pool and rewrites raw payloads
before returning actions from a step, so steady-state publication allocates no
action objects. The Machine's drive epoch is
strictly monotonic and is never restored to an earlier value: a stacked drive
shares this scratch set, so an epoch that could be restored would revalidate an
outer action over payload the nested drive has overwritten
([60E §5.1](60e-resumable-registration-and-entry.md)). The driver validates
canonical vtable, schema, Machine capability, and epoch before interpreting
variant fields, and copies the operands it needs into driver-local scalars
before releasing its borrow, so no action payload is re-read across generated
code. There is no public action-submission operation: the driver accepts an
action only as the direct result of the active StepMethod. A retained alias
therefore grants no independent right to drive the Machine, and the language
promises neither a fresh StepAction identity nor snapshot payload contents for
every step.

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
   every live Oop slot in MachineState, enumerating the frame arena rather than
   walking the caller chain — a chain walk terminates at an entry-barrier frame,
   whose caller is a native activation, and the outer frames beyond it hold Oops
   in arena chunks that the native-stack scan does not cover;
2. the Continuation vtable trace hook traces every live Oop slot in its detached
   segment;
3. Machine tracing also marks scratch StepAction Oops and any generation or
   descriptor metadata represented by Oops;
4. FrameView traces its owning Machine through its declared Oop field and does
   not trace frame storage independently.

For each Frame, tracing consults the immutable V2 FrameDescriptor and marks
only the Oop slots selected by the Frame's
committed root-state PC. During action publication that root-state PC may
temporarily remain at the predecessor after `continueAt:` or `callSite:` has
installed the semantic successor PC; the pending-transition record prevents
StepMethod re-entry in that interval. Suspension and capture retain this
pending state until a supplied answer commits the Call edge (60P).

During a layout-v2 edge transition, the driver first validates the returned
action against the pending record and roots every copied action operand it still
needs. It then applies the non-allocating parallel-copy and clear plan, advances
the root-state PC to the already installed semantic successor PC, and clears
the pending record. GC therefore observes a complete predecessor root state,
driver-owned transition roots, or a complete successor root state; it never
observes half-applied slot ownership. A dead or reassigned physical Oop slot is
cleared to canonical nil before FrameView or generated code can observe the
successor state.

The private Rust substrate implements this order as of 2026-08-22. Layout-v2
descriptors own dense root maps, validated canonical edge plans, and maximum-
copy-sized scratch reserved when the Frame is pushed. Only begin/commit edge
operations can move its semantic and root-state PCs. Canonical descriptor
admission and generated-code integration use this substrate; 60P completes
suspend/capture publication and removes the old direct-PC mutation path.

The Runtime root provider is limited to scoped host ownership that is not
already represented by a reachable heap Oop: a Machine while a host drive is in
progress, a just-returned StepAction during validation, host-retained
process-local handles, and 60E's dense implementation-anchor slots. An anchor
slot is an explicit mutable GC root owned by one named ResumableEntry; the
provider traces those slots directly and may update them for a moving collector.
It must not walk the capability registry or infer roots from every registry
entry. Generic block entries have no implementation-anchor slot.

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
an allocation, synchronous Method call, late-bound Machine send, callback, or
step return must already be in a descriptor-marked frame slot or an explicitly
scoped driver root. The one bounded exception is 60F's fresh-Oop ingress
handoff: a generated local may carry a new Oop through a compiler-proven
safepoint-free sequence into the immediately following IdApi send. That send
must establish scoped roots for receiver, selector, and arguments before lookup
or allocation, and the store must establish the descriptor root before the
scope ends.

The compiler and Machine protocol must not leave an Oop only in a C local
across:

- birnam allocation;
- a call to a synchronous Method;
- a Machine microcode or other IdApi send;
- a GC-capable helper;
- a callback into host, foreign, or metacircular code;
- a return to the Machine driver.

S1 can enforce this rule manually for the test StepMethod. S2 must validate it
through emitted frame metadata and targeted forced-collection tests. Its per-PC
root map covers every value live at any safepoint while that PC's step executes,
including values created after entry but before the StepMethod returns.

The ingress exception is not permission to trust conservative stack scanning.
The compiler rejects any producer-to-send path containing a Birnam allocation,
helper safepoint, callback, lookup, send, or return to the driver. Source-send
results are rooted and written to the pending caller result slot by the driver;
they do not cross this generated-local bridge.

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

Each resumable Frame holds a physical `CodeGeneration` pin on the mapping that
owns its step address and FrameDescriptor storage. 60E's ResumableEntry holds a
distinct registration-code handle while the entry is live or retiring.
Resolve-and-pin acquires a call-code pin; a successful Frame push transfers that
pin to the Frame, and a failed push releases it. The Frame releases or transfers
its pin when it is popped, detached into a Continuation, or destroyed.
Unregistration releases the registration-code handle only after binding,
reader, entry, Frame, Continuation, and explicit-lease quiescence.

The physical-code registry must keep both:

- the native code handle alive so the step address remains callable;
- the descriptor and source metadata alive so the frame remains interpretable.

Physical pinning is separate from the durable Store projection of the current
`DispatchGeneration`. A durable dispatch record does not by itself prove that a
process-local native code handle remains mapped. Conversely, a mapped
`CodeGeneration` does not identify the logical DispatchGeneration in which one
of its entries is published.

From S1 onward, no operation may call `dlclose` or equivalent code disposal
without the owning Runtimes' safety verdicts. S1 accounts for registrations,
bindings, readers, acquired entries, Frames, and explicit leases; S3 extends
that set with Continuations. Frames and continuations are not the only holders:
a heap vtable can outlive the generation whose code it names. Until vtable
reachability is computable, S1 conservatively retains every generation that
installed a method. [60E §7](60e-resumable-registration-and-entry.md) defines
the structured physical-disposition result and the later reclamation gate.

Frame migration across generations is not part of this contract.

## 10. Inspection boundary

Inspection creates a process-local FrameView Oop on demand. A FrameView contains
its owning Machine Oop as a traced field and a raw arena identity, Frame handle,
and Frame generation. It is a read-only capability, not the Frame and not an
execution argument. Creating two views of the same Frame need not return the
same Oop.

The FrameView Birnam protocol returns copied, safe data:

- portable implementation identity, runtime-local DispatchGeneration
  association, and physical CodeGeneration identity as distinct values;
- selector or method identity;
- source-map location;
- program counter;
- slot kinds;
- selected Oop values where the caller has permission.

Before every access, its primitive-backed reader validates the Machine
capability and Frame generation. A view of a popped Frame or destroyed Machine
answers a kinded staleFrameView failure; it never aliases a reused arena slot.

Inspection does not return native function pointers, Frame addresses, arena
addresses, or mutable references to frame storage. It is owner-thread-only in
the first implementation. FrameView's identity and read protocol are
metacircularly visible even while its reads terminate in Rust primitives.
During a layout-v2 pending transition, a FrameView may report the kinded
`frameTransitionPending` state but physical slot reads fail with that kind
rather than exposing predecessor values under a successor source location.

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

S0 acceptance freezes this complete matrix. Each item is labelled with the
first stage that can execute it; a stage gates only on its own and earlier
items.

1. **(S1)** frame handles reject use-after-pop and use-after-destroy;
2. **(S1)** descriptors reject invalid program counters, slot kinds, call-site
   tags, and call-site fields;
3. **(S1)** machine roots survive forced collection while Running;
4. **(S3)** suspended roots survive forced collection while Suspended;
5. **(S3)** continuation roots transfer correctly during Capture and Restore;
6. **(S1)** registration and physical call-code pin counts prove acquire, failed-
   push release, successful transfer to Frame, and final release independently
   of the unload-refusal gate;
7. **(S3)** pin counts prove transfer from Frame to Continuation and final
   release after consumption or finalization;
8. **(S1)** a synchronous Method can allocate and re-enter without losing
   machine roots;
9. **(S1)** every checked Frame primitive rejects the wrong Machine owner or
   lifecycle, stale drive epoch, descriptor mismatch, invalid slot, and wrong
   slot kind before that primitive mutates storage;
10. **(S1)** owner-thread violations fail deterministically;
11. **(S1)** an action from another Machine or stale epoch is rejected before
    operand decoding;
12. **(S1)** a live Machine Oop traces external Frame Oops without the registry
    rooting every Machine;
13. **(S1)** Machine finalization invalidates its capability and releases
    external state exactly once;
14. **(S3)** Continuation finalization invalidates its capability and releases
    external state exactly once;
15. **(S1)** stale FrameViews reject reused arena slots;
16. **(S1)** Birnam reflection can observe the canonical Machine, StepAction, and
    FrameView protocols;
17. **(S1)** durable image validation rejects every process-local execution
    capability introduced through S1;
18. **(S3)** durable image validation also rejects Continuation and suspension
    capabilities introduced in S3.
19. **(S1)** each frozen Machine publisher returns the canonical action kind,
    stamps the current Machine and epoch, and allocates no action object on the
    steady-state path; retained scratch aliases grant no submission authority.
20. **(S1.1)** forced collection immediately after frame push observes every
    descriptor-live Oop slot as rooted or nil, while a popped frame retained in
    arena capacity contributes no roots and its offset cannot be read through
    the old generation;
21. **(S1.1)** arena diagnostics report current and high-water live frames and
    reserved bytes, and finalizing or explicitly destroying a Machine returns
    all of its reserved arena capacity after roots and pins drain;
22. **(S2)** descriptor-v2 validation rejects missing or malformed PC root maps,
    out-of-range/non-Oop bits, invalid padding, and edge plans that cannot
    establish the successor root state;
23. **(S2)** every safepoint-live logical value is in the current root-state
    PC's colored map, including a value created before late-bound microcode
    allocates;
24. **(S2)** interfering logical values never share a slot, while a fixture with
    disjoint live ranges proves that coloring reuses one;
25. **(S2)** a physical slot whose logical owner changes across an edge is
    cleared or transferred even when its root bit is set in both PC maps;
26. **(S2)** parallel-copy cycles preserve every rooted Oop without exposing a
    half-applied transition to collection;
27. **(S2)** a pending Call result is rooted as nil and receives the result
    before the successor StepMethod runs;
28. **(S2)** forced collection at step entry, after generated stores, in
    late-bound microcode, during a synchronous leaf, while a caller is parked,
    and immediately after transition preserves the required roots;
29. **(S2)** FrameView never exposes an untraced stale Oop from a dead or
    reassigned colored slot; and
30. **(S2)** live and AOT artifacts produce identical layout-v2 coloring, root
    maps, edge transfers, call windows, and source-location identities;
31. **(S2)** `continueAt:` and `callSite:` preserve their frozen PC-installing
    semantics while tracing remains on the predecessor root-state map until the
    exact returned action commits; and
32. **(S2)** mismatched action return, repeated publication, StepMethod re-entry,
    Frame mutation, and FrameView slot reads while a transition is pending fail
    deterministically without dropping predecessor roots.
