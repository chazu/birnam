# 60F — Resumable Compiler Lowering

> **Status:** S2 pre-implementation decision record. The migration unit,
> all-or-nothing eligibility rule, evaluation and lookup order, source-send
> boundary, CFG/PC model, Oop liveness, descriptor-v2 root maps, deterministic
> slot coloring, edge-transfer semantics, and call-staging rules were accepted
> 2026-08-07. The exact C/blob carrier, Design 49 migration-change projection,
> and durable resumable image tag remain open and still gate S2 implementation.
>
> This record lowers the accepted execution semantics of
> [60A](60a-stackless-execution-semantics.md), the private Frame model of
> [60B](60b-stackless-machine-and-frames.md), the Method-shaped StepMethod
> contract of [60C](60c-stepmethod-calling-abi-options.md), and the registration
> seam of [60E](60e-resumable-registration-and-entry.md) into a compiler
> contract. It does not add suspension, scheduling, public continuation syntax,
> static dispatch, or a new IdApi field.

## 1. Purpose and current seam

The current Rust compiler projects the parser CST into `Expr`, `Statement`, and
`Block` trees, then recursively emits C. A nested source send becomes a nested
`birnam_send` expression. C activations therefore retain the receiver,
arguments, locals, result destination, and native return address for the whole
send chain.

S2 inserts an explicit lowering boundary:

```text
CST
  -> source-shaped Expr / Statement / Block
  -> resumable control-flow IR
       logical Oop values
       basic blocks and program counters
       Call / TailCall / Return terminators
       liveness and source locations
  -> physical Frame layout
       colored Oop slots
       per-PC root maps
       per-edge transfer plans
       immutable call-site descriptors
  -> emitted StepMethod C + descriptor + thunk + registration operation
```

The source-shaped tree remains useful for synchronous emission and diagnostics.
The resumable IR is a separate representation; the C emitter must not infer
program counters, liveness, or slot ownership recursively while spelling C.
The IR is compiler-owned data above the floor, not a new IdApi or Runtime ABI.
Another live-replaceable compiler may build a different IR or coloring and still
emit a valid versioned descriptor satisfying this contract.

## 2. Accepted S2 decisions

| Area | Decision |
|---|---|
| Migration unit | One named method implementation or one block-body implementation. Migration is not a property of a selector, vtable, object, or module. |
| Selection | Resumable lowering is an explicit compiler disposition supplied above the runtime floor. It adds no source syntax and no IdApi field. |
| Eligibility | Selection is all-or-nothing for that implementation. A selected unsupported body is a compile error; it never silently falls back to synchronous emission. Unselected bodies retain the existing synchronous compiler path. |
| Evaluation | Evaluate the receiver once, then arguments once from left to right. Only after all operands are ready does the driver perform fresh dynamic lookup. |
| Send boundary | Every ordinary source send in a migrated implementation terminates a step as Call or TailCall. The compiler never predicts that the current target will remain synchronous. |
| Straight-line work | Literal construction, local movement, block construction, and other bounded work may share a step when the rooting rules remain satisfied. No step contains an unbounded source loop or recursive send chain. |
| Values | The lowering IR names logical Oop values independently of physical Frame slots. Shadowed lexical bindings have distinct identities. |
| Liveness | The compiler computes liveness over the complete CFG and treats every allocation, GC-capable helper, late-bound Machine send, synchronous Method call, and return to the driver as a safepoint. |
| Frame roots | S2 descriptor layout v2 carries a root bitmap for every PC. A map covers the union of Oops that can be live at any safepoint while that PC's step executes. |
| Slot allocation | S2 deterministically colors interfering logical Oop values into physical Oop slots. One-slot-per-value is not the S2 artifact contract. |
| Edge transfer | Every control edge describes which physical values survive, which slots are cleared, and which parallel copies or pending result initialization are required. The frozen publishers install their PC/action state first; a short Runtime-owned pending-transition state preserves predecessor roots until the driver validates and commits the edge. Bitmap differences alone are insufficient. |
| Calls | One reusable contiguous call-staging window satisfies the frozen `argument_start` / `argument_count` action shape. The allocator may safely coalesce producers with staging slots. |
| Source maps | Source identity belongs to logical values, PCs, and call sites. A per-PC location map relates a live logical value to its current physical slot. |
| Malleability | Lowering does not seal selectors, cache targets across sends, bypass metacircular lookup, or rewrite active Frames after installation. |
| Reproducibility | Live and AOT compilation use the same lowering and descriptor serialization path. Stable input produces byte-identical canonical descriptor data. |

