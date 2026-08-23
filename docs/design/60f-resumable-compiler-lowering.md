# 60F — Resumable Compiler Lowering

> **Status:** S2 implemented decision record. The migration unit,
> all-or-nothing eligibility rule, evaluation and lookup order, source-send
> boundary, CFG/PC model, Oop liveness, descriptor-v2 root maps, deterministic
> slot coloring, edge-transfer semantics, and call-staging rules were accepted
> 2026-08-07. The fresh-Oop ingress-root handoff, retained implementation-
> closure adapter, exact eligibility matrix, physical descriptor carrier, and
> Design 49 migration-change projection were accepted 2026-08-14. The
> pointer-free `ResumableManifest` artifact/image projection, lazy block-vtable
> resolver, mapped-image verification sequence, and no-IdApi-growth boundary
> were accepted 2026-08-14. The revision-scoped `ArtifactSelection` carrier,
> relational materialization boundary, artifact/generation provenance, and
> live/AOT replay rule were accepted 2026-08-14. The identity-layer separation,
> implementation-anchor root table, and artifact-slice versus generation-total
> selection composition were accepted 2026-08-14. The final adversarial review
> closes the resolver-key gate with artifact-local manifest indices. The
> prepare/publish/abort ordering and staged-unit admission contract in
> §§13.1 and 13.4 were accepted 2026-08-15. The shared private target-resolution
> amendment of 60E was accepted the same day. The S1 substrate alignment and
> [60I](60i-stackless-artifact-byte-grammar.md)'s exact durable bytes,
> native-envelope layout, bounds, and fixture gate closed 2026-08-22.
> S2 implementation began the same day and closed 2026-08-23 with the shared
> GenerationProjection authority, capture-free projected initializer subset,
> candidate AOT delivery, and mapped/fresh-rebuild image parity. The
> retained fixed-stack depth miss activates S4; it does not reopen S2.
>
> This record lowers the accepted execution semantics of
> [60A](60a-stackless-execution-semantics.md), the private Frame model of
> [60B](60b-stackless-machine-and-frames.md), the Method-shaped StepMethod
> contract of [60C](60c-stepmethod-calling-abi-options.md), and the registration
> seam of [60E](60e-resumable-registration-and-entry.md) into a compiler
> contract. 60F owns semantic contents and cross-format agreement;
> [60I](60i-stackless-artifact-byte-grammar.md) is the sole authority for their
> physical bytes and native envelope. This record does not add suspension,
> scheduling, public continuation syntax, static dispatch, or a new IdApi field.

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
| Fresh Oop handoff | A producer may carry a fresh Oop in a generated local only through a statically verified safepoint-free handoff into the immediately following IdApi send. The send ingress-roots receiver, selector, and arguments before lookup or allocation; a store establishes the descriptor root before that scoped ingress root ends. |
| Named method closure | A named resumable entry retains the exact implementation object supplied to `defineMethod:on:as:` in a dedicated Runtime-owned GC anchor. Frame construction copies it into the descriptor-declared implementation slot. Its StepMethod is an adapter that performs the ordinary late-bound `value:`-family send; the implementation block body is selected independently. |
| Durable resumable identity | Each unit emits one canonical pointer-free `ResumableManifest`. Image v5 carries the same expected entry set plus tagged references to generated block vtables. Mapped boot compares the image, artifact, Runtime registry, and restored bindings before user entry. |
| IdApi boundary | Durable resumable discovery and recovery use compiler artifacts, Runtime side tables, image records, and private host shims. They add no IdApi field, object/vtable slot, lookup rule, or Method ABI. |
| Selection carrier | Relational or other orchestration materializes a revision-scoped canonical `ArtifactSelection` before compilation. The compiler performs no live relational query. Artifacts retain the total table for their declared revision scope; DispatchGenerations retain the canonically recomposed `GenerationSelection`. Image/AOT recovery replays those bytes rather than choosing again. |

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

§13.5 freezes the carrier. Relational rules or another policy layer may derive a
sparse request, but orchestration materializes it into an owned,
revision-qualified value before the compiler runs. The compiler expands omitted
implementations to `Synchronous`, validates every explicit key, and emits one
canonical `ArtifactSelection` total over its declared compilation scope. It
never queries Store, the relational
engine, an environment flag, the current target binding, or ambient migration
state while lowering.

`ResumableRequired` first runs an eligibility pass. If any construct in that
implementation is outside the accepted S2 subset, compilation reports a kinded
diagnostic naming the construct and source span. It must not emit synchronous C
for the selected implementation, partially migrate it, or publish a thunk whose
body takes the old recursive path.

Ordinary compilation remains `Synchronous` until an explicit migration policy
selects an implementation. S2 therefore does not silently change all currently
lowerable code. The existing compile selectors supply an empty request and
therefore remain all-synchronous; explicit S2 orchestration uses §13.5's new
above-floor selection-bearing compiler protocol.

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

### 3.3 Exact eligibility matrix

Eligibility chooses an execution carrier; it never chooses whether Birnam
source is legal. The live and AOT paths call one shared normalization and
eligibility pass and produce the same decision, stable reason code, and source
span. One unsupported form makes the selected implementation wholly
ineligible; the compiler never mixes synchronous and resumable control inside
one implementation.

| Normalized form | S2 treatment |
|---|---|
| Integer, float, string, embedded-string, symbol, `nil`, and Boolean literals | Eligible. Allocating forms obey §7.1's fresh-Oop handoff. |
| Parameters, lexical locals, captures, and global references | Eligible. Global values remain late-bound through the existing environment. |
| `let` and expression statements | Eligible. |
| Ordinary sends | Eligible. §4's evaluation order and one Machine boundary per source send apply without selector or receiver-type exceptions. |
| Result-threaded `=>` syntax | Eligible after its existing structural send desugaring. |
| Vector and map literals | Eligible only after canonical desugaring to the ordinary `Vector new` / `add:` and `Map new` / `at:put:` sends. |
| Logic variables | Eligible only after canonical desugaring to the ordinary `LogicVar named:` send. |
| Block creation | Eligible. Capture discovery, allocation, and rooting follow §12. |
| Block body | Selected independently as resumable or synchronous. An ineligible nested body does not make creation of its block object illegal. |
| Method-definition implementation object | A literal block body is selected independently; the named method uses §3.4's retained-object adapter. A nonliteral implementation object remains valid and is invoked through the same adapter. |
| Top-level program or module initialization | Synchronous in S2. Resumable module initialization requires a later contract for persistent effects and publication. |
| Compiler intrinsic | Eligible only if normalization completely rewrites it to another row before selection. A residual intrinsic is ineligible. |
| Unknown future AST or IR form | Ineligible by default rather than guessed. Ordinary compilation continues through the synchronous compiler. |

Ordinary compilation selects `Synchronous`, so an ineligible implementation
continues to compile with its existing semantics. `ResumableRequired` reports a
kinded diagnostic at the first unsupported normalized form. Deterministic
resource ceilings such as maximum descriptor bytes, PCs, slots, captures, or
arity are selection failures under the same rule, not language limits.

No eligibility rule may inspect a selector spelling, receiver vtable, current
binding, or known library identity to assume synchronous or sealed behavior.

### 3.4 Named methods retain and invoke their implementation object

`defineMethod:on:as:` currently installs the supplied implementation object and
invokes it through the arity-appropriate `value:` family send. S2 preserves
that second late-binding point. A named method's ResumableEntry strongly owns
the exact implementation Oop, and its StepMethod is a small adapter which:

1. receives the original method receiver and arguments;
2. stages them as arguments to the implementation object;
3. issues a normal Machine Call using `value`, `value:`, or the required
   repeated-keyword selector; and
