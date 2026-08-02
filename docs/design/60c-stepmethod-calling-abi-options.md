# 60C — StepMethod Calling ABI Options

> **Status:** Request for comment, 2026-08-01. No ABI decision has been made.
> This record presents four deliberately contrasting designs for external human
> review. It records a current lean, not an accepted contract.

## 1. Question

What C calling ABI should connect a native-compiled resumable StepMethod to the
stackless Machine driver?

The answer is difficult to reverse once independently compiled artifacts ship.
It must preserve Birnam's small floor without turning the execution machinery
into opaque Rust policy, and it must leave enough representation freedom for
the runtime, compiler, and garbage collector to evolve.

This record narrows only the StepMethod invocation and Frame-access boundary.
Resumable method registration, complete artifact descriptors, compiler lowering,
and host drive results remain separate decisions even where an option below
places requirements on them.

At a glance:

- **Option A** optimizes for physical compatibility with the existing Method
  shape.
- **Option B** optimizes for the smallest conceptual interface.
- **Option C** optimizes for generated-code throughput.
- **Option D** optimizes for explicit authority and versioned evolution. It is
  the current lean.

## 2. Constitutional constraints

The options are evaluated against the current architectural commitments:

1. Birnam is a fully malleable live object environment. Object identities and
   language protocols should remain inspectable and replaceable.
2. The id floor is independently usable mechanism. Policy belongs in loadable
   Birnam-visible layers where the bootstrap regress permits it.
3. Source-loaded and binary-loaded methods must have the same semantics and
   enter through the same registration and execution contracts.
4. Machine and StepAction are first-class Birnam Oops. StepMethod returns a
   StepAction Oop, not a Rust enum, C tagged union, or opaque action sink.
5. Frame is private execution storage, not an Oop. A FrameView Oop may provide
   read-only inspection, but generated execution never receives a Frame object.
6. StepAction operands name rooted Frame slots. They never borrow Oop pointers,
   temporary argument arrays, or Frame addresses.
7. The outer driver must decode a canonical StepAction without sending a
   message to perform that decoding. Ordinary dispatch is the operation the
   driver is implementing; using it at the regress-breaking point would recurse.
8. A StepMethod may allocate. Every Oop live across an allocation, synchronous
   native call, or step return must first be stored in a traced Frame slot.
9. The existing synchronous Method ABI remains supported. A language send from
   resumable generated code must not assume its target will remain synchronous
   after live replacement.
10. New ABI surface is versioned. An old ABI is never silently reinterpreted as
    a new one.

The governing ownership rule is:

> Birnam owns the object identity and protocol; Rust temporarily owns the
> low-level storage and primitive operations behind it.

The current Method ABI is:

```c
typedef Oop (*Method)(Oop receiver, const Oop *args, size_t count);
```

The current `IdApi` is append-only and already broad. None of the options below
requires adding StepMethod operations to `IdApi`; a separate versioned execution
ABI can remain a narrower seam.

Relevant prior records are:

- [30 — The destination](30-the-destination.md), especially the five invariant
  goals and frozen-floor/loadable-layer distinction;
- [42 — Object canonical, relational derived](42-object-canonical-relational-derived.md),
  especially raw floor mechanism versus the supported malleable MOP;
- [60A — Execution semantics](60a-stackless-execution-semantics.md);
- [60B — Machine, Frames, and GC](60b-stackless-machine-and-frames.md); and
- the current [IdApi ABI evidence](../../abi/README.md).

## 3. Shared requirements

Every acceptable design must arrange that:

- one invocation identifies exactly one active Machine and current Frame;
- invocation-scoped native authority cannot be retained and used after return;
- the driver can reject wrong-owner, stale, malformed, or wrong-Machine use;
- Oop and raw Frame slots remain distinguishable;
- writes preserve the collector's current and future barrier requirements;
- program-counter and slot bounds are validated before Rust indexes storage;
- exactly one StepAction Oop is returned;
- no Rust panic or foreign unwind crosses the C boundary;
- artifact metadata identifies the StepMethod ABI before registration;
- live and AOT code use the same ABI;
- synchronous Method and resumable StepMethod entries cannot be confused.