## 3. Migration unit and eligibility

### 3.1 Per-implementation disposition

Resumability is a property of an installed implementation generation. It is not
part of the selector's language identity. The same selector may name a
synchronous Method in one generation, a resumable entry in the next, and a
synchronous Method again later.

The compilation request supplies one of two dispositions:

```text
Synchronous
ResumableRequired
```

The exact Rust and Birnam orchestration surface carrying that request is not
frozen here. It remains above the IdApi floor and must be represented in
artifact provenance so live and AOT compilation cannot disagree about which
implementation was requested.

`ResumableRequired` first runs an eligibility pass. If any construct in that
implementation is outside the accepted S2 subset, compilation reports a kinded
diagnostic naming the construct and source span. It must not emit synchronous C
for the selected implementation, partially migrate it, or publish a thunk whose
body takes the old recursive path.

Ordinary compilation remains `Synchronous` until an explicit migration policy
selects an implementation. S2 therefore does not silently change all currently
lowerable code.

### 3.2 Blocks are separate implementations

A block literal is created by its enclosing implementation, but the block body
has its own implementation identity, StepMethod, FrameDescriptor, thunk, and
registration. Selecting an enclosing method does not make the block body part
of the enclosing Frame.

Selection may migrate the enclosing method and block body together, or select
either implementation independently. A resumable block reached through an
ordinary `value` / `value:` send follows 60E's thunk path; a migrated send to
that block resolves and pushes its Frame directly. Captures remain ordinary
Oop fields of the block object and are copied into the block Frame's declared
entry slots when invoked.

The S2 block proof selects both the creating implementation and block body. It
must not pass merely because the block stayed synchronous.

## 4. Semantic preservation and late binding

Resumable and synchronous compilation of the same accepted source must preserve:

- receiver and argument evaluation count and order;
- ordinary result and ErrorVt values;
- selector-determined arity;
- inheritance, DNU, and custom metacircular lookup;
- lookup and mutation visibility at each send;
- global and Store reads at their original execution point; and
- block capture identity and lexical binding behavior.

Lowering may change native-stack consumption, Frame allocation, diagnostic
telemetry, and execution cost. Those are the intended implementation changes.

The compiler must not put a target Method, ResumableEntry, defining vtable, or
lookup result into a call-site descriptor. A call site contains operand slots,
arity, source identity, successor PC, and action kind. The driver performs
60E's current `resolve-and-pin` after the operands are evaluated.

Consequently, if receiver or argument evaluation redefines the outer selector
or changes metacircular lookup policy, the outer send observes that new state.
An active Frame keeps its captured StepMethod and descriptor, while its next
Call or TailCall resolves current dispatch exactly as 60E §7.1 requires.

Machine microcode also remains late-bound. Generated StepMethods use the
accepted messages from 60B §1. An optimizer may remove neither those semantic
sends nor a source send unless it proves the same mutation, lookup, failure,
and reflection behavior under the ordinary Birnam protocol. S2 defines no such
optimization tier.

## 5. Evaluation and lookup order

For an ordinary source send:

```birnam
[[makeReceiver] foo: [makeFirst] bar: [makeSecond]]
```

the semantic order is:

```text
1. evaluate makeReceiver exactly once
2. evaluate makeFirst exactly once
3. evaluate makeSecond exactly once
4. perform fresh lookup of foo:bar:
5. invoke the selected synchronous target or push the selected resumable Frame
```

Nested receiver and argument expressions follow the same rule recursively.
Message-chain and `=>` lowering retain their already accepted source order.
Selector interning or descriptor lookup is compiler/runtime mechanism and may
be prepared earlier only when doing so has no Birnam-visible effect; method
resolution may not.

The lowerer materializes each completed operand in a rooted logical value before
starting the next operand when another safepoint can intervene. The call action
is not published until receiver, selector, and every argument staging slot hold
their final values.

The current synchronous C emitter nests receiver and argument expressions in a
single C call expression. C evaluation order is not the Birnam contract. Before
S2 compares synchronous and resumable behavior, the synchronous path for the
accepted subset must also sequence receiver and arguments into explicit
temporaries in the order above. TCC and AOT therefore share one language order
even when their C compilers would otherwise choose differently. This may change
generated C and artifact identity; it clarifies previously backend-dependent
behavior rather than granting S2 a different source semantics.