4. returns the result through the ordinary caller continuation.

The inner lookup is fresh on every invocation. If it resolves to a resumable
block entry, the driver pushes that block's independent Frame; otherwise it
uses the protected synchronous Method path. The named adapter never copies a
block's captures directly or bypasses its `value:` lookup. A later direct tier
may remove the adapter only behind an explicit lookup guard and invalidation
contract.

Registration receives the live implementation Oop separately from immutable
descriptor bytes and installs it in 60E's Runtime-owned implementation-anchor
table before the entry can become visible. Frame construction copies that Oop
into the descriptor-declared implementation slot before resolve protection
ends. Active Frames root that slot and pin the old physical code generation; a replacement affects
new sends without rewriting suspended execution. Image recovery must restore
or recreate that object before registration and binding publication.

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

Lowering version 1 uses at most 64 non-terminator operations per basic block;
an otherwise straight-line region is split with Continue. This is compiler-
local identity input, not a Runtime ABI ceiling.

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

There is one bounded ingress exception. A producer may return a fresh Oop into
a generated local when the compiler proves that the only following operations
before its storage send are native moves with no allocation, callback, lookup,
send, helper safepoint, or driver return. On entry, `IdApi.send` copies or roots
its receiver, selector, and argument Oops in Runtime-owned scoped storage before
performing metacircular lookup or any other GC-capable work. `oopAt:put:` may
then remain fully late-bound: the incoming argument stays rooted for the whole
send, and the checked store establishes the descriptor-traced Frame root before
that scope ends.

A source-send result does not use that generated-code bridge. The driver roots
the returned Oop immediately, commits it directly to the caller's pending result
slot, and establishes the successor root state before another Birnam safepoint.
The verifier rejects a producer-to-ingress path containing any safepoint or a
successor which does not root a still-live result. This is an implementation
obligation of the existing synchronous send boundary, not a new IdApi field or
a no-GC region.

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

- `continueAt:` selects an exact target from the current PC's canonical
  contiguous edge group, then validates and installs that next program counter
  before returning ContinueAction;
- `callSite:` validates its call-site descriptor and installs its continuation
  PC before returning CallAction; and
- TailCall, Return, and Abort publishers validate the current Frame before
  returning their actions.

Layout v2 preserves those semantics. A Continue PC may own multiple successor
edges, ordered contiguously by target PC; the requested next PC selects one
exact edge, while a Call PC still owns exactly one continuation edge. A
canonical publisher first requires that no other action transition is pending,
validates the source PC and exact edge or terminal operation, and prepares the
canonical scratch action and pending record without mutating the Frame. Failure
through that preparation point leaves the Frame unchanged. Continue and Call
then atomically record the pending transition and install the successor semantic
PC as the frozen protocol requires, but do not change the root-state PC or
reuse/clear predecessor slots. TailCall, Return, and Abort record terminal
pending state without a successor PC.

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

The owning unit registers every selected block body during unit initialization,
whether or not execution has evaluated the literal yet. Registration allocates
no block or block vtable. The literal and mapped-image loader call the same
idempotent unit-local resolver when they actually need the private vtable. The
resolver installs the already-registered entry thunk through `birnam_method`;
it never installs the StepMethod or reconstructs registration metadata from a
native address. §13.4 defines the durable identity and recovery path.

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
- portable implementation identity;
- selector-determined language arity and driver argument count; and
- the registration operation emitted before binding publication.

The mapping/initializer context supplies the process-local physical
CodeGeneration identity and lease only when executing the registration
operation; neither is part of the artifact description or its canonical bytes.
For the frozen no-argument initializer and Method-shaped entry point, 60I's
artifact-private, owner-thread-quiescent Begin/End exports carry that pair across the
complete initialization sequence. Generated Prepare operations read the active
pair rather than accepting it as an extra function operand. End clears the pair
and reports any registration failure accumulated by those operations; the
loader commits the Runtime staged-unit transaction only after an accepted
program result and accepted End, and otherwise invokes unconditional staged
rollback.
For a named-method adapter, the artifact initializer also supplies the live
implementation-closure Oop. That process-local value is not part of canonical
descriptor bytes and is never reconstructed from a native address.

Registration copies or interns every immutable descriptor component before
publication. No bitmap, transfer plan, source-map slice, or call-site table may
remain borrowed from temporary initializer memory.

Live and AOT compilation must call the same canonical descriptor serializer.
Mapped-image boot consumes the serialized descriptor and re-registers the thunk;
it does not rerun liveness or coloring. Rebuilt boot recompiles source and must
produce semantically identical registration. §13.4's manifest is generated
from these same accepted artifact descriptions; it is not a second lowering or
an inference from installed native addresses.

### 13.1 Registration-envelope semantic contents

The accepted physical carrier has two parts: a small native registration
envelope for process-local values and one pointer-free canonical descriptor
blob. It crosses a dedicated host registration shim and does not change IdApi.
The following list is the semantic field inventory; 60I owns the normative C
declaration, offsets, alignment, nullability table, ABI assertions, and fixtures.

The version-1 envelope is a sized C record containing, in order:

- `u32 carrier_version` and `u32 struct_size`;
- `u32 operation` (`Prepare` or `Abort`) and `u32 flags`;
- `u32 manifest_entry_index` and a zero `u32` reserved field;
- `u64 registration_token`, zero for `Prepare` and the returned token for
  `Abort`;
- a borrowed `const u8 *descriptor_bytes` and `u64 descriptor_length`;
- the entry-thunk and StepMethod function pointers;
- an optional implementation-closure Oop, required for a named adapter and
  absent for a generic block-vtable entry;
- an opaque process-local physical CodeGeneration lease and identity;
- the stable portable implementation identity; and
- the 32-byte SHA-256 of the complete descriptor blob.

The registration symbol is the only stackless-specific publication symbol.
`Prepare` validates the index against the current unit's staged manifest context
and stages an unpublished entry at that exact row, returning an opaque
initializer-local token. No second entry may claim the row. Successful
direct-binding publication consumes that staged entry through the existing
Runtime binding accounting. If installation is rejected, generated
initialization calls the same symbol with `Abort`, the token, and the same row
index; a mismatch fails without removing another staged entry. The token and
index are never Oops, descriptor fields, durable identities, or
language-visible values. Registration, Store publication, and abort preserve
60E's all-or-nothing ownership rules.

The envelope and blob are borrowed only for the registration call. Before
answering success, the Runtime validates and decodes the blob into owned
immutable storage, installs the optional implementation object in a dedicated
anchor slot, and acquires its own physical-code lease. Failure transfers no
ownership, retains no anchor, and publishes nothing. Candidate preparation does
not yet know or encode its successor logical DispatchGeneration; publication
associates the staged entry with that generation after its digest is computed.

### 13.2 Canonical descriptor-v2 semantic contents

The descriptor blob contains no logical DispatchGeneration identity, physical
CodeGeneration identity, Oop, function pointer, native address, `usize`, or
borrowed pointer. Its only implementation identity is portable and computable
from already-known compilation inputs. It uses fixed-width little-endian
integers, `u32` PC/slot/logical-value/section indices, `u64` identities and
bitmap words, `u32::MAX` for
an absent optional index, eight-byte-aligned sections, and zero canonical
padding. Slot `n` is bit `n % 64` of root word `n / 64`; padding bits beyond the
declared slot count are zero.

Those common encoding constraints are collected by 60I. The exact header,
directory, record, offset, bound, and fixture tables are deliberately not owned
by this semantic section.

Sections occur once in this canonical order:

1. header, identities, declared counts, and section directory;
2. receiver, argument, implementation, and capture entry locations;
3. PC records;
4. per-PC root bitmaps;
5. call-site records and call-staging descriptions;
6. edge records;
7. parallel-copy pairs and clear bitmaps; and
8. logical-value and source-map records.

Every section offset is relative to the start of the blob. Sections are
monotonic, non-overlapping, within `descriptor_length`, and checked with
overflow-safe arithmetic. Reserved header fields and unknown required flags
must be zero. The Runtime reads fields from byte slices rather than casting the
blob to native structs.

An edge's copy section is a sequence of `(source_slot, destination_slot)` pairs
with parallel semantics. Commit first reads every source into descriptor-sized,
preallocated transfer scratch, then writes destinations, performs declared
result initialization and clears, and finally advances the root-state PC. It
does not allocate or decode metadata during the transition.

The canonical serializer orders every record by its deterministic IR identity.
Live and AOT output for the same artifact input must have identical bytes and
SHA-256. Descriptor layout v1 remains a Runtime-internal S1 representation; S2
artifacts carry only the distinguishable version-2 blob. Pre-S2 images whose
runtime ABI hash lacks the registration carrier rebuild from source rather than
being upgraded in place.

Identity construction is acyclic and uses three distinct domains:

1. the portable implementation identity is derived from §13.5's stable source
   key and lowering-version inputs before descriptor serialization;
2. the descriptor hash is computed from the canonical descriptor bytes;
3. the logical DispatchGeneration base digest is computed from its source map,
   portable operations, carrier tags and descriptor hashes, and complete
   GenerationSelection digest;
4. that already-computed base digest becomes §13.4's manifest-wide authority;
   the later artifact checksum may cover the manifest but never feeds back into
   the descriptor or DispatchGeneration digest; and
5. mapping the finished artifact creates a process-local physical
   `CodeGeneration` identity and lease outside every canonical byte sequence.

The candidate's existing expected-predecessor projection supplies the complete
logical context needed for step 3. A compiler entry point never substitutes a
current mapping handle, allocation counter, or eventual artifact checksum for
that projection. The same descriptor may consequently be carried by more than
one logical DispatchGeneration without changing its bytes.

### 13.3 Design 49 execution disposition

A synchronous/resumable switch is a managed executable change, not a source
change. The SourceRevision remains identical, while the successor
DispatchGeneration records, for each portable method operation:

```text
target
selector
action = Install | Remove
carrier = Synchronous | Resumable { descriptor_sha256 }
```

The owning generation/artifact digest plus operation position already identifies
the implementation. The operation therefore does not duplicate an
implementation digest, carrier schema, layout version, entry kind, or native
address: those are owned by the generation or committed by the descriptor hash.

This is an append-only semantic replacement for Design 49's withdrawn v1 base-
digest encoding, not a compatibility extension. New generations use the domain
`birnam.dispatch-base.v2\0`. After each existing portable operation's definition
digest, v2 writes one carrier byte (`0` removal, `1` synchronous install, `2`
resumable install) followed by 32 zero bytes for removal/synchronous or the raw
descriptor SHA-256 for resumable. After the ordered operation sequence it writes
the raw 32-byte SHA-256 of §13.5's complete canonical `GenerationSelection`.
This final digest is required even when the table is all-synchronous: it makes a
block-only carrier change create a successor generation although blocks are not
separate Design 49 method operations. SourceRevision hashing remains v1 and
unchanged. Pre-S2 records are rebuilt rather than accepted as v2.

Changing a carrier tag, descriptor hash, or accepted selection-table digest
creates a successor generation even when source, selector, and intended
language behavior are unchanged. Rollback restores the predecessor operation,
carrier, and GenerationSelection. Active predecessor Frames keep their
generation pins; fresh sends perform normal lookup and reach the successor
binding.

Runtime reconciliation derives a binding fingerprint from the owning generation,
operation position, carrier tag, and optional descriptor hash. A synchronous
binding must name the generation-owned Method installed for that operation. A
resumable binding must name a registered thunk whose entry has the same owner and
descriptor hash. Raw code addresses remain local validation data and never enter
durable identity, so relocation alone is not drift. The generation's accepted
selection slice for the artifact's declared revision scope must agree with the
artifact export; §13.5 defines that projection. A slice or recomposed-total
mismatch is provenance drift even if the current native addresses happen to
match.

`deriveImageMethodTargets` reconstructs these expected fingerprints from the
saved generation provenance. After initialization and registration, it accepts
new process addresses only when their generation, operation, carrier, and
descriptor match. `managedMethodChanged:on:` is a pre-mutation notification and
does not receive the incoming Method; outside a managed commit/rollback, any
impending mutation of a managed direct binding therefore marks the current
generation drifted. Managed publication records the successor fingerprint
transactionally instead of inferring it through that hook.

There is no compatibility reader for pre-S2 operation provenance. Such images
take the accepted rebuild-from-source path and produce current records.

### 13.4 Durable `ResumableManifest` and image-v5 projection

The durable resumable tag is one canonical, pointer-free
`ResumableManifest v1`, not a language-visible bit on a Method, selector,
object, or vtable. The compiler derives it from the same accepted artifact
descriptions that supply §13's registrations. Store or method-mirror facts may
mirror this information for tools, but they are malleable program state and are
never the authority for native-code admission.

The manifest header contains:

- a domain/magic, schema version, total byte length, and required flags;
- the logical DispatchGeneration base digest computed by §13.2's acyclic order,
  which is also the existing image/code-generation authority digest;
- the entry count and canonical section offsets; and
- zero reserved fields for fail-closed extension.

Each entry contains:

- `NamedAdapter` or `BlockBody` kind;
- the stable `u64` implementation identity;
- the complete descriptor SHA-256;
- selector bytes and selector-determined language arity; and
- the exact §13.5 stable implementation key: SourceRevision, normalized root,
  root index, lexical block path, and implementation kind. A named adapter's
  empty path therefore retains its Design 49 portable operation position,
  while block identity remains unambiguous across composed revisions.

The generation authority is manifest-wide rather than repeated per entry. The
durable entry key is the tuple `(authority digest, stable source binding key,
descriptor SHA-256)`. The `u64` portable implementation identity remains useful
for descriptors, source maps, and diagnostics, but collisions never select an
entry or invalidate two otherwise distinct source keys. The durable key contains
no Oop, native address, `usize`, Runtime handle, registration token, physical
lease, or manifest index. Manifest rows sort by kind, source binding key, and
implementation identity. Duplicate
entry keys, duplicate source binding keys within a kind, noncanonical order,
unknown required flags, malformed selector arity, or a descriptor hash not
owned by the unit are invalid artifacts. Live and AOT compilation of the same
accepted artifact input must emit byte-identical manifest bytes. 60I owns the
exact row grammar, export symbols, length carrier, and resolver C signature.

The unit exports the exact canonical bytes and length through 60I's artifact
ABI. Their contents are copied and validated before the artifact loader accepts
the unit. More precisely, physical mapping makes the static exports available; the
host copies and validates the manifest and opens a Runtime staged-unit context
before it invokes the generated Birnam initializer. Native static constructors
may not register or publish resumable entries. The staged context binds the
exact manifest bytes and authority to the physical CodeGeneration, making an
initializer's manifest index unambiguous. A unit that contains block-body
entries also exports 60I's unit-local manifest-index resolver.

