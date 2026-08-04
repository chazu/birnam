# 60C — The StepMethod Calling Contract

> **Status:** Request for comment, revised 2026-08-04. No ABI decision has been
> accepted. The current lean is to reuse the existing Method ABI without adding
> a StepMethod-specific native API.
>
> [60E §9](60e-resumable-registration-and-entry.md) proposes three amendments to
> this record — the driver argument count, microcode sealing, and evidence item
> 8 — which take effect only if this record is accepted with them. They are
> marked inline below.

## 1. Decision under review

A StepMethod should use the already frozen Method function shape:

```c
typedef Oop (*Method)(Oop receiver, const Oop *args, size_t count);
```

The stackless driver gives that existing shape one narrow semantic contract:

- `receiver` is the active Machine Oop;
- `args` is an ordinary borrowed Oop argument buffer;
- the driver supplies a descriptor-declared buffer of length 0 or 1, where a
  declared operand is the resume token
  ([60E §9.1](60e-resumable-registration-and-entry.md)); and
- the result is one canonical StepAction Oop.

The proposal does **not** add any of the following:

- a `StepMethodV1` function-pointer shape;
- a `BirnamStepInvocationV1` record;
- a `BirnamStepApiV1` capability table;
- an invocation lease disguised as an Oop;
- a borrowed Frame pointer or execution window; or
- stackless-specific entries in IdApi.

StepMethod is therefore not a second native calling convention. It is a
semantic role fulfilled by an ordinary Method entry. Registration metadata
distinguishes synchronous Methods from resumable StepMethods and tells the
driver how to interpret the returned Oop.

This record narrows only that calling contract and the regress-breaking Machine
protocol used by generated code. Complete artifact descriptors, compiler
lowering, host drive results, and continuation representation remain separate
decisions.

## 2. Why start from Scheme's machine model

The original Scheme interpreter avoids implementing guest recursion with host
recursion. It models execution as a small register machine with an expression,
environment, continuation link, program counter, and value register. Its
`MLOOP` repeatedly invokes the microinstruction selected by the program counter.
An instruction changes the registers and returns to that loop. A non-tail call
explicitly saves a frame; return restores one.

That design does not pass a versioned capability table to every instruction.
The active machine state is already the instruction's context. The mechanism
needed to proceed is the register protocol plus one outer dispatch loop.

The corresponding Birnam concepts are:

| Scheme interpreter | Birnam stackless runtime |
|---|---|
| register bank | Machine plus its current private Frame |
| program counter | current Frame program counter and StepMethod |
| continuation link | Machine frame chain |
| microinstruction | StepMethod |
| save and restore | CallAction and ReturnAction |
| `MLOOP` | Rust Machine driver |

Scheme's proper-tail-recursion requirement gives the semantic reason for this
shape: a tail call does not create a redundant continuation when the existing
continuation already has the required meaning. Racket's native primitive
interface demonstrates a related implementation pattern: a primitive keeps its
ordinary native calling convention and can return a deferred tail application
to the outer evaluation loop.

Birnam retains StepAction rather than copying Scheme's register mutation
literally. That makes the requested transition an inspectable Birnam object and
provides one uniform result for call, return, suspension, capture, restoration,
and failure. It does not require a second function ABI.

References:

- [Scheme: An Interpreter for Extended Lambda Calculus](https://research.scheme.org/lambda-papers/lambda-papers-scheme-report.html),
  especially sections 4.2, 5.1, and 5.2;
- [R5RS proper tail recursion](https://people.csail.mit.edu/jaffer/r5rs/Proper-tail-recursion.html);
  and
- [Racket BC tail evaluation](https://docs.racket-lang.org/inside/Evaluation.html#%28part._.Tail_.Evaluation%29).

## 3. Architectural constraints

The contract is evaluated against the following commitments:

1. Birnam is a fully malleable live object environment. Language identities and
   protocols should remain inspectable and replaceable.
2. The id floor is independently useful mechanism. Stackless execution must not
   enlarge IdApi merely to make generated code convenient.
3. Source-loaded and binary-loaded methods have the same semantics and enter
   through the same registration and execution contracts.
4. Machine and StepAction are first-class, process-local Birnam Oops.
5. Frame is private execution storage, not an Oop. A FrameView Oop may support
   inspection, but generated code receives neither a Frame nor a Frame pointer.
6. StepAction operands name rooted Frame slots. They do not retain temporary
   argument arrays, Oop pointers, or Frame addresses.
7. A StepMethod may allocate. Oops live across allocation or step return remain
   rooted in traced Frame slots.
8. A resumable language send returns a CallAction or TailCallAction. Generated
   code must not recursively enter an arbitrary target through IdApi.send and
   assume that target will remain synchronous after live replacement.
9. The outer driver directly validates and decodes canonical StepAction objects.
   Sending a language message to interpret the action would recurse through the
   operation the driver is implementing.
10. No Rust panic, C unwind, or malformed native result crosses an unchecked
    boundary.

The governing ownership rule remains:

> Birnam owns the object identity and protocol; Rust temporarily owns the
> low-level storage and primitive operations behind it.

Relevant prior records are:

- [30 — The destination](30-the-destination.md);
- [42 — Object canonical, relational derived](42-object-canonical-relational-derived.md);
- [60A — Execution semantics](60a-stackless-execution-semantics.md);
- [60B — Machine, Frames, and GC](60b-stackless-machine-and-frames.md); and
- the current [IdApi ABI evidence](../../abi/README.md).

## 4. The calling contract

### 4.1 Entry

The driver resolves the current Frame's registered implementation entry and
invokes it through the existing Method carrier:

```c
Oop action = entry(machine, NULL, 0);            /* descriptor declares none */
Oop action = entry(machine, &resume_token, 1);   /* descriptor declares one  */
```

The null pointer is legal because the existing Method ABI permits a null
argument buffer when `count` is zero. The source receiver, source arguments,
locals, temporaries, and return destination are already in the current Frame;
they are not repeated in the native argument buffer. The one declarable operand
is the **resume token**: a SmallInt program counter for the frame being entered,
which spares the step a microcode send to recover it
([60E §9.1](60e-resumable-registration-and-entry.md)).

The entry validates that:

- `receiver` is a live Machine of the expected runtime;
- `count` matches the count its FrameDescriptor declares; and
- the Machine is actively driving the implementation and Frame described by
  the registration record.

Violation returns an AbortAction or another canonical failure Oop selected by
the execution semantics. It must not panic or index Frame storage.

### 4.2 Result

The entry returns exactly one Oop implementing the canonical StepAction
protocol. The driver validates its vtable, payload schema, owning Machine,
generation, drive epoch, and slot operands before changing Machine state.

The Method function type does not itself distinguish a synchronous value from a
StepAction. Registration does, and
[60E](60e-resumable-registration-and-entry.md) defines it: a vtable holds an
ordinary entry thunk, never the StepMethod address, so an ordinary send can
never reach a StepMethod with a non-Machine receiver. Invoking an address under
the wrong semantic tag is malformed-artifact or runtime-corruption failure, just
as invoking any foreign function under the wrong ABI contract would be.

The function shape is not being overloaded with non-Oop control data. Receiver,
arguments, and result retain their ordinary meanings. The additional rule is
only that this registered method receives a Machine and returns a StepAction.

### 4.3 Process IdApi

Generated modules already receive and retain the process-owned IdApi during
module initialization. A StepMethod uses that same IdApi for interning,
small-integer conversion, allocation, and ordinary sends to the primitive-backed
Machine protocol. The Machine driver does not pass another native API to each
invocation.

## 5. Machine as the register protocol

Machine is the explicit semantic context of a StepMethod. Its current private
Frame is analogous to the current register bank and continuation link in the
Scheme interpreter.

Generated code accesses that state through ordinary messages to Machine. The
initial protocol needs operations in these categories:

- inspect the current program counter;
- load an Oop or raw value from a descriptor-validated Frame slot;
- store an Oop or raw value into a descriptor-validated Frame slot;
- publish Call, TailCall, Return, Continue, and Abort actions; and
- later, publish suspension, capture, and restoration actions.

The exact selector names and decomposition are not frozen by this record. S1
must demonstrate the smallest useful protocol before those details are accepted.
For example, S1 may find that action publication belongs on canonical action
prototypes rather than Machine, or that the current program counter need not be
exposed as a separate message.

The protocol is metacircularly visible: Machine, selectors, StepAction
prototypes, methods, and returned actions are all Birnam Oops. The initial
implementations of the irreducible Frame operations are synchronous native
methods installed by the stackless runtime layer. They are not new IdApi fields
or separately negotiated C symbols.

This is the stackless equivalent of Scheme's microcode. It is a deliberately
small regress breaker beneath the visible protocol, not a second general-purpose
runtime API.

## 6. Which sends may occur inside a StepMethod

There are two different operations that both appear as sends at source level:

1. **Machine microcode sends** access the active Frame or publish a StepAction.
   Their initial implementations are synchronous primitive methods. A
   StepMethod may invoke them through IdApi.send and receive their result before
   returning to the driver.
2. **Language sends** invoke arbitrary user-replaceable behavior. A resumable
   StepMethod does not invoke these recursively through IdApi.send. It stores
   the receiver, selector, arguments, and live values in Frame slots and returns
   CallAction or TailCallAction to the driver.

The distinction is semantic and must be visible in compiler lowering. Treating
every Machine micro-operation as a separate resumable language call would make
one source step explode into many driver transitions. Treating every language
send as a synchronous primitive call would preserve the native-stack problem
the design exists to remove.

The synchronous primitive implementation is temporary ownership of mechanism,
not permanent ownership of language policy. Moving a Machine operation into
Birnam later requires preserving a non-recursive regress path; it cannot be
done merely by replacing a primitive with an arbitrary resumable method while a
StepMethod is waiting synchronously for its result.

That limitation should be explicit and small. It is the real malleability
boundary to review, rather than hiding the same boundary inside a native
capability table.

## 7. Illustrative generated code

This example uses local wrappers around the existing IdApi. Those wrappers are
generated-C conveniences, not exported ABI:

```c
static IdApi *id;

static Oop send0(Oop receiver, Oop selector) {
    return id->send(receiver, selector, NULL, 0);
}

static Oop send1(Oop receiver, Oop selector, Oop argument) {
    Oop arguments[] = {argument};
    return id->send(receiver, selector, arguments, 1);
}

static Oop send2(Oop receiver, Oop selector, Oop first, Oop second) {
    Oop arguments[] = {first, second};
    return id->send(receiver, selector, arguments, 2);
}

/* This implementation's descriptor declares one operand, so the driver
   supplies the resume token and the step does not send for it. A descriptor
   declaring none would instead recover the counter through microcode. */
Oop example_step(Oop machine, const Oop *arguments, size_t count) {
    if (count != 1) {
        return send0(machine, selector_abort_invalid_invocation);
    }

    Oop pc = arguments[0];
    if (!id->isSmallInt(pc)) {
        return send0(machine, selector_abort_invalid_pc);
    }
    switch (id->smallIntValue(pc)) {
    case 0:
        return send2(
            machine,
            selector_call_using_descriptor,
            id->mkSmallInt(CALL_SITE_0),
            id->mkSmallInt(PC_AFTER_CALL_0)
        );
    case 1:
        return send1(
            machine,
            selector_return_using_slot,
            id->mkSmallInt(SLOT_RESULT)
        );
    default:
        return send0(machine, selector_abort_invalid_pc);
    }
}
```

The exact Machine selectors and operands are illustrative. In particular, an
immutable call-site descriptor may be preferable to passing several slot
indices on every transition. The point of the example is the boundary:

- the function has the existing Method shape;
- every native argument and result is an Oop;
- only the existing IdApi is used; and
- control returns to the outer driver as a StepAction Oop.

## 8. Driver responsibilities

The Rust Machine driver remains a small irreducible loop:

1. validate the Machine state and current Frame;
2. resolve the registered StepMethod entry from the Frame's implementation
   identity;
3. invoke that entry through the Method ABI with Machine and the
   descriptor-declared argument buffer;
4. receive one Oop;
5. directly validate and decode the canonical StepAction payload;
6. update, push, pop, detach, or restore private Frame records;
7. either stop with a host-visible drive result or repeat from step 1.

The driver does not send `execute`, `apply`, or another ordinary message to the
returned action. That direct decoding is the bottom of dispatch and therefore
the intentional regress breaker.

The driver also owns adaptation of existing synchronous Methods. When a
CallAction resolves to a synchronous Method, it may invoke that Method using the
ordinary receiver and argument buffer assembled from rooted Frame slots, store
the returned value, and continue. When it resolves to a StepMethod, it pushes or
replaces a resumable Frame instead.

## 9. Frame access, lifetime, and trust

No lease is passed to StepMethod. Machine validates that a drive is active and
selects its current Frame. Every Machine microcode method validates slot kinds,
bounds, implementation identity, and current drive epoch before touching
storage.

A lease would improve diagnostics for accidental stale use, but it would not
create a security boundary. A loaded native artifact already executes trusted
machine code and can retain Oops, call arbitrary symbols, or corrupt process
memory. The design should not permanently enlarge the ABI to simulate memory
safety against code that already has native authority.

Machine and action epoch checks still catch ordinary runtime mistakes:

- invoking a StepMethod when its Machine is not driving;
- returning an action from another Machine;
- returning an action issued in an earlier epoch;
- naming a slot of the wrong kind; and
- naming a call site or program counter outside the registered descriptor.

If later evidence demonstrates that invocation identity is independently useful
to Birnam programs, it should become a genuine process-local Birnam object with
identity and protocol. It should not be introduced preemptively as an opaque
native token or an object-shaped lease with no language meaning.

## 10. GC and rooting

The calling contract does not impose a no-allocation or no-GC region.

- Oops representing receiver, arguments, locals, selectors, temporaries, and
  results remain in descriptor-traced Frame slots while live.
- A Machine store primitive must root its incoming Oop before any operation that
  can collect.
- Generated code must store a newly produced Oop in a traced slot before another
  operation that may allocate unless the ordinary Method/send rooting contract
  already covers that dynamic extent.
- Temporary C argument arrays are borrowed only for the synchronous IdApi.send
  that receives them. They never appear in StepAction or Frame state.
- Returned StepAction Oops are traced through Machine-owned scratch roots or
  another root established before generated code can allocate them.

S1 must force collection at each Machine protocol boundary. Passing a normal
execution test is not evidence that the roots are correct.

## 11. What is and is not frozen

If accepted, this record freezes only:

- StepMethod uses the existing Method function shape;
- the driver invokes it with Machine as receiver and an ordinary argument
  buffer;
- the driver contract uses a descriptor-declared buffer of length 0 or 1, per
  the 60E §9.1 amendment;
- registration distinguishes synchronous Method from StepMethod; and
- StepMethod returns one canonical StepAction Oop.

It does not freeze:

- Frame layout or arena representation;
- Machine payload layout;
- Machine selector names;
- the number or decomposition of Machine microcode methods;
- StepAction allocation versus per-Machine scratch reuse;
- call-site descriptor layout;
- compiler IR or generated-C helper names;
- a direct-slot optimized tier; or
- suspension and continuation policy beyond the semantics already accepted in
  60A and 60B.

Reusing Method is conservative because that ABI is already frozen. The new
one-way commitment is the semantic registration tag and returned-StepAction
contract, not another native layout.

## 12. Benefits and costs

### Benefits

- IdApi and the independently useful id floor do not grow.
- Existing module initialization, loader machinery, function-pointer storage,
  and generated-C scaffolding remain applicable.
- Machine and StepAction are the only semantic objects at the call boundary.
- Frame representation, GC barriers, and arena strategy remain private.
- The runtime resembles the well-understood Scheme register-machine and
  trampoline model.
- Metacircular visibility is concentrated in ordinary Birnam objects and
  protocols rather than mirrored in a second C interface.
- Future performance work can optimize an observed bottleneck without making
  the optimization the initial semantic contract.

### Costs

- Machine microcode messages pay normal dispatch overhead unless later compiler
  evidence justifies a compatible optimization.
- Some primitive-backed Machine operations must remain synchronous at the
  regress boundary.
- Machine's current Frame is dynamic state rather than an explicit invocation
  capability.
- Method and StepMethod share a physical function-pointer type, so artifact
  validation and registration tags must be correct.
- Native code is trusted; Machine and epoch checks improve correctness but do
  not sandbox a malicious artifact.

These costs are visible and testable. None requires a second permanent API
before S1 demonstrates the execution path.

## 13. Deferred and rejected alternatives

### 13.1 Method shape plus lease and slot helper

The former Option A reused Method physically but changed `args` into a control
array containing an ephemeral lease and introduced a separate slot helper.
That design is not the current proposal. It made ordinary arguments carry
non-language control meaning and created object-shaped identity for a value
that intentionally had no Birnam protocol.

If an invocation object becomes semantically useful later, it should be a real
Birnam object passed as an ordinary argument.

### 13.2 Dedicated Machine-only function shape

The former Option B used `Oop (*StepMethod)(Oop machine)`. It is conceptually
close to the current proposal but physically unnecessary. The existing Method
shape already expresses a Machine receiver and a declared argument buffer,
avoids another function-pointer type, and preserves established loader
behavior — and a Machine-only shape could not carry the resume token of §4.1 at
all.

### 13.3 Borrowed execution window

The former Option C exposed direct slot banks and reusable action cells. It
remains a possible optimized execution tier only if measurements show that
ordinary Machine protocol sends violate an accepted overhead budget. It must
not be selected speculatively because it freezes Frame contiguity, pointer
lifetime, write-barrier behavior, and action payload representation.

Any future fast tier must preserve the Method-shaped semantic entry or be
registered as an explicitly different optional optimization. Failure to support
that tier must fall back to the ordinary Machine protocol, not reject otherwise
valid source or artifacts.

### 13.4 Invocation record and capability table

The former Option D introduced a borrowed invocation record and versioned table
of frame and action operations. It is rejected as the current direction because
it creates a second permanent native substrate beside IdApi before the need for
one is demonstrated. The table's functions are less visible and less malleable
than ordinary Machine protocol methods, while solving lifetime and authority
problems that trusted native code can already bypass.

If a future non-native or sandboxed artifact format changes the trust model,
that format can define its own checked boundary. It should not determine the
initial trusted-native StepMethod ABI.

### 13.5 Mutate Machine and return no action

The original Scheme microinstructions mutate the program counter and return to
`MLOOP`. Birnam could copy that literally and use `nil` or Machine as the return
value. This is not preferred because StepAction already gives control transfer
Birnam identity, protocol, inspection, and a uniform extension point for
suspension and continuations. Returning a StepAction is a small semantic cost
with substantial metacircular value.

## 14. Evidence required before acceptance

S1 should implement one hand-written StepMethod using only:

- the existing Method ABI;
- the process-owned IdApi;
- Machine protocol sends; and
- canonical StepAction Oops.

The probe must demonstrate:

1. at least one million tail transitions without native stack growth;
2. a resumable CallAction and ReturnAction pair;
3. adaptation to an existing synchronous Method;
4. forced GC with live Oops in every relevant Frame slot;
5. rejection of a driver argument count the FrameDescriptor does not declare;
6. rejection of wrong-Machine, stale-epoch, malformed-action, invalid-PC, and
   wrong-slot-kind cases;
7. identical registration from live and AOT artifacts; and
8. generated C that uses no stackless-specific IdApi fields, and no
   stackless-specific native symbol other than the entry shim
   ([60E §9.3](60e-resumable-registration-and-entry.md)).

Measure:

- nanoseconds per Continue transition;
- nanoseconds per Call/Return pair;
- Machine protocol sends per source-level step;
- time spent in ordinary dispatch versus driver validation; and
- allocation caused by action construction.

Only if those measurements fail an agreed budget should S1 prototype a direct
slot or specialized action-publication optimization. That budget is stated in
[60E §8](60e-resumable-registration-and-entry.md), against the recorded send
constant. The baseline semantics remain the Method-shaped contract.

## 15. Questions for reviewers

Review should now focus on the smaller set of real decisions. Questions 1, 4, and
6 are answered by [60E](60e-resumable-registration-and-entry.md) and are recorded
here as resolved rather than deleted:

1. **Resolved (60E §9.1).** Zero arguments is not frozen; the driver supplies a
   descriptor-declared buffer of length 0 or 1, so a resume token need not cost
   a microcode send.
2. What is the minimum Machine microcode protocol needed by the S1 hand-written
   method?
3. Should action publication be protocol on Machine, canonical StepAction
   prototypes, or immutable call-site descriptors?
4. **Resolved (60E §9.2).** Microcode selectors are installed on Machine's own
   private vtable and sealed against overwrite, so an arbitrary resumable method
   cannot displace a primitive the driver depends on.
5. Should the driver invoke the registered Method pointer directly, or should
   current implementation selection itself be an object-visible dispatch step?
6. **Resolved (60E §8).** The provisional budget is stated against the recorded
   send constant; exceeding it is the trigger for the §13.3 tier.
7. Which parts of Machine policy must be replaceable on day one, and which
   primitive storage operations are acceptable under the governing ownership
   rule?

The requested review outcome is either acceptance of the Method-shaped baseline
or one concrete semantic requirement that the existing Method ABI cannot
express. Convenience for native implementation is not, by itself, sufficient
reason to grow the language floor.