## 6. Resumable control-flow IR

### 6.1 Basic blocks and PCs

One StepMethod implements one source implementation. Its descriptor numbers a
dense PC range beginning at zero. PC assignment is deterministic for a compiler
lowering version; source order and stable IR value IDs, never hash-map
iteration, break ties.

A basic block contains bounded operations and ends in exactly one terminator:

```text
Call(call_site, successor_pc)
TailCall(call_site)
Return(value)
Continue(successor_pc)
Abort(reason)
```

S2-generated source sends always use Call or TailCall. Continue is available
for a bounded non-send split introduced by the compiler, not as a way to hide
an unbounded loop inside one StepMethod invocation. Abort represents a compiler-
generated checked failure path, not general language conditions.

The entry PC is zero. Each non-tail Call names the PC at which the caller waits
and later resumes. A tail-position source send becomes TailCall regardless of
the currently installed target. If it resolves to a synchronous Method, the
driver performs 60A's Call-plus-Return degradation and increments the accepted
diagnostic.

### 6.2 Every source send crosses the driver

The lowerer does not classify a source send as synchronously safe. Each source
send returns control to the driver, which resolves one of:

```text
pinned ResumableEntry  -> push or replace a Frame
protected Method       -> invoke synchronously and store its result
unbound                -> ordinary DNU redispatch
```

This is required even when the current image happens to install a synchronous
primitive for that selector. Static prediction would make later redefinition
or custom lookup observably ineffective.

### 6.3 Logical values

Every evaluated receiver, argument, literal result, lexical binding version,
send result, block object, and compiler temporary that crosses a safepoint has
a stable logical value ID in the IR. IDs are local to one implementation and
are assigned deterministically from source structure plus deterministic
compiler-created order.

Assignments create new logical values. Shadowed source names and successive
definitions of the same local are not conflated. Source maps retain the lexical
binding identity separately from its current logical value version.

S2 has one physical slot class: Oop. Raw compiler-visible slots remain deferred
under 60B. Non-Oop PC numbers, descriptor indices, masks, and implementation
identities live in descriptor or driver state, not mixed into Oop slots.

## 7. Liveness and safepoints

### 7.1 Analysis

The compiler computes live-in and live-out sets to a fixed point over the CFG.
A logical value is live when a future path can read it before another definition
of that value. Calls include the successor's needs as well as receiver,
selector, argument, and result-destination requirements.

At minimum, each of these is a safepoint for Frame-root purposes:

- Birnam heap allocation;
- any ordinary Machine microcode send;
- any other IdApi send;
- a synchronous Method invocation by the driver;
- a helper documented as GC-capable;
- return from StepMethod to the driver; and
- any callback into host, foreign, or metacircular code.

An Oop live across a safepoint must be in a descriptor-rooted Frame slot or in a
driver-owned scoped root explicitly covered by 60B. A generated C local alone
is never the proof, even while conservative native-stack scanning remains.

### 7.2 PC root maps are step-safepoint unions

Descriptor layout v2 has one immutable root bitmap per PC. It is not merely the
block's live-in set. It is the union of every logical Oop that can be live at
any safepoint from entry to that PC's StepMethod return.

This distinction is mandatory. A step may create and store a value, then invoke
late-bound Machine microcode which allocates before the StepMethod returns. The
new value must already be covered by the current PC's bitmap.

The compiler verifies after coloring that every safepoint-live logical value's
physical slot is present in the current bitmap. Descriptor validation rejects:

- a missing PC map;
- a bitmap with the wrong word count or nonzero padding bits;
- a bit outside the physical slot range;
- a root bit naming a non-Oop slot in a later mixed-kind layout; or
- an edge whose live successor values cannot be established by its transfer
  plan.

A layout-v2 Frame keeps a private **root-state PC** in addition to the semantic
program counter. They normally match. The accepted `continueAt:` and
`callSite:` protocol installs the successor program counter before returning its
StepAction, so during that short publication-to-driver interval the Frame also
holds a pending transition and continues tracing the predecessor root-state PC.
Only a successful driver commit advances the root-state PC and clears the
pending transition. Generated StepMethod entry is legal only when no transition
is pending and the two PCs match.

S1 descriptor layout v1 remains valid for hand-written probes and treats every
declared Oop slot as live for the Frame's lifetime. S2 compiler output uses
layout v2 and may not claim v1 while depending on PC-specific roots.