The resolver accepts an artifact-local index into that exact unit's canonical
sorted manifest and returns that row's private vtable Oop. It bounds-checks the
index and requires a `BlockBody` row whose staged registration has the same
physical CodeGeneration, implementation identity, descriptor hash, kind, and
arity. It is idempotent within the mapped generation: repeated normal
evaluation, image header resolution, and direct Oop-field resolution answer the
same vtable. It runs only with the owning Runtime entered. A null result, an
out-of-range index, a named-adapter row, or a wrong-unit/code-generation context
fails rather than falling back to an exported vtable name.

Canonical manifest indices are assigned only after the complete manifest has
been sorted. Generated block-literal code embeds its row index, and every
registration envelope—named or block—carries its own index so post-initializer
admission compares one exact row rather than searching by `u64` identity. The
index is a private physical locator valid only with the exact artifact manifest;
it is not a source identity, relation key, or stable cross-artifact name.

Every manifest entry is registered unconditionally while the unit initializer
runs, including a block literal that has never been evaluated. This registers
immutable execution metadata only; it allocates neither the block object nor
its vtable. The Runtime's generation-manifest reference retains an accepted
unbound block entry until the owning unit can no longer execute. When normal
evaluation or image recovery first asks for the block vtable, the resolver uses
the ordinary `proto`, layout, and `birnam_method` paths to create it and install
the already-registered thunk. The resolver never registers lazily, exposes a
StepMethod as a Method, or derives an entry from the relocated thunk address.
Its instantiated vtable is recorded in the Runtime's block-vtable catalog and
rooted for the lifetime of that generation or its surviving bindings.

Registration during initialization remains transactional. Named bindings may
consume their prepared entries immediately, and block bindings created by
top-level evaluation may do the same. After the initializer returns, the
artifact loader compares the complete manifest with the generation-owned
prepared and published entries. It atomically commits one manifest reference
for every exact entry or rolls back the unit's registrations and publications;
it never accepts a partially registered manifest.

#### 13.4.1 Image-v5 representation

S2 makes a clean image-format cut to v5 and bumps the hashed runtime ABI.
Pre-S2 images are not upgraded in place; they take the accepted source-rebuild
path and produce a current image.

An image with a Code record contains exactly one `Resumables` record, including
when the manifest is empty. The record follows Authority and precedes Heap,
Facts, and Code records. It contains the authority digest, the complete
canonical manifest bytes, and their SHA-256. Missing, duplicate, reordered,
noncanonical, checksum-mismatched, or authority-mismatched records fail image
preflight before mapping or Store mutation. After mapping, the image manifest
must also be byte-identical to the manifest exported by the captured artifact;
a digest match alone is insufficient acceptance evidence. An image without a
Code record contains no `Resumables` record.

Image-v5 heap and fact fields add 60I's tagged resumable block-vtable manifest
index alongside ordinary exported-name and heap references. A heap object's
vtable locator is likewise tagged as either an exported vtable name or a
resumable block-vtable manifest index. The index addresses the canonical sorted
manifest carried by the same image; it is valid only when that row is a
`BlockBody`. This supports both
ordinary block instances and a durable Oop field or fact that directly names
their private vtable, while repeated references preserve one resolver-returned
Oop identity. The image never repeats a full entry key at each reference,
serializes the generated vtable's native slots, substitutes its exported
parent, or publishes a generated name through IdApi.

The image writer does not infer these references by examining Method addresses
through IdApi. The Runtime supplies a read-only image catalog that maps each
instantiated generated block vtable to its owning artifact authority, canonical
manifest index, durable entry key, and current generation-owned binding
fingerprint. The collector verifies the key against that indexed row and writes
the row index when encoding object
headers and Oop fields. A foreign-generation, missing, retired,
descriptor-mismatched, or drifted generated vtable makes mapped-image capture
fail; it is never silently encoded as an ordinary parent-vtable object.
Supporting images that embed several executable authorities is a later image
composition feature, not an excuse to omit an owning unit from S2 output.

#### 13.4.2 Mapped recovery and final check

Mapped-image recovery is ordered as follows:

1. preflight image v5, Authority, Code, and the complete `Resumables` record;
2. map the captured unit without invoking its generated Birnam initializer,
   copy and validate the artifact manifest, require byte identity with the
   image manifest, and open the staged-unit context;
3. run the generated initializer inside one rollback-capable generation
   transaction;
4. compare the expected manifest set with the unit's prepared and published
   Runtime registrations;
5. atomically commit the exact generation-manifest entry set;
6. materialize heap objects, passing every checked block-vtable manifest index
   directly to the unit-local resolver and verifying the result through the
   Runtime catalog;
7. replay durable facts and perform Design 49 binding reconciliation; and
8. verify every named binding fingerprint and every referenced block-vtable
   binding before the image entry point, reactor, or other user execution.

The registry comparison is exact only within the captured generation. Entries
owned by another Runtime or generation are irrelevant. A missing or extra
generation-owned registration, duplicate entry, wrong kind, authority,
implementation identity, descriptor hash, selector/arity, resolver result, or
final carrier answers `resumableEntryUnrestored` as an image-validity failure.
The diagnostic names the stage, entry kind, durable source binding key, and
expected versus observed identities without printing process-local addresses
as durable identity. No first send is allowed to discover the error.

Rebuilt recovery recompiles the saved source through the same lowering,
descriptor, manifest, and registration paths. It need not reproduce relocated
addresses, but it must produce the same durable entry keys, named binding
fingerprints, block-vtable sharing, late-bound behavior, and final program
result as mapped recovery.

#### 13.4.3 IdApi and malleability boundary

The Runtime registry, block-vtable catalog, protected target resolver, and
Machine execution state are private mechanisms inside `birnam-id::Runtime`.
Artifact selection, compiler manifests, the artifact resolver, and image codec
remain above or beside that floor. This decision adds no public IdApi field,
stackless query, Method argument, object/vtable payload slot, exported language
name, lookup exception, or selector restriction. Registration and entry use
private host symbols in the already accepted `birnam_*` family; the resolver is
a private artifact export consumed by the loader, not an IdApi or
Birnam-language export. The image codec receives the Runtime catalog explicitly
rather than teaching public IdApi to reflect on resumability.

Ordinary `IdApi.send` and Machine Call/TailCall both use the Runtime's one
private resolve-and-pin authority over the exact metacircular lookup result.
This makes the execution substrate stackless-aware without moving lowering or
selection policy below IdApi, and without changing lookup, inheritance, DNU, or
late-binding semantics.

Normal lookup and mutation remain authoritative. Managed replacement records a
successor DispatchGeneration and updates Runtime binding/catalog references
transactionally. The existing pre-mutation overwrite notification marks an
unmanaged change to a managed resumable binding drifted because it cannot see
the incoming Method. Drift does not prohibit the language mutation or rewrite
active Frames; it only prevents the old manifest from claiming that the changed
binding is a valid mapped-image snapshot. Active Frames and implementation
objects retain their accepted generation pins, while fresh sends perform the
same late-bound lookup as before.

### 13.5 Selection request and provenance carriers

Selection is explicit compilation policy above the ID layer. It is neither
source syntax nor a property inferred from a selector, current binding,
receiver vtable, module name, environment variable, compiler optimization
level, or eligibility result. The only accepted requests are:

```text
Synchronous
ResumableRequired
```

There is no `Preferred`, `Automatic`, or fallback disposition. A required body
that cannot satisfy §3.3 fails the whole unit transaction before publication.

#### 13.5.1 Stable implementation keys

Orchestration may supply a sparse request, but each requested implementation is
identified independently of a native address or live Oop by:

```text
source_revision_digest
root_kind = NonMethodProgram | MethodOperation
root_index
lexical_block_path
implementation_kind = NamedAdapter | BlockBody
```