The four options disagree over what the ABI should expose in order to meet those
requirements.

## 4. Option A — Reuse the Method-shaped ABI

### Shape

Keep the existing physical function shape and give its arguments a StepMethod-
specific meaning:

```c
typedef Oop (*StepMethodV1)(
    Oop machine,
    const Oop *control,
    size_t control_count
);
```

For V1, `control_count` is one and `control[0]` is an ephemeral lease Oop. The
lease identifies this invocation but is not a Frame or a public execution
object. One checked primitive provides slot access:

```c
enum BirnamStepSlotOperationV1 {
    BIRNAM_STEP_SLOT_LOAD,
    BIRNAM_STEP_SLOT_STORE
};

uint32_t birnam_step_slot_v1(
    Oop machine,
    Oop lease,
    uint32_t operation,
    uint32_t slot,
    Oop *value_io
);
```

One version of this option asks Machine, through ordinary messages, to construct
the returned StepAction:

```c
Oop example_step(Oop machine, const Oop *control, size_t count) {
    Oop lease = control[0];
    Oop pc;
    birnam_step_slot_v1(
        machine, lease, BIRNAM_STEP_SLOT_LOAD, SLOT_PC, &pc
    );

    if (birnam_small_integer_value(pc) == 0) {
        Oop operands[] = {
            lease,
            birnam_small_integer(SLOT_RECEIVER),
            birnam_small_integer(SLOT_SELECTOR),
            birnam_small_integer(SLOT_RESULT)
        };
        return birnam_send(
            machine, SELECTOR_CALL_USING_SLOTS, operands, 4
        );
    }

    Oop operands[] = {
        lease,
        birnam_small_integer(SLOT_RESULT)
    };
    return birnam_send(
        machine, SELECTOR_RETURN_USING_SLOT, operands, 2
    );
}
```

### What it hides

Frame identity and layout remain private. The lease and slot primitive validate
the dynamically current Frame. The existing Method function-pointer carrier can
continue to hold either entry shape, provided registration metadata says which
meaning applies.

### Advantages

- It introduces no second physical three-argument calling convention.
- Existing C toolchains, loaders, and function-pointer carriers already support
  the shape.
- A single checked slot primitive is a very small native mechanism surface.
- The lease makes invocation lifetime more explicit than a Machine-only design.
- Source and binary lowering can reuse much existing generated-C scaffolding.

### Liabilities

- The control array is no longer a language argument array even though its type
  says that it contains Oops.
- A lease represented as an Oop gives object-shaped representation to a value
  that intentionally has no language identity or protocol.
- Method and StepMethod become physically indistinguishable. A missing or stale
  registration tag can call an address under the wrong semantic contract.
- Ordinary message sends for action construction create a regress. The selected
  constructor must remain synchronous and atomic even if Birnam later replaces
  it with resumable code.
- Replacing those sends with native action helpers removes much of this option's
  metacircular advantage and makes it converge toward Option B.
- The meaning of `control` and `control_count` is difficult to extend without
  accumulating conventions inside an API originally designed for arguments.

### Principal review question

Is physical reuse of Method valuable enough to justify giving the same C shape
two unrelated semantic meanings?

## 5. Option B — Machine is the sole capability

### Shape

Pass only the first-class Machine Oop:

```c
typedef Oop (*StepMethodV1)(Oop machine);
```

While StepMethod runs, Machine implicitly selects its current Frame. A small
versioned helper surface provides Frame access and StepAction publication:

```c
uint32_t birnam_step_pc_v1(Oop machine);
Oop birnam_step_load_oop_v1(Oop machine, uint32_t slot);
void birnam_step_store_oop_v1(Oop machine, uint32_t slot, Oop value);

Oop birnam_step_call_v1(
    Oop machine,
    uint32_t next_pc,
    uint32_t receiver_slot,
    uint32_t selector_slot,
    uint32_t argument_start,
    uint32_t argument_count,
    uint32_t result_slot
);

Oop birnam_step_return_v1(Oop machine, uint32_t value_slot);
Oop birnam_step_abort_v1(Oop machine, uint32_t failure);
```

Generated code is compact:

```c
Oop example_step(Oop machine) {
    switch (birnam_step_pc_v1(machine)) {
    case 0:
        return birnam_step_call_v1(
            machine, 1,
            SLOT_RECEIVER, SLOT_SELECTOR,
            SLOT_ARGUMENTS, 1, SLOT_RESULT
        );
    case 1:
        return birnam_step_return_v1(machine, SLOT_RESULT);
    default:
        return birnam_step_abort_v1(machine, INVALID_PC);
    }
}
```

The action helpers return Machine-selected StepAction Oops. They are primitive
storage operations behind Birnam-owned identities and protocols, not hidden
Rust action values.

### What it hides

Machine's native-backed payload resolves to MachineState. MachineState records
which StepMethod invocation, Frame, descriptor, and epoch are dynamically
active. Every helper validates that state before touching storage.

### Advantages

- It has the smallest and clearest StepMethod signature.
- The only semantic argument and result are Birnam Oops.
- No Frame, context layout, helper table, or lease becomes part of the calling
  convention.
- Rust retains complete freedom over Frame representation.
- Typed action helpers are simple for generated C and return the agreed
  metacircular objects.

### Liabilities

- Public Machine identity doubles as ambient authority over its dynamically
  current Frame.
- Invocation identity is real but implicit in MachineState. Helper correctness
  depends on hidden dynamic state rather than an explicit borrowed capability.
- A retained Machine Oop can be presented to a helper outside the StepMethod;
  validation can reject it, but the signature does not make the misuse obvious.
- Nested runtimes, instrumentation, tests, and future reentrancy must all agree
  on what "Machine's current invocation" means.
- Helper symbols themselves become a versioned ABI. Adding operations requires
  new symbols or an additional negotiation mechanism.
- A design that forbids GC inside StepMethod to simplify this state would be too
  restrictive for Birnam and is not considered acceptable here.

### Principal review question

Is implicit dynamic activation a good example of a deep small-floor interface,
or is it hidden ambient authority that should be represented explicitly?

## 6. Option C — Borrowed execution window

### Shape

Give generated C a concrete, invocation-scoped view over Frame storage and a
bank of reusable StepAction objects:

```c
typedef struct BirnamActionCellV1 {
    uint64_t issue;
    uint32_t next_pc;
    uint32_t operands[6];
} BirnamActionCellV1;

typedef struct BirnamActionPortV1 {
    Oop object;
    BirnamActionCellV1 *cell;
} BirnamActionPortV1;

typedef struct BirnamActionBankV1 {
    BirnamActionPortV1 call;
    BirnamActionPortV1 tail_call;
    BirnamActionPortV1 return_;
    BirnamActionPortV1 continue_;
    BirnamActionPortV1 abort;
} BirnamActionBankV1;

typedef struct BirnamStepWindowV1 {
    uint32_t abi_version;
    uint32_t byte_size;

    Oop machine;
    Oop *oop_slots;
    uintptr_t *raw_slots;
    uint32_t oop_slot_count;
    uint32_t raw_slot_count;
    uint32_t pc;

    BirnamActionBankV1 *actions;
    uint64_t issue;
} BirnamStepWindowV1;

typedef Oop (*StepMethodV1)(BirnamStepWindowV1 *window);
```

Header-only publishers fill the native-backed payload of a Machine-owned
StepAction and return that action's Oop:

```c
static inline Oop birnam_step_call_v1(
    BirnamStepWindowV1 *step,
    uint32_t next_pc,
    uint32_t receiver_slot,
    uint32_t selector_slot,
    uint32_t argument_start,
    uint32_t argument_count,
    uint32_t result_slot
) {
    BirnamActionPortV1 *port = &step->actions->call;
    port->cell->next_pc = next_pc;
    port->cell->operands[0] = receiver_slot;
    port->cell->operands[1] = selector_slot;
    port->cell->operands[2] = argument_start;
    port->cell->operands[3] = argument_count;
    port->cell->operands[4] = result_slot;
    port->cell->issue = step->issue;
    return port->object;
}
```

Generated code directly reads slots and publishes actions:

```c
Oop example_step(BirnamStepWindowV1 *step) {
    switch (step->pc) {
    case 0:
        return birnam_step_call_v1(
            step, 1,
            SLOT_RECEIVER, SLOT_SELECTOR,
            SLOT_ARGUMENTS, 1, SLOT_RESULT
        );
    case 1:
        return birnam_step_return_v1(step, SLOT_RESULT);
    default:
        return birnam_step_abort_v1(step, INVALID_PC);
    }
}
```

### What it hides

The complete Rust MachineState and arena remain hidden, but Rust must present
each invocation as contiguous Oop and raw slot banks with stable pointer
lifetimes. The banks may alias Frame storage or be copied execution windows.

### Advantages

- It provides the shortest hot path: one StepMethod call, direct slot access,
  inline action publication, and no per-slot helper calls.
- It allocates no StepAction per transition.
- Oop and raw slots are physically separated, making precise tracing simple.
- The concrete view is easy for C compilers to optimize.
- The issue cookie allows the driver to reject an action published by an older
  invocation.

### Liabilities

- It freezes contiguous slot banks, pointer widths, action-cell layouts, action
  bank ordering, and publication rules in V1.
- Generated code can retain a borrowed pointer, index out of bounds, write the
  wrong slot kind, or bypass a future GC write barrier. Native artifacts are
  trusted, but the ABI makes mistakes easier to turn into memory corruption.
- A moving collector, compressed Oops, segmented frames, or different slot
  representation may require copying windows or a new ABI.
- The action objects remain metacircularly visible, but their storage shape is
  substantially more native-visible than in the other options.
- The performance benefit is assumed rather than measured against a checked
  helper design.

### Principal review question

Is contiguous borrowed Frame storage a representation Birnam is willing to
freeze permanently for an unmeasured hot-path advantage?

## 7. Option D — Explicit invocation plus typed capability table

### Shape

Pass Machine as the semantic object and a versioned, invocation-scoped record as
the private execution capability:

```c
typedef struct BirnamStepInvocationV1 BirnamStepInvocationV1;
typedef struct BirnamStepApiV1 BirnamStepApiV1;

struct BirnamStepInvocationV1 {
    uint32_t abi_version;
    uint32_t byte_size;
    uint64_t activation;
    const BirnamStepApiV1 *api;
};

typedef Oop (*StepMethodV1)(
    Oop machine,
    const BirnamStepInvocationV1 *invocation
);
```

`activation` is a raw, checked token. It is not an Oop, Frame pointer, or public
language capability. The record is borrowed only for the dynamic StepMethod
call.

The table exposes typed primitive mechanisms:

```c
struct BirnamStepApiV1 {
    uint32_t abi_version;
    uint32_t byte_size;

    uint32_t (*pc)(const BirnamStepInvocationV1 *);

    uint32_t (*load_oop)(
        const BirnamStepInvocationV1 *, uint32_t slot, Oop *result
    );
    uint32_t (*store_oop)(
        const BirnamStepInvocationV1 *, uint32_t slot, Oop value
    );
    uint32_t (*load_raw)(
        const BirnamStepInvocationV1 *, uint32_t slot, uintptr_t *result
    );
    uint32_t (*store_raw)(
        const BirnamStepInvocationV1 *, uint32_t slot, uintptr_t value
    );

    Oop (*call)(
        const BirnamStepInvocationV1 *,
        uint32_t next_pc,
        uint32_t receiver_slot,
        uint32_t selector_slot,
        uint32_t argument_start,
        uint32_t argument_count,
        uint32_t result_slot
    );
    Oop (*tail_call)(const BirnamStepInvocationV1 *, /* slot operands */);
    Oop (*return_)(const BirnamStepInvocationV1 *, uint32_t value_slot);
    Oop (*continue_)(const BirnamStepInvocationV1 *, uint32_t next_pc);
    Oop (*abort)(const BirnamStepInvocationV1 *, uint32_t failure);
};
```

These signatures illustrate the typed operation boundary; they do not decide
the failure channel. In particular, reviewers must still choose whether PC and
slot failures return status immediately, latch a driver failure on the
invocation, or use a combination of both.

Later operations can be appended when `byte_size` and declared feature bits say
they are available. An incompatible interpretation receives a coexisting V2;
V1 is never changed in place.

Generated code is explicit without seeing Frame storage:

```c
Oop example_step(
    Oop machine,
    const BirnamStepInvocationV1 *invocation
) {
    const BirnamStepApiV1 *api = invocation->api;

    switch (api->pc(invocation)) {
    case 0:
        return api->call(
            invocation, 1,
            SLOT_RECEIVER, SLOT_SELECTOR,
            SLOT_ARGUMENTS, 1, SLOT_RESULT
        );
    case 1:
        return api->return_(invocation, SLOT_RESULT);
    default:
        return api->abort(invocation, INVALID_PC);
    }
}
```

Typed action publishers obtain and populate the Machine's current canonical
StepAction Oops. The Oops, their vtables, their methods, and their reflective
protocol remain Birnam-owned. The table operation is only the primitive storage
and regress-breaking transition beneath that protocol.

### What it hides

The activation token resolves to the current Frame and descriptor only inside
Rust. Generated code cannot inspect the token or derive a Frame address. The
table can be implemented by different runtime tiers while preserving the same
StepMethod artifact.

### Advantages

- Semantic object authority and native storage authority are visibly separate:
  Machine is an Oop; invocation is a borrowed mechanism capability.
- No hidden thread-local "current Frame" is required.
- Frame layout, arena strategy, GC barriers, and validation remain private.
- Version and size fields support append-only growth and pre-invocation
  compatibility checks.
- A runtime, test harness, instrumented driver, or future implementation tier can
  supply a different operation table without changing generated code.
- Typed operations are easier to validate and document than a generic opcode and
  byte-packet portal.
- It does not add stackless-specific fields to the independently useful IdApi.

### Liabilities

- The signature and two C-layout records become new permanent ABI surface.
- Slot access and action publication involve indirect native calls unless the
  compiler/runtime later establishes a compatible fast path.
- A large table can become a shallow grab bag. Only irreducible Frame and action-
  publication mechanisms belong here.
- The table itself is native mechanism, not a Birnam object. Its operation
  identities and signatures are not live-malleable inside V1.
- Version negotiation, status behavior, and feature requirements add explicit
  machinery that Options A and B initially avoid.

### Principal review question

Does the explicit activation boundary buy enough correctness and evolution room
to justify a second small versioned API beside IdApi?

## 8. Comparison

Option A minimizes change to the existing loader and function-pointer shape, but
it does so by overloading the meaning of a language argument ABI. It appears
especially attractive if physical compatibility is the dominant constraint.
Its weakest point is action construction: ordinary sends are maximally visible
but require a synchronous regress breaker, while native constructors make the
design less distinct from Option B.

Option B has the simplest interface. It treats Machine as a deep object hiding
all execution state, which is idiomatic Birnam. Its cost is that the private
invocation capability has not disappeared; it has become dynamic ambient state.
The decision turns on whether that implicit authority is elegant encapsulation
or avoidable hidden coupling.