## 8. Deterministic slot coloring

### 8.1 Interference

The allocator constructs an interference graph over logical Oop values. Two
values interfere when any valid execution requires both to be present at the
same safepoint, action publication, edge transfer, or source-visible inspection
point. Interfering values must receive different physical slots.

The graph also includes constraints introduced by:

- entry receiver and arguments;
- block captures;
- call receiver, selector, and argument staging;
- a Call result destination while its arguments remain needed by the driver;
- edge parallel-copy sources and destinations; and
- any value retained for the accepted source-location contract.

Values with disjoint lifetimes may share a slot. A physical slot is not itself
a source binding, object identity, or stable cross-compilation location.

### 8.2 Determinism and versioning

Coloring need not find the minimum possible number of slots. It must:

- use a deterministic graph traversal and deterministic tie-breaking;
- choose the lowest permitted slot when choices are otherwise equal;
- avoid dependence on allocation addresses, hash randomization, or compilation
  mode;
- run through the same implementation for live and AOT artifacts; and
- include its lowering/layout version in canonical descriptor identity.

A later compiler may improve coloring and produce a different descriptor
identity without changing language semantics. Mapped and rebuilt paths for the
same artifact must nevertheless consume the same serialized descriptor rather
than independently recoloring it.

Runtime validation is invariant-based: it does not require one privileged Rust
coloring heuristic. Compiler/version identity and canonical descriptor bytes
make a particular artifact reproducible while preserving Birnam's ability to
replace its compiler and emit a different valid layout.

### 8.3 Entry locations

The descriptor identifies the initial receiver slot and initial contiguous
argument range. Those logical values are preassigned to the required entry
locations. Once an entry value is dead, another noninterfering logical value may
reuse its physical slot.

Initial Frame construction populates the entry receiver and arguments and sets
every other slot that may be observed by the entry PC to `nil` before the Frame
becomes visible to GC or generated code.

## 9. Call-staging window

StepAction schema v1 and 60B's call-site descriptor use a receiver slot,
selector slot, and contiguous `argument_start .. argument_start + count` range.
General coloring does not naturally assign arbitrary argument values consecutive
colors.

Each implementation therefore reserves one reusable call-staging window:

```text
call receiver
call selector
call argument 0
...
call argument (maximum source-send arity - 1)
```

Zero-argument methods still have receiver and selector staging slots. The
argument portion is sized to the maximum arity of a source send in that
implementation. Every call site uses the prefix matching its arity.

Staging locations are Oop slots and participate in the current root-state map.
They are reused only after the driver has copied and rooted the previous
action's operands. A Call result slot may not overlap the active argument
prefix, preserving 60B's existing validation rule.

The allocator may coalesce a producer's logical value with its staging location
when the interference graph proves that safe. Otherwise the generated step
performs a checked move through Machine microcode. Coalescing may remove moves;
it may not change evaluation order, lookup time, or the rooted-state invariant.

An implementation with no source sends has no call-staging window.

## 10. Edge transfer and slot ownership

### 10.1 Why root-map differences are insufficient

Suppose physical slot 4 holds logical value `a` at PC 2 and, after coloring,
will hold unrelated value `b` at PC 3. Slot 4 may be rooted in both PC maps.
Comparing only those maps would leave `a` in place even though its logical
ownership ended. If `a` were instead omitted from both maps while its pointer
remained readable through FrameView or microcode, it could become a dangling
Oop.

Every control edge therefore has an immutable transfer plan tied to its source
PC, terminator/call-site identity, and successor PC.

### 10.2 Transfer plan

A transfer plan describes:

- physical slots whose logical value is preserved unchanged;
- parallel copies required for successor logical values;
- slots cleared to canonical `nil` because ownership ended or changed;
- for Call, the result slot initialized to rooted `nil` while the caller waits;
  and
- the successor root bitmap expected after the transition.

Parallel copies have simultaneous semantics. Their implementation must preserve
cycles using a driver-owned rooted scratch Oop or a validated spare slot; it may
not sequentially overwrite a source that a later copy still needs. S2's initial
straight-line subset may emit no cyclic copies, but descriptor validation and
the transfer executor must not encode an order-dependent format that prevents
future CFG joins.

Every successor-rooted slot is established by exactly one of:

- preservation of the same logical value in the same slot;
- a transfer copy;
- canonical `nil` initialization for a value not yet produced; or
- the driver's Call-result write before the successor StepMethod runs.

Physical slots not valid in the successor are cleared to `nil` before the
Frame can be inspected or traced in that state. This keeps physical FrameView
access memory-safe even though source-level debugger mutation remains deferred.

### 10.3 Action publication and pending transitions

60B's frozen publishers already have mutation semantics:

- `continueAt:` validates and installs its next program counter before
  returning ContinueAction;
- `callSite:` validates its call-site descriptor and installs its continuation
  PC before returning CallAction; and
- TailCall, Return, and Abort publishers validate the current Frame before
  returning their actions.

Layout v2 preserves those semantics. A canonical publisher first requires that
no other action transition is pending, validates the source PC and exact edge or
terminal operation, and prepares the canonical scratch action and pending record
without mutating the Frame. Failure through that preparation point leaves the
Frame unchanged. Continue and Call then atomically record the pending transition
and install the successor semantic PC as the frozen protocol requires, but do
not change the root-state PC or reuse/clear predecessor slots. TailCall, Return,
and Abort record terminal pending state without a successor PC.

While pending:

- tracing uses the predecessor root-state map;
- Call staging and Return result slots therefore remain rooted and readable by
  the driver;
- checked Frame mutation, another action publication, and StepMethod re-entry
  fail with `frameTransitionPending` before touching storage; and
- FrameView may report the kinded pending state but cannot read physical slot
  contents until commit or Machine failure resolves it.

The StepMethod must return the action it just published. If it instead returns
a malformed, stale, or different action, the driver moves the Machine to Failed.
The already installed semantic PC is not rolled back, matching the accepted rule
that valid checked microcode mutation is not transactional. The predecessor root
state remains traced until failed-Machine teardown releases or clears it safely.

This short pending state is Runtime mechanism, not a public continuation or a
new StepAction field. It is necessary because action schema v1 carries slot
references rather than copied operand Oops.

### 10.4 Driver ordering

For Call, the driver performs this order:

1. validate the action against the current Machine, epoch, descriptor, pending
   source PC, and exact call site;
2. copy receiver, selector, and argument Oops from the still-predecessor staging
   slots and establish driver-owned scoped roots;
3. validate the already selected edge transfer plan without mutating the Frame;
4. execute parallel copies and clear/reinitialize slots;
5. advance the root-state PC to the already installed continuation PC and clear
   the pending transition;
6. perform fresh protected resolve-and-pin;
7. push the resumable callee or invoke the protected synchronous Method;
8. write the returned Oop to the declared result slot before resuming the
   successor StepMethod.

If validation or transfer preparation fails, no Frame mutation occurs. Once a
validated transfer begins it is a Runtime-owned non-allocating operation. A
failure after the transition follows 60A's driver-failure rules; it does not
pretend the predecessor state remains resumable.

TailCall validates its pending terminal action, roots and copies its operands,
then resolves and replaces or synchronously completes the current Frame under
60A's existing rules. Return roots the result before popping its Frame.
Continue validates the matching pending action, commits its edge transfer and
root-state PC, and performs no resolution.

GC at every point observes either the complete predecessor root state,
driver-owned operand roots during transition, or the complete successor root
state. It never observes a half-applied copy plan.

## 11. Source maps and inspection

Source maps attach source meaning to logical IR, not permanently to physical
slots. For each implementation they record at least:

- source span for every PC;
- source span and selector spelling for every call site;
- lexical binding identity, including distinct shadowed bindings;
- logical value definitions and uses relevant to diagnostics; and
- at each PC, the physical slot holding each live inspectable logical value, or
  an explicit `unavailable` state.

A constant may be represented as a source-map constant only if inspection can
reconstruct the same Oop without allocation or semantic lookup. Otherwise it
occupies a rooted slot when live.

FrameView remains a read-only view of physical execution state. It must never
return an untraced stale pointer: edge transfer clears dead/reassigned slots,
and physical slot reads return `frameTransitionPending` during the short
publication-to-commit interval.
Source-aware inspection consults the pinned descriptor and source map for that
Frame generation. It does not reinterpret an old Frame using the currently
compiled method's coloring.

Debugger-grade mutation remains deferred. Nothing in S2 promises that writing a
source variable through an inspector is equivalent to `oopAt:put:` on whichever
slot currently carries it.

## 12. Blocks and captures