`MethodOperation.root_index` is the operation's canonical position in that
SourceRevision. `NonMethodProgram.root_index` is zero. A named adapter uses a
method-operation root and an empty block path. A block path is the sequence of
`u32` child ordinals from its normalized root through lexically enclosing block
bodies. Ordinals come from the shared normalized tree in source order, never
from byte offsets, hash iteration, current dispatch, or compiler-emission
order. This also identifies block bodies created by synchronous module
initialization without making the top-level program itself resumable.

Remove operations and the nonmethod program activation are not implementation
rows. Every installed named adapter and every lexical block body, including a
nested block and an implementation object literal passed to
`defineMethod:on:as:`, has exactly one row. Duplicate keys, a named row with a
block path, an absent operation/revision, a path that does not resolve to one
block, or an unknown implementation kind rejects the request before lowering.

#### 13.5.2 Materialization and canonical bytes

> **Request shape accepted 2026-08-23.** The Birnam-facing request is an outer
> Vector of exact seven-element row Vectors:
>
> ```text
> [#implementationDisposition,
>  revisionDigest,
>  rootKind,
>  rootIndex,
>  lexicalBlockPath,
>  implementationKind,
>  disposition]
> ```
>
> `revisionDigest` is a String containing exactly 64 lowercase hexadecimal
> bytes. `rootKind` is exactly `#nonMethodProgram` or `#methodOperation`;
> `implementationKind` is exactly `#namedAdapter` or `#blockBody`; and
> `disposition` is exactly `#synchronous` or `#resumableRequired`. `rootIndex`
> and every element of the lexical-block-path Vector are nonnegative SmallInts
> bounded by `u32`. The path is bounded by 60I's `MAX_PATH_DEPTH`. No Map,
> association-like object, alternate tag, uppercase digest, integer enum,
> omitted field, or extra field is accepted. Duplicate stable implementation
> keys reject the complete request even when their dispositions agree.

The request is deliberately convenient relational input, not durable encoding.
The compilation boundary validates the complete outer Vector, copies every
digest, ordinal, enum, and disposition into compiler-owned storage before
lowering, enumerates the complete normalized implementation inventory for the
declared SourceRevision scope, fills every omitted row with `Synchronous`, and
produces one total `ArtifactSelection v1` for that scope.

60I owns its pointer-free canonical grammar, domain, headers, records, bounds,
and fixtures. Semantically, rows sort lexicographically by revision digest,
root kind, root index, block path, and implementation kind. Unknown required
flags, invalid enum values, nonzero reserved data, trailing bytes, noncanonical
order, duplicate/missing inventory rows, or count/length overflow fail closed.
Every row must belong to one declared covered revision, and every
eligible implementation in every covered revision must have exactly one row.
The compiler hashes the exact bytes with SHA-256.

The normalized inventory and request expansion are shared by live, candidate,
candidate AOT, and image compilation. Stable source scope plus the same sparse
request must produce byte-identical ArtifactSelection bytes. Changing a
relational rule after materialization has no effect on that in-flight
compilation; it can only request a successor compilation.

A `GenerationSelection v1` is a different, domain-separated value whose exact
domain and grammar belong to 60I. Its scope is the complete active
DispatchGeneration source map; its rows are the canonical union of accepted
artifact slices whose declared scopes partition the active SourceRevisions. It
repeats the covered-revision inventory and row semantics above so completeness
can be checked without consulting a mutable relation. Its SHA-256, not an
artifact-slice hash, enters `dispatch-base.v2`.

Canonical bytes are the authority at artifact, generation, and image
boundaries. A Rust implementation may decode them into a sorted vector or build
a temporary lookup index, but those native collection types, their iteration
order, and their partitioning are non-authoritative caches. Composition is
defined as a linear canonical merge of sorted rows; the GenerationSelection
bytes depend only on the complete revision inventory and rows, never on how
those rows happened to be bundled into code units.

#### 13.5.3 Compiler and artifact surfaces

The existing compilation selectors remain all-synchronous compatibility
surfaces by supplying an empty request. Explicit orchestration uses these new
above-floor protocols:

```text
CompiledUnit compile:named:output:selection:
Codegen compileCandidate:named:output:selection:
Codegen compile:output:selection:
```

They add no IdApi field. Selection-bearing compilation requires indexable
SourceRevision metadata; it does not guess identities for arbitrary unindexed
source. `compileImageCode:` deliberately receives no caller-selected policy. It
must read the exact accepted GenerationSelection from the current
DispatchGeneration, project the unchanged rows into the image compilation's
complete ArtifactSelection scope, and fail if that provenance is absent,
malformed, incomplete, or stale.

Every S2 artifact exports its canonical scope-total bytes and native length
through 60I's artifact ABI. Candidate manifest v4 commits the lowercase-hex
ArtifactSelection SHA-256 plus its exact covered-revision set in addition to its
existing revision and operation metadata; the candidate envelope's complete
artifact hash covers the exported bytes. Inspection copies and validates the
export before reflecting any row.

The exact v4 counted/length-framed grammar was accepted 2026-08-23:

```text
v4|runtime|N:name|source_digest|non_method_digest|method_digest|revision_digest|selection_sha256|R:<64-hex revision>*|D:<N:dependency>*|O:<N:target><N:selector><I|R><arity:>*
```

`selection_sha256` is exactly 64 lowercase hexadecimal bytes.
`R`, `D`, and `O` are canonical colon-terminated decimal counts; each `N:`
frames exactly `N` following bytes. Revisions are concatenated fixed-width
lowercase-hex digests in strictly increasing order. Dependencies, targets, and
selectors are nonempty framed byte strings, so legal selector separators such
as `|`, `,`, and `;` are data rather than grammar. Operations end with `I` or
`R` and a colon-terminated arity. No trailing byte is accepted. A reader may
retain v2/v3 compatibility for older candidates, but neither version can be
emitted by current S2 compilation or claim current activation provenance.

For every `ResumableRequired` row, lowering must emit exactly one matching
`ResumableManifest` entry. Every `Synchronous` row must emit none. Named rows
must additionally agree with the candidate's projected DispatchGeneration
carrier and descriptor hash. Block rows agree through their stable lexical
identity and descriptor hash. A request/selection/manifest/registration mismatch rejects the artifact;
the compiler never edits the accepted selection afterward to describe what it
happened to emit.

#### 13.5.4 Relational and generation projection

The relational layer may derive sparse request rows from arbitrary policy: call
graph analysis, recursion, module ownership, operator choice, staged migration,
or an explicit user decision. Query result order has no authority; the
materializer resolves human-facing target/selector descriptions to exact
revision/root/path keys and applies the canonical ordering above before
compilation.

Each ArtifactSelection export covers exactly the SourceRevision inputs compiled
into that unit. A candidate normally replaces one revision slice rather than
recompiling the whole active generation. Activation verifies the export against
candidate manifest v4, removes every predecessor slice for exactly the declared
covered revisions, carries all other predecessor slices forward byte-for-byte,
and inserts the candidate slice unchanged. Duplicate revision ownership,
overlapping scopes, missing active revisions, or duplicate implementation keys
reject activation.

The Runtime then performs the specified linear merge of those sorted slices and
serializes one canonical GenerationSelection over the successor's complete
source map. Its SHA-256 enters §13.3's DispatchGeneration base digest.
Generation-total identity therefore depends on the complete rows, not on code
unit partitioning. Candidate manifest v4 commits the ArtifactSelection slice
digest and scope; the accepted generation records both its immutable slice set
and the recomposed GenerationSelection bytes and digest.