Option C is the performance leader by construction. It is also the largest
one-way representation commitment. It should win only if measurements show that
checked slot operations materially violate the agreed stackless overhead budget
and if the project is willing to keep a V1 direct-window tier indefinitely.

Option D exposes more mechanism than Option B but less representation than
Option C. Its interface is deeper than a generic operation portal because each
typed operation hides validation, rooting, barrier, and storage policy. Its main
risk is designing too much API before the compiler demonstrates which operations
are actually necessary.

All four can preserve source/binary parity and return StepAction Oops. None is
made metacircular merely by having more sends. Metacircularity comes from keeping
Machine, StepAction, descriptors, compiler lowering, and control policy in the
object environment while minimizing the native regress breaker.

## 9. Current lean

The current lean is **Option D: explicit invocation plus a typed capability
table**, with the following constraints:

1. The table is separate from IdApi and contains only execution mechanisms.
2. It uses typed functions, not one generic `transact(opcode, bytes)` portal.
3. Machine and StepAction remain the only semantic objects crossing the call.
4. The invocation record is borrowed, opaque in meaning, and invalid after
   StepMethod returns.
5. Typed action publishers return Machine-selected, Birnam-visible StepAction
   Oops; they do not return native action values.
6. A StepMethod may allocate. The liveness rule is enforced through traced
   Frame slots rather than a no-GC restriction.
7. Resumable generated language sends return CallAction rather than recursively
   calling IdApi.send. This preserves replacement between synchronous and
   resumable implementations.
8. V1 starts with only the operations required by an S1 hand-written StepMethod.
   S3 operations are appended only after S1/S2 evidence.

This lean prefers a narrow explicit authority boundary over both hidden dynamic
state and exposed Frame storage. It is not yet a decision because its expected
helper-call cost has not been measured and its smallest sufficient operation set
has not been demonstrated.

Option B is the strongest alternative if reviewers value conceptual minimalism
over explicit invocation authority. Option C should remain available as a later
optimized execution tier if measurement justifies its stronger representation
commitments. Option A is least favored because physical signature reuse does not
remove the semantic need to distinguish the entry kinds.

## 10. Evidence requested before decision

Before freezing an ABI, build throwaway, non-shipping probes for Options B, C,
and D that execute the same hand-written StepMethod and report:

- native stack movement across at least one million tail transitions;
- nanoseconds per Continue, Call/Return pair, and synchronous-leaf adaptation;
- number of indirect/helper calls per transition;
- forced-GC behavior with live Oops in Frame slots;
- malformed slot/action rejection behavior;
- generated C size and readability;
- whether the same fixture can be registered from both live and AOT artifacts.

Option A needs a probe only if a reviewer argues that physical Method-shape reuse
is itself a decisive compatibility advantage.

Benchmarks should compare against the current Rust synchronous Method floor. A
stackless design is not assumed faster; it must make its control and
continuation benefits affordable.

## 11. Questions for reviewers

Reviewers are asked to answer:

1. Which option best expresses Birnam's floor-versus-malleable-layer boundary?
2. Is an invocation capability meaningfully different from a Frame handle, and
   is that distinction clear enough in Option D?
3. Should Machine alone be sufficient authority, or should every StepMethod call
   carry a distinct activation token?
4. Is direct slot access an acceptable permanent ABI tier?
5. Should core action publication use typed functions or one generic
   `publish(action, operands)` primitive?
6. Which helper failures are compiler/runtime invariants, and which must become
   recoverable typed driver failures?
7. What measured overhead would justify choosing Option C over D?
8. Which aspects of StepMethod execution must be observable or replaceable from
   Birnam on day one, rather than merely reflected and movable later?

The requested review outcome is a preferred option, any unacceptable one-way
commitments, and the minimum evidence needed to accept or reject the current
lean.