Capture discovery is lexical and deterministic. Captures are ordered by stable
enclosing binding identity, not first hash-table encounter. The block object's
payload remains all Oops under its existing traced layout.

Creating a block evaluates and stores each capture once in that canonical order.
Every captured Oop remains rooted during block allocation. Invoking a resumable
block copies its receiver, invocation arguments, and captures into the entry
locations declared by the block body's descriptor.

The block body's liveness, coloring, root maps, call window, source map, and
registration are independent of the creating method's. Its vtable never holds
the StepMethod directly: direct `birnam_method` installation publishes the
entry thunk and its emitted registration as required by 60E.

The durable marker by which mapped-image validation discovers resumable block
vtables is still open in §16. S2 image integration cannot be accepted until that
marker and restored-registration check are implemented.

## 13. Artifact and registration output

One successfully lowered implementation produces one indivisible artifact
description containing:

- the StepMethod symbol;
- the ordinary entry-thunk symbol or assigned unique pooled thunk;
- canonical descriptor layout version and bytes;
- entry receiver/argument locations;
- call-site table and call-window size;
- PC root maps and edge-transfer plans;
- source-map identity and contents;
- implementation and code-generation identities;
- selector-determined language arity and driver argument count; and
- the registration operation emitted before binding publication.

Registration copies or interns every immutable descriptor component before
publication. No bitmap, transfer plan, source-map slice, or call-site table may
remain borrowed from temporary initializer memory.

Live and AOT compilation must call the same canonical descriptor serializer.
Mapped-image boot consumes the serialized descriptor and re-registers the thunk;
it does not rerun liveness or coloring. Rebuilt boot recompiles source and must
produce semantically identical registration. The exact byte carrier and durable
cross-check remain open in §16.

## 14. Diagnostics and failure rules

The compiler reports at least these kinded failures before publication:

- selected implementation contains an unsupported construct;
- CFG construction finds an invalid or unreachable continuation;
- a logical value can reach a safepoint without a root location;
- interference coloring assigns one slot to simultaneously live values;
- a call cannot populate its contiguous staging prefix;
- an edge cannot establish every successor-rooted value;
- an action does not match the exact pending source PC, edge, and publisher;
- source-map location data disagrees with coloring;
- live and AOT canonical descriptors disagree for the same artifact input; or
- the physical descriptor carrier cannot represent the accepted semantic data.

No such failure installs a synchronous substitute, publishes a thunk, mutates a
vtable binding, or leaves an unpublished registration behind.

Runtime validation rejects malformed descriptor data before publication. A
malformed action or transfer at execution time follows the accepted Machine
failure taxonomy and cannot become an ordinary thunk result.

## 15. S2 acceptance evidence

### 15.1 Lowering and semantic equivalence

Tests must prove:

1. selected eligible methods and selected block bodies emit resumable artifacts;
2. selected unsupported bodies fail explicitly while the same unselected source
   still uses the synchronous compiler;
3. receiver evaluation occurs once, arguments occur once left to right, and
   lookup occurs afterward in both explicitly sequenced synchronous C and
   resumable output;
4. a redefinition or lookup-policy change during argument evaluation is visible
   to the outer send;
5. every source send increments a generated Call/TailCall probe, including a
   send whose current target is synchronous;
6. inherited, DNU, and custom lookup behavior matches synchronous compilation;
7. tail position is stable across synchronous-to-resumable redefinition; and
8. ordinary callers observe the same result or ErrorVt value and no Machine.

### 15.2 Liveness, coloring, and GC

Tests must prove:

9. two interfering values never share a slot;
10. at least two noninterfering values do share a slot, proving coloring is live;
11. stable input produces byte-identical coloring, masks, transfers, and source
    locations in live and AOT modes;
12. every safepoint-live value's colored slot appears in its PC root map;
13. a slot whose logical owner changes while its root bit remains set is cleared
    or transferred correctly;
14. Call/Continue publication installs the frozen successor PC while tracing
    remains on the predecessor root-state map until driver commit;
15. mismatched action return and reentrant Frame mutation while pending fail
    without losing predecessor roots;
16. cyclic parallel-copy execution preserves all values;
17. dead and reassigned slots read as `nil`, never stale untraced Oops;
18. forced collection at step entry, after each generated store, inside
    late-bound microcode, during synchronous leaf invocation, while a caller is
    parked, and immediately after transition preserves exactly the required
    values;