Activation transactionally relates the SourceRevisions, exact artifact slices,
GenerationSelection bytes/digest, candidate/artifact digest, successor
DispatchGeneration, operation-carrier rows, and `ResumableManifest` digest.
Image v5 records the complete GenerationSelection. Each mapped backing code unit
must still match its own ArtifactSelection slice. A monolithic image unit covers
the complete active revision set, so its export has the same scope and rows as
the GenerationSelection even though their domain-separated headers and hashes
differ. Relational facts may expose the rows for queries and tooling, but
native-code admission still requires exact canonical-byte agreement at the
appropriate slice or generation boundary and agreement with Runtime
registrations; mutable facts alone cannot bless a carrier.

A carrier-only change leaves every SourceRevision unchanged and creates a
successor DispatchGeneration through §13.3's GenerationSelection digest. This
includes a change affecting only a nested or module-owned block. Rollback
restores the predecessor slice set, GenerationSelection, and operation
carriers. Active Frames remain pinned to
their accepted generation; fresh sends continue to use ordinary late-bound
lookup. Nothing in selection seals a selector, changes method lookup, or gives
the relational layer a process-local execution capability.

#### 13.5.5 Projected initializer transaction and candidate AOT

S2 admits one nonmethod initializer form into GenerationProjection: a
capture-free top-level `let name := [block]`. The binding's block keeps its
ordinary `NonMethodProgram/0/path/BlockBody` implementation key and may be
selected independently. Every admitted binding is materialized once during
activation, including an explicitly synchronous row, so a successor never
inherits a closure from a predecessor revision by accident.

The runtime journals the exact prior and post Store values for each affected
Globals key. Activation failure restores the prior values; rollback first
requires every current value to equal its recorded post value and then restores
the exact predecessor, including absence. This is a VM-owned root transaction,
not a general effect log. A projected binding with lexical captures, an
arbitrary top-level send, object mutation, I/O, FFI, or module introduction is
outside S2. Revision transition hashes all such residual nonmethod forms and
requires that digest and the dependency vector to remain byte-identical.

`Codegen compile:output:selection:` is source-to-candidate AOT delivery of this same
predecessor-bound candidate envelope. It does not create an eager selected
module-loader authority: the result must enter `CompiledUnit` open/map/activate.
Image code projects the complete accepted GenerationSelection. A selected
projected global is reconstructed exactly once by mapped code; `--rebuild`
compiles and maps a fresh unit from the durable source map and selection rather
than executing captured native bytes or serializing a duplicate closure fact.

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
- live and AOT canonical descriptors disagree for the same artifact input;
- the physical descriptor carrier cannot represent the accepted semantic data;
- canonical manifest construction finds a duplicate, malformed, or
  nondeterministically ordered entry;
- an artifact description, exported manifest, or image projection disagrees
  about an entry's kind, binding key, arity, or descriptor hash;
- a selection row names the wrong revision, root, path, or implementation kind,
  or the scope or complete generation selection inventory is incomplete or
  noncanonical; or
- request, artifact export, candidate manifest, generation composition,
  `ResumableManifest`, or Runtime registration disagree about a disposition.

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
3. live and AOT eligibility produce the same normalized form, decision, reason
   code, and source span for every matrix row in §3.3;
4. receiver evaluation occurs once, arguments occur once left to right, and
   lookup occurs afterward in both explicitly sequenced synchronous C and
   resumable output;
5. a redefinition or lookup-policy change during argument evaluation is visible
   to the outer send;
6. every source send increments a generated Call/TailCall probe, including a
   send whose current target is synchronous;
7. inherited, DNU, and custom lookup behavior matches synchronous compilation;
8. a named adapter retains its exact implementation object through forced GC
   after Prepare, publication, alias removal, and image re-registration, copies
   it into the Frame implementation slot, and performs a fresh late-bound
   `value:`-family lookup on every invocation; Abort and final quiescent teardown
   release the anchor without affecting another alias;
9. changing that implementation object's `value:` binding after method
   installation changes subsequent calls through the normal lookup rules;
10. tail position is stable across synchronous-to-resumable redefinition; and
11. ordinary callers observe the same result or ErrorVt value and no Machine.

### 15.2 Liveness, coloring, and GC

Tests must prove:

12. two interfering values never share a slot;
13. at least two noninterfering values do share a slot, proving coloring is live;
14. stable input produces byte-identical coloring, masks, transfers, and source
    locations in live and AOT modes;
15. every safepoint-live value's colored slot appears in its PC root map;
16. a fresh allocation survives the verified safepoint-free handoff into send
    ingress roots and then into its descriptor-rooted Frame slot;
17. inserting any safepoint into that handoff makes verification fail;
18. a slot whose logical owner changes while its root bit remains set is cleared
    or transferred correctly;
19. Call/Continue publication installs the frozen successor PC while tracing
    remains on the predecessor root-state map until driver commit;
20. mismatched action return and reentrant Frame mutation while pending fail
    without losing predecessor roots;
21. cyclic parallel-copy execution preserves all values;
22. dead and reassigned slots read as `nil`, never stale untraced Oops;
23. forced collection at step entry, after each generated store, inside
    late-bound microcode, during synchronous leaf invocation, while a caller is
    parked, and immediately after transition preserves exactly the required
    values;
24. a Call result slot is rooted as `nil` while the caller is parked and
    contains the result before successor execution; and
25. coloring does not change a captured block's values or lifetime.

Property tests should generate small CFGs, compare compiler liveness with a
reference data-flow solver, and verify coloring and edge transfers against an
uncolored logical interpreter. Focused fixtures still provide readable failures
for every rule above.

### 15.3 Calls, stack depth, and registration

Tests must also satisfy 60E §14's S2 items:

26. migrated-to-migrated Call, inheritance, and custom lookup push Frames
    directly with no thunk invocation;
27. a resumable block executes through direct block-vtable registration;
28. live and AOT registrations have identical portable descriptor, arity,
    implementation, root-map, coloring, and transfer identities; mapping those
    bytes twice may produce different physical CodeGeneration identities, and
    carrying the descriptor into a successor logical DispatchGeneration changes
    neither descriptor bytes nor hash;
29. the physical carrier rejects malformed sizes, offsets, hashes, masks,
    transfers, reserved bits, and ownership tokens before publication;
30. synchronous-to-resumable and resumable-to-synchronous carrier-only changes
    create successor DispatchGenerations without changing SourceRevision;
31. rollback restores both dispositions, relocation with a matching fingerprint
    is accepted, and an untracked or descriptor-mismatched carrier is drift;
32. TailCall to a synchronous Method increments `tailCallToSynchronous`, while
    the fully migrated deep-recursion proof remains zero;
33. two mutually recursive migrated implementations run at least one million
    calls with bounded native-stack movement;
34. the reusable call window handles zero through maximum tested arity and never
    overlaps an active Call result;
35. live and AOT output carry byte-identical canonical manifests whose named
    and block rows agree with their indexed descriptors and registrations; two
    block rows forced to share a `u64` implementation identity but having
    distinct source keys remain valid and resolve to their separate vtables by
    manifest index;
36. unit initialization registers a never-evaluated block body without
    allocating its vtable, and first normal evaluation creates exactly one
    private vtable through the same idempotent resolver used by image recovery;
37. image-v5 preflight rejects a missing, duplicate, reordered, malformed,
    checksum-mismatched, authority-mismatched, or artifact-mismatched
    `Resumables` record before mapping or Store mutation;
38. image encoding and recovery preserve repeated block-vtable identity through
    both heap object headers and direct Oop/fact references without exporting a
    generated name;
39. mapped boot rejects missing, extra, duplicate, wrong-kind, wrong-arity,
    wrong-descriptor, wrong-resolver, and final-binding mismatches as
    `resumableEntryUnrestored` before user entry;