19. a Call result slot is rooted as `nil` while the caller is parked and
    contains the result before successor execution; and
20. coloring does not change a captured block's values or lifetime.

Property tests should generate small CFGs, compare compiler liveness with a
reference data-flow solver, and verify coloring and edge transfers against an
uncolored logical interpreter. Focused fixtures still provide readable failures
for every rule above.

### 15.3 Calls, stack depth, and registration

Tests must also satisfy 60E §14's S2 items:

21. migrated-to-migrated Call, inheritance, and custom lookup push Frames
    directly with no thunk invocation;
22. a resumable block executes through direct block-vtable registration;
23. live and AOT registrations have identical descriptor, arity, implementation,
    root-map, coloring, and transfer identities;
24. TailCall to a synchronous Method increments `tailCallToSynchronous`, while
    the fully migrated deep-recursion proof remains zero;
25. two mutually recursive migrated implementations run at least one million
    calls with bounded native-stack movement;
26. the reusable call window handles zero through maximum tested arity and never
    overlaps an active Call result;
27. mapped and rebuilt image paths re-register equivalent named and block
    implementations once §16's durable tag is settled; and
28. Call/Return, ordinary-entry, per-step microcode, Frame-slot high water,
    boundary-depth, native-stack depth, and descriptor-size measurements are
    published against 60E §11's same-run baseline.

The canonical repository gate, native-prefix smoke, cold-archive smoke,
relocation smoke, frozen IdApi fixtures, and process-local image rejection tests
remain mandatory.

## 16. Remaining decisions before S2 implementation

The lowering, liveness, and physical-layout semantics above are accepted. These
integration decisions remain open and still prevent compiler implementation
from publishing S2 artifacts:

1. **Physical descriptor carrier.** Freeze the C structs/blob sections,
   alignment, integer widths, mask encoding, edge-copy encoding, source-map
   reference, size/version checks, ownership transfer, and canonical byte
   serialization used by the registration shim. Descriptor layout v2 must be
   distinguishable from S1 layout v1 without changing StepAction schema v1.
2. **Migration as a Design 49 change event.** Decide what
   `deriveImageMethodTargets` and `managedMethodChanged:on:` observe when source,
   selector, and language behavior are unchanged but the installed carrier
   switches between synchronous and resumable. The successor generation must
   reconcile the change rather than report false drift or omit it.
3. **Durable resumable tag.** Choose the self-describing image/artifact marker
   used to enumerate every resumable named method and block vtable, verify that
   registration completed after initializer execution, and report
   `resumableEntryUnrestored` before user entry when it did not.
4. **Selection carrier.** Freeze how live and AOT orchestration request
   `ResumableRequired` and record that disposition in artifact provenance. This
   is compiler policy above the floor, not public continuation syntax.

Those decisions must be accepted in this record before source compiler lowering
begins. Arena trimming, actual code-mapping reclamation, concurrent mutation,
suspension, Continuations, scheduling, debugger mutation, and an optimizing
direct-call tier remain owned by later records and do not block S2.

## 17. Implementation slices after acceptance

Once §16 closes, S2 should land in dependency order:

1. **S2.1 — Semantic order, logical IR, and oracle.** Make synchronous emission
   sequence receiver and arguments explicitly, then build deterministic
   CFG/value IDs, evaluation-order tests, reference liveness, and an uncolored
   interpreter.
2. **S2.2 — Descriptor layout v2.** Add PC root maps, edge transfers, canonical
   serialization, validation, Frame tracing, and transition-order GC tests while
   preserving S1 layout v1.
3. **S2.3 — Coloring and call staging.** Add interference construction,
   deterministic coloring, staging/coalescing, source locations, and property
   comparison against the uncolored interpreter.
4. **S2.4 — Named-method lowering.** Emit StepMethods, thunks, registrations,
   ordinary-send entry, direct migrated calls, forced-GC fixtures, and the deep
   mutual-recursion proof.
5. **S2.5 — Blocks and artifact parity.** Lower captures and block bodies,
   exercise direct block-vtable registration, and prove live/AOT descriptor
   identity.
6. **S2.6 — Image integration and acceptance.** Implement the durable tag,
   mapped/rebuilt registration check, complete 60E's S2 items, publish same-run
   measurements, and run the repository gates.

No slice may expose a resumable binding whose descriptor, registration,
rooting, or image-recovery path is only partially safe.