40. mapped and rebuilt paths produce equivalent named registrations, block
    registrations, block-vtable sharing, late-bound behavior, and results;
41. an unmanaged rebind remains observable to fresh sends and marks the old
    mapped snapshot drifted rather than sealing the selector or changing an
    active Frame; and
42. Call/Return, ordinary-entry, per-step microcode, Frame-slot high water,
    boundary-depth, native-stack depth, and descriptor-size measurements are
    published against 60E §11's same-run baseline.

### 15.4 Selection and relational provenance

Tests must additionally prove:

43. the existing compilation selectors materialize a scope-total
    all-synchronous ArtifactSelection and preserve the frozen IdApi layout and
    fingerprint;
44. differently ordered relational query results for the same sparse request
    produce byte-identical ArtifactSelection bytes for the same declared scope
    in live, candidate, and candidate AOT compilation;
45. changing relational facts after request materialization cannot affect an
    in-flight lowering, and a structural spy observes no Store or relational
    query from the compiler pass;
46. wrong-revision, absent-root, invalid-path, duplicate, missing-total-row,
    malformed-length, unknown-flag, and noncanonical-order selections fail
    before lowering or publication;
47. `ResumableRequired` on an ineligible named or block body fails the whole
    unit without emitting or recording a synchronous fallback;
48. a named adapter, its implementation block, nested blocks, and a block owned
    by nonmethod module initialization can be selected independently by their
    stable keys;
49. candidate manifest v4, the exported ArtifactSelection slice and scope,
    `ResumableManifest`, descriptors, and registrations agree exactly, and any
    one-sided disposition or digest edit rejects the candidate;
50. candidate activation replaces only its declared SourceRevision slices,
    carries unchanged predecessor slices byte-for-byte, rejects duplicate or
    incomplete ownership, and recomposes a deterministic GenerationSelection
    independent of code-unit partitioning and native collection iteration;
51. a block-only selection change leaves SourceRevision unchanged, changes the
    DispatchGeneration digest, and rollback restores the exact predecessor
    slice set, GenerationSelection, and carriers; and
52. `compileImageCode:` accepts no policy override and reproduces the current
    generation's complete accepted selection rows, including block rows, while
    image v5 carries GenerationSelection, each mapped unit agrees with its own
    ArtifactSelection, and mapped and rebuilt recovery agree.

The canonical repository gate, native-prefix smoke, cold-archive smoke,
relocation smoke, frozen IdApi fixtures, and process-local image rejection tests
remain mandatory.

## 16. S2 pre-implementation decision gate

The lowering semantics, eligibility matrix, liveness and physical layout,
descriptor carrier, Design 49 projection, durable `ResumableManifest` image
integration, and relationally materialized ArtifactSelection/GenerationSelection
carriers, artifact-local manifest-index resolver, and prepare/publish/abort
ordering are accepted. The S1 shared private resolve-and-pin alignment and
60I's complete byte grammar both closed 2026-08-22. No design gate remains
before S2 implementation begins.

Arena trimming, actual code-mapping reclamation, concurrent mutation,
suspension, Continuations, scheduling, debugger mutation, and an optimizing
direct-call tier remain owned by later records and do not block S2. An
implementation must still satisfy §15's evidence; closing the design gate is
not acceptance of code that has not yet been written.

## 17. Implementation slices after gate closure

With §16 closed, S2 lands in dependency order:

0. **S2.0 — Selection and provenance carrier.** Add stable implementation
   inventory keys, relational request materialization, canonical
   ArtifactSelection slices, candidate-manifest v4 and artifact exports,
   partition-independent GenerationSelection composition/digest projection,
   and all-synchronous compatibility defaults without changing IdApi.
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
4. **S2.4 — Named-method lowering.** Emit retained-implementation adapters,
   StepMethods, thunks, registrations, ordinary-send entry, direct migrated
   calls, forced-GC fixtures, second-lookup malleability tests, and the deep
   mutual-recursion proof.
5. **S2.5 — Blocks and artifact parity.** Lower captures and block bodies,
   emit canonical manifests, register block bodies independently of vtable
   allocation, add the idempotent unit resolver and Runtime image catalog, and
   prove live/AOT descriptor and manifest identity.
6. **S2.6 — Image integration and acceptance.** Implement image-v5
   `Resumables` and block-vtable reference tags, mapped/rebuilt registry and
   binding checks, complete 60E's S2 items, publish same-run measurements, and
   run the repository gates.

No slice may expose a resumable binding whose descriptor, registration,
rooting, or image-recovery path is only partially safe.

Implementation ledger, 2026-08-23:

- S2.0 has the allocation-free canonical selection admission path, canonical
  encoders, sparse request materialization, portable identity derivation, and
  partition-independent generation composition. Candidate compilation now
  emits manifest v4 and the exact canonical ArtifactSelection export; staging
  recomputes the normalized inventory, and mapping checks the copied export,
  digest, and covered-revision set before executing candidate-owned code.
  Candidate ABI 4 also carries the exact expected-predecessor digest and the
  byte-exact ArtifactSelection payload. Successor projection removes the
  replaced revision slice, carries untouched slices forward, recomposes the
  complete GenerationSelection, and binds its digest into `dispatch-base.v2`.
  The approved seven-element Birnam request Vector is now admitted
  by exact runtime type, tag, symbol, digest, integer, path, shape, and duplicate
  checks and copied into owned Rust storage before lowering. The explicit
  `compile:output:selection:` and
  `compileCandidate:named:output:selection:` protocols materialize that request
  against the complete inventory. Explicit synchronous, named, and candidate
  `BlockBody` `ResumableRequired` rows publish their canonical provenance.
  Selection-bearing candidate AOT now emits that same predecessor-bound candidate
  envelope and must enter the same open/map/activate lifecycle, eliminating a
  second eager-loader authority.
- S2.1 has explicit synchronous operand sequencing plus deterministic logical
  values, binding versions, dense PCs, Call/TailCall/Return/Continue lowering,
  reference liveness, and an uncolored interpreter.
- S2.2 has a bounded borrowed decoder for all fifteen descriptor-v2 sections,
  stable-key identity checks, execution/source-map cross-validation against the
  nine frozen descriptors, and a private Runtime layout-v2 root/edge substrate.
  Pending edges retain predecessor roots; nonallocating commit uses
  preallocated parallel-copy scratch, initializes Call results, clears dead
  slots, and then advances root state. The production canonical serializer now
  reproduces all nine frozen descriptors byte-identically and re-admits every
  output; Runtime admission copies their execution metadata, including ordered
  capture-entry slots, into owned v2 storage without retaining artifact
  pointers. The compiler constructs all fifteen sections for capture-bearing
  BlockBody implementations from the colored logical IR and admits the result
  through that same serializer/parser path.
- S2.3 has deterministic conservative interference coloring, fixed entry
  locations, a disjoint reusable call-staging suffix, PC root maps, edge clear
  plans, and a colored interpreter checked against the uncolored oracle. The
  generated Method-shaped StepMethod uses only late-bound Machine microcode,
  stages receiver, selector, and arguments in language order, and passes both
  focused Rust tests and a C11 warnings-as-errors syntax check. Block-literal
  inputs participate in liveness and root maps; generated StepMethods allocate
  nested synchronous or selected closures and copy their canonical capture
  vector while the source values remain Frame-rooted.
- S2.4 has a pure named-adapter unit composer that takes an already-authorized
  logical generation authority, assigns canonical manifest indices, compiles
  descriptor/StepMethod/thunk/registration fragments, and hands exact
  operation-indexed fragments to artifact emission. The emitter creates and
  roots the implementation block first, calls Prepare with that live Oop,
  installs only the selected unique thunk, and calls Abort if direct
  publication rejects; the complete generated translation unit passes a C
  warnings-as-errors syntax check. Public candidate compilation now projects
  successor authority before emission, publishes and remaps exact manifest and
  selection exports, then completes live validation, host-quiescent activation,
  selected dispatch, rollback, and predecessor restoration. Publication uses
  monotonic process-local SmallInt capability IDs; admission performs the full
  plan check once, while commit rechecks exact active-generation identity and
  numeric Store basis. Collection-induced finalizer mutations therefore stale a
  prepared plan without changing dispatch; preparation suppresses direct removals
  as well as installs, so its dry run remains inert.
- S2.5 has the canonical ResumableManifest encoder/decoder, exact reproduction
  of the four core frozen manifests, descriptor/selector/stable-key checks, and
  exact ArtifactSelection agreement including forged-portable-ID rejection.
  The exact 136-byte registration-envelope carrier, staged-unit manifest
  context, physical generation identity and lease checks, owned descriptor
  admission, named-implementation rooting, one-shot Prepare/Abort tokens,
  incomplete-unit rollback, and first-binding publication token consumption
  also exist. The runtime ABI fingerprint is migrated to v3 and hashes the
  exact frozen registration tuple reconstructed from source-owned constants;
  fixture-free cold archives therefore reproduce it without importing test
  data. Codegen resolves the private registration symbol from the exact
  selected `RTLD_LOCAL` floor without widening loader visibility or growing
  `IdApi`. The artifact-private static Begin/End context is now emitted with
  exact zero/nesting/failure rules and is serialized by owner-thread
  quiescence; generated Prepare/Abort helpers
  consume it without changing the frozen initializer or Method ABI. The exact-
  floor loader now byte-checks an expected manifest, opens Runtime staging,
  brackets the complete initializer/entry sequence, commits only after an
  accepted program result and accepted context End, and otherwise uses an
  unconditional loader-private cancel seam. It assigns the physical mapping
  handle as the process-local generation identity and a nonzero monotonic
  mapping lease; neither enters canonical bytes. A zero-entry manifest leaves no
  committed Runtime unit, because such a unit
  owns no registration, code anchor, or catalog row; this permits a disposed
  methodless mapping's loader handle to be reused safely under a fresh lease.
  Production named lowering now
  composes resumable fragments into the unit, publishes the manifest and entry
  thunks, and retains GenerationSelection. Successful staging retains the byte-exact committed
  manifest and entry capabilities and invalidates every initializer-local Abort
  token. The staged Runtime context journals every direct method mutation,
  defers predecessor-entry retirement, and on failure restores overwritten
  bindings, removes new bindings, returns prepared entries to the unpublished
  state, and restores the pre-transaction block-catalog drift snapshot before
  unregistering the incomplete unit. The Runtime records generated block-vtable bindings bidirectionally by
  authority and canonical manifest index, roots the catalog, and makes
  overwrite or alias drift fail closed. A deterministic unit-local
  `__birnam_resolve_block_vtable_v1` C emitter creates each private vtable once,
  installs only its registered thunk through ordinary `birnam_method`, and
  rejects unknown indices. The candidate block composer now includes that
  resolver, eagerly prepares every selected block row before literal
  resolution, defers selected closure allocation during the dry preparation
  pass, and emits capture-aware selected and synchronous nested construction.
  Live candidate proofs cover a capture-bearing selected leaf, an all-selected
  nested family, and a selected parent with synchronous children through
  activation, ordinary dispatch, and exact rollback. Candidate materialization
  now also admits capture-free top-level block bindings, derives candidate AOT
  output through the same predecessor projection, and rolls Globals roots back
  exactly while rejecting capture-bearing or effectful initializer changes.
- S2.6 has a separate bounded image-v5 parser/writer that reproduces every
  valid frozen image byte-for-byte, rejects the committed malformed images,
  validates the Authority/Resumables/Code relationship, and checks every
  tag-10 occurrence against a `BlockBody` manifest row before materialization.
  Ordinary heap objects are decoded only through a runtime-supplied exported
  layout. Production Rust Codegen now reads the durable current
  GenerationSelection, projects every accepted row unchanged into the
  monolithic image ArtifactSelection, assigns generation-global symbol and
  manifest indices, rebases module-local operation and nonmethod-block keys,
  and emits the corresponding named and BlockBody resumable fragments. The
  compatibility `compileImageCode:` selector still accepts no policy input;
  the writer uses `compileImageCode:manifest:` to capture the dylib and its
  exact authority-bound manifest as one operation. The writer verifies those
  manifest bytes against the current generation authority instead of
  synthesizing an empty manifest, then places the same bytes in image v5.
  A production lifecycle test activates a nonempty generation containing a
  selected named adapter, its selected method-owned `BlockBody`, and a selected
  projected nonmethod `BlockBody`, then exercises image-code capture. The
  corresponding production acceptance run verifies all three indexed
  StepMethod/thunk families, the unit-local block-vtable
  resolver, exact exported manifest bytes, and total staged registration. Rust
  Codegen preflight, code mapping, and fact replay consume v5. The
  Runtime now exposes loader-private, exact-floor catalog queries for the
  writer's vtable-to-index lookup, the loader's index-to-vtable verification,
  and byte-exact committed-manifest agreement. These remain outside IdApi and
  Runtime ABI v3 because generated code does not import them. The production
  writer uses the reverse query for both tag-10 Oop fields and generated-vtable
  heap locators and rejects missing, foreign-authority, or drifted catalog
  entries. All-synchronous image-code artifacts now export the exact
  authority-bound empty manifest; image preflight and mapping copy its native
  length and bytes, require byte identity with the image record, and require a
  resolver export whenever the admitted manifest contains a block row before
  Birnam initialization can run. After staged commit the loader now rechecks
  the exact committed manifest through the selected floor, retains the exact
  artifact resolver for the recovery scope, resolves tag-10 heap locators and
  fields only through that function, and requires the resulting vtable to agree
  with the Runtime's authority/index catalog before allocation or fact replay.
  Recovery clears that resolver scope at its explicit end. Design 47's image
  lifecycle seam is now present: Image exposes a domain-separated composition
  identity derived from the admitted code checksum, pure Birnam replaces saved
  origin units with rollback-free `#composed` components before user entry, and
  Codegen rebuilds the stable-owned-target binding ledger from live direct
  dispatch. Source recovery recompiles and maps the accepted projection before
  composition adoption; mapped recovery uses the captured projection. The
  production smoke publishes a nonempty selected generation,
  proves identical mapped/rebuilt behavior and composition identity, verifies
  origin `#historical`/`#composedInto` facts and absent rollback capability,
  resaves and reboots the composed image, then activates and rolls back a new
  candidate AOT unit—including its projected global—over that recovered base.
  The retained S2 release measurement probe
  publishes Call/Return, ordinary-entry, fallback-thunk, microcode, Frame,
  boundary-depth, native-stack-movement, and descriptor-size evidence against a
  same-run `sends.compiled-unary` baseline; the 2026-08-23 run measured 1.170 x
  S for Call/Return and 1.308 x S for ordinary entry, within their 4 x and 6 x
  budgets. The separate fixed-2-MiB-stack depth probe reproducibly measured
  3,557 synchronous levels versus 1,277 mixed resumable/synchronous-barrier
  levels (`0.359 x`), intentionally failing the provisional `0.5 x` budget and
  activating 60E's recorded S4 migration trigger. Arbitrary effectful
  initializers remain intentionally outside the completed S2 projection.
