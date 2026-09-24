# Execution over the production Id heap

Design 90 batch 1 now runs native Methods, Blocks and Machines over the production
prefix, including frames, bounded drives, suspension, continuations and native
control boundaries, inline fallback, atomic flat/graph recovery, hot PC candidate
selection and scalar numeric evaluation. Batch 1 is complete with allocation-elision
explicitly moved to batch 2. There is no compiler dependency or alternate heap.
The pure artifact codec is a private implementation dependency.

## Traced producers and consumers

The retained implementation establishes these boundaries:

| Source | Existing responsibility and migration |
|---|---|
| Runtime `runtime/method.rs` | Exact `NativeEntry` capture; `invokeOn:` and `invokeOn:arguments:`; `methodFor:` and `addMethod:as:`. The old function-address owner registry and Lifetime `set_edge` are replaced by tracing actual Id dictionary bindings to owned Methods. |
| Runtime `legacy_method.rs` | Language lookup returns Method/nil, while Id consumes NativeEntry/Unit. Keep that adaptation with Execution and retain both function and context; discard the all-Oop calling-convention bridge. |
| Runtime `execution.rs`, `execution/`, `runtime/scheduling.rs` | Preserve frame arenas, generation leases, acquired-entry and frame pins, bounded drives, native barriers, suspension and continuations. The engine and its unit tests now live in this crate. `machine.rs` connects the retained transitions to Id; `control.rs` reconnects the native Block adapters. Hot PC selection and scalar numeric evaluation now live here. Compiler construction and allocation-elision remain batch 2. |
| Runtime `runtime.rs` drive/recovery and retirement | Selected entries remain pinned across nested calls, waiting frames and recovery. End a managed turn before draining releases, keep the outward result rooted, and never perform asynchronous cleanup from a finalizer. Preserve these mechanics when moving the drive. |
| Runtime `registration.rs` | Validate descriptor bytes, arity, digest, manifest agreement and prepare/abort claims. Keep the codecs and admission algorithm private. Do not send the old Rust/C registration structure through Bytes. The native descriptor constructor below feeds the retained canonical validator and frame registry directly. Portable-profile admission and generated recovery publication still need their real batch 2 producers. |
| Former TCC installation/retirement exports | Removed from production. TCC owns actual mappings; generated initializers construct owned Methods and use `addMethod:as:`. TCC drains detached mappings after Execution's checkpoint and `retirementReady` admission. |
| Codegen `native.rs` and retained legacy emitters | Production source compilation emits typed NativeEntry steps, Method registration and Block creation through current owner messages. Legacy portable/Image/optimizer emitters remain migration inputs for their later consumers. |

## Callable contract

Bootstrap loads `libbirnam_execution` with Namespace after the completed values
and collections. It publishes Execution, Method/MethodVt, Block, Machine,
FrameView, Continuation and the existing StepAction prototypes. State belongs to
that initialized Execution instance; repeated loading returns that instance.

| Receiver / selector | Operands | Result |
|---|---|---|
| Method `fromNative:owner:` | NativeEntry, Object owning its code/context | A Method capturing that exact entry and an opaque owner reference. Native authors enumerate their own context references. |
| Object or behavior `methodFor:` | selector Object | An owned Method for current lookup, or canonical nil. A raw native method can be captured only if its resident image can be pinned. Its author still owns its native context. |
| Method instance `invokeOn:` | receiver Object | Invoke its captured implementation with no arguments, applying the existing language result conversion. |
| Method instance `invokeOn:arguments:` | receiver Object, argument-sequence Object | Read through live `size`/`at:` messages and invoke the captured implementation. Retain the sequence and each intermediate result across callbacks. |
| Method instance `clone` | none | The same Method identity. |
| behavior `addMethod:as:` | Method Object, selector Object | Install the Method through the current Id `addMethod` operation. Installing `lookup` adapts Method/nil results to NativeEntry/Unit. |
| Execution `checkpoint` | none | Unit. Release retired resident-image pins only outside active invocations and GC callbacks. This is the native retirement checkpoint, not a task scheduler. |

All sends use current Id lookup. Capturing an actual callable for exact invocation
is the existing Method semantics, not permission to cache dependency services.
NativeEntry context is never guessed to be an Oop or decoded by another owner.
At the language invocation boundary, Object stays Object, Unit becomes nil,
NativeEntry becomes an owned Method, and scalar results/native failures use the
existing `nativeResultRequiresWrapper`, `nativeInvalidRequest` and `nativeFailure`
Error kinds. Direct native sends keep their typed results. A captured native Id
lookup already returns NativeEntry/Unit and retains that convention when installed
as lookup; Object-valued language lookup uses the Method/nil adaptation.

An installed Execution entry names an immutable callable owned by a Method. GC's
behavior enumeration sends ObjectModel `referencesOf:do:` for its associations,
then Id `bindingsIn:do:` and visits the Method named by each Execution entry.
Thus a live binding retains Method → declared owner. Removal/replacement changes
that edge immediately, including through native Id `addMethod`/`removeMethod:`;
there is no parallel binding registry to synchronize. Dictionary inheritance
retains parent behaviors through Id's existing edges.

The Method registry itself is weak. `retire:` removes dead records before address
reuse and queues resident-image pins for the next safe checkpoint. A registered
origin's native callable allocation stays reserved until its engine generation
retires, preventing reuse of a function/context key still in the engine. Its dead
language edges are not traced: this reservation does not retain owner Objects.
Active calls
and partially read argument sequences are rooted through Execution `rootsDo:`.
Each synchronous callback borrows its NativeEntry visitor only until return;
no internal mutable borrow crosses a send. Lookup adaptation holds its selected
Method across nested lookup, replacement and collection before returning its
exact entry. Ordinary future sends perform current lookup.

## Machine and registration contract

| Receiver / selector | Operands | Result and ownership |
|---|---|---|
| Method `fromStep:descriptor:selection:owner:` | NativeEntry step, canonical FrameDescriptor-v2 Bytes, ArtifactSelection Bytes containing exactly one matching row, code/context owner Object | Owned resumable Method. Both byte spans are bounded and copied during the call; no Rust carrier crosses the boundary. The owner identity names the generation. |
| Method `fromStep:descriptor:selection:owner:implementation:` | Same executable/descriptor/selection/code-owner operands, plus implementation Object | Owned named-adapter Method. The descriptor must declare an implementation slot; its separate implementation is traced and copied into that slot on entry. |
| Method instance `withInline:at:owner:` | NativeEntry candidate, Integer baseline return slot, candidate owner Object | New captured Method with the same baseline registration and an optional inline candidate. See recovery contract below. |
| Method instance `optimizeWith:` | Sequence of plain native Methods, one per baseline PC; empty sequence removes the candidate | Same Method. Attach candidate metadata to the shared baseline registration; see hot selection below. |
| Machine `recoverAt:with:` / `recoverAt:withGraph:` | Integer baseline PC, recipe sequence Object | Machine Object after one atomic pending-call recovery; typed failure for missing/stale authority or invalid recipe. |
| Block `fromMethod:captures:` | Method Object, capture sequence Object | Block with `value` / `value:…` matching descriptor arity. Reads captures through current `size`/`at:`; validates descriptor capture count. Clone copies this owned representation shallowly. |
| Machine `new` | none | New Machine and its own scratch actions, allocated through Id. |
| Machine `runBlock:` / `runBlock:budget:` | Block or any receiver implementing `value`; optional Integer transition budget | Ordinary current `value` dispatch through the retained run adapter. Result, suspended Machine, captured Continuation, or runnable Machine at a committed budget stop. |
| Machine `resumeWith:` / `resumeWith:budget:` | supplied Object, optional Integer budget | Resume a waiting result-bearing edge. Reject runnable and completed Machines. |
| Machine `continueWithBudget:` | Integer budget | Continue a runnable Machine, without inventing a waiting result. |
| Machine `restoreContinuation:with:` | Continuation Object, supplied Object | Reconnects the former host `restore_stackless_continuation` operation: restore into its original suspended Machine, commit its pending edge and drive. Wrong-Machine admission does not consume it; successful restoration is one-shot. |
| Machine `destroy` | none | Abandon runnable, suspended, completed or failed state and release frame pins. No asynchronous task cleanup runs. |
| Machine `oopAt:` / `oopAt:put:` | Integer slot; optional Object value | Checked access to its current committed frame. |
| Machine `continueAt:`, `callSite:`, `tailCallSite:`, `returnAt:` | Integer descriptor PC/site/slot | Publish the existing checked StepAction. |
| Machine `suspendAt:continueAt:` / `captureAt:continueAt:` | Integer result slot and PC | Publish waiting/capture actions for the current delimiter. |
| Machine `restore:with:` / `abortAt:` | Integer continuation/value slots or reason slot | Existing low-level action publications; distinct from host continuation restoration. |
| Machine `protectedBlock:`, `leaveBoundary:`, `park`, `controlEligible` | Existing control-adapter arguments | Retained resumable native adapters, not language scheduling policy. `prepareBlockControl` is idempotent; loading installs these completed adapters once and subsequent preparation preserves replacements. |
| Machine `boundaryCallSite:`, `exitBoundaryAt:`, `exitRunnableBoundary:`, `boundaryDeliveryPending`, `acknowledgeBoundaryDelivery` | Existing site/slot Integers or exit Object | Retained protected-call and single-delivery mechanics. |
| Machine `currentFrameView` | none | Live view, or nil when no frame exists. Views validate arena/generation on every read; stale access returns `staleFrameView`, bad slots `invalidFrameSlot`. Identity selectors return Word; PC/count selectors return Integer. |

`executionState`, `isRunnable`, the five existing arena diagnostics, and action
kind/schema/operand reflection are also installed. Raw native admission failures
use typed Id outcomes. Language Method invocation keeps the conversion described
above. Step entries receive the Machine and zero or one Integer PC operand,
according to the admitted descriptor, and return an Object StepAction.

Id owns every allocation. `objects.rs` records only weak metadata and accesses
Execution's own payloads; it is not another allocator. Public boundaries refresh
behavior and allocation extent through Id. Blocks trace their captured Method and
values; Machines and Continuations trace exact live frame roots plus each frame's
originating Method. A captured Method alias traces its registration's original
Method. Actual dictionary bindings still supply binding reachability; registration
never fabricates a binding count or roots every code owner.

The original Method holds an explicit generation lease. Retirement releases
Machine/Continuation pins before Method leases, then the existing managed-turn
and checkpoint rules retire registration metadata. An entered or detached frame
keeps its exact Method and code owner reachable after replacement. End-of-frame
release removes that edge. Native executable contexts remain opaque and their
native authors remain responsible for their own reference reporting.

All mutable engine borrows end before Id sends or native calls. Driver scopes
root Machine, PC, selected Method and results across callbacks. Existing native
barriers prevent nested callbacks from parking the caller. GC `prepareAllocation`
admits public mutation and rejects collection-critical reentry. Finalization only
releases native state; it does not run language cancellation or cleanup policy.

## Remaining migration and acceptance

The canonical frame engine, boundaries, scheduling, tail-entry and recovery
algorithms have moved out of Runtime. The obsolete all-Oop wrappers in those
files are deleted. The pure tail layout plan now belongs here; Codegen temporarily
shares that private source without becoming an Execution dependency.

The mixed Runtime Method values, driver, task-control and recovery wrappers have
been removed. TCC's owner-registration bridge and its Runtime exports are now also deleted.
Production Codegen and TCC construct owned Methods directly. Runtime retains
allocation-elision and later integration algorithms. Retained source/compiler acceptance cases are migration inputs, not
passing production tests; see [Runtime's remaining scope](../birnam-runtime/README.md).

By explicit scope decision, allocation-elision moves to batch 2: numeric chain/loop
reuse, closure/region elision, unboxed numeric values, discarded clones
and virtual aggregates need real compiler producers and guarded recovery.
Their old allocation-budget/deferred-identity dependencies are explicitly retired
by the [revised optimization contract](../../docs/design/90-functional-modules.md#optimization-preserves-language-behavior-not-allocation-history).
Recovery describes observable values and aliases; it does not recreate dead
allocation history. The native candidate contract does not implement or prove
those algorithms. Batch 2 must supply generated-code registration, necessary
owner messages, portable admission, guarded recovery and real compiled-mapping
reclamation. Native fixture recovery does not prove production generated recovery.

The Store-backed Birnam TaskControl/TaskExecutor policy, image/worker integration
and full CLI startup remain deferred. The native control adapters here do not
substitute a new scheduler for that source policy.

## Validation

`mise exec -- cargo test --offline -p birnam-execution --lib` runs the 21
engine/planner/graph tests: canonical admission, frame validation, parallel-copy/root
transitions, capability reuse, scheduling barriers and generation leases.

`mise run native-messages` audits actual dylib exports/dependencies and runs
`execution_methods` and `execution_machines` with the existing prefix tests. Two
independent C fixture dylibs are compiled and loaded through Id. The Machine
fixture consumes the established descriptor golden files, not a second runtime.
Production cases include calls/tail calls, native arguments/results and GC, Block
captures/cloning, fresh replacement, two-Machine interleaving, frame-view staleness,
wait versus runnable resumption, one-shot capture/restore, protected-boundary exit,
native barriers, collection-phase rejection, and owner reclamation after paused
replacement or continuation consumption. The final lifetime probes release all
temporary Method/Block/owner roots and retain only the paused Machine or detached
Continuation; their declared owner is reclaimed after the last frame edge ends.
Fixture contexts intentionally live for the test process; reclaimable compiled
mappings are a separate batch 2 check.

## Inline candidate and recovery contract

The retained inline-call consumer needs an executable candidate, its baseline
Method and opaque code owner. `Method withInline:at:owner:` answers a new
owned Method preserving that baseline registration: NativeEntry candidate,
Integer baseline return-slot, Object candidate owner. Captures snapshot this
choice. The candidate runs only for an eligible non-tail, unprotected registered
Call, with Machine as receiver and the original receiver followed by arguments
as Object operands. Object answers commit an inline result; Unit declines to the
baseline. A candidate can instead consume its scoped request through the existing
Machine `recoverAt:with:` or `recoverAt:withGraph:` and then answer Unit. Rejection
or error releases the acquisition; a consumed request transfers it to the frame.
No recovery authority or Rust structure is passed through native operands.

Recipe sequences retain their existing size/at: encoding; graph construction
uses Integer, Float and ObjectModel messages. Admission is rechecked after
callbacks and before publication. The graph validator is a private pure
algorithm here, shared with Codegen until its producer moves. Native fixtures supply
executable candidates now; compiler construction and emitted candidates remain
batch 2 producers. This connection is not acceptance for the other existing
numeric/region/closure optimization algorithms.

A flat recipe contains exactly one Object per baseline descriptor slot. A graph
recipe is `[inputs, nodes, slots]`, all sequences read through current `size` and
`at:`. Nodes retain the existing encoding: `[0, Integer]`, `[1, Float]`, or
`[2, Integer input-index-of-behavior, field-references]`. A nonnegative Integer
reference selects an input; `-n-1` selects node `n`. Nodes only reference earlier
nodes. Limits remain 128 inputs, 64 nodes, 128 output slots and 512 total fields.
Repeated references preserve identity; distinct number nodes remain distinct and
Float nodes preserve exact bits. Malformed graphs publish no partial frame.

Graph aggregates require an all-Oop ObjectModel layout and ObjectModel as the
current inherited GC reference owner, including for empty aggregates. Integer,
Float and ObjectModel perform construction through live messages. Scoped roots
retain every intermediate across allocation and callbacks; guard revalidation
precedes each allocation and follows callbacks and field writes. A recursively
consumed request cannot publish again. The code owner for a recovered baseline
stays pinned until its frame ends or is abandoned; the finished candidate has no
remaining acquisition. Errors release unconsumed acquisitions.

Execution checks its own Machine slot/control entries through current Id lookup,
with `behaviorEpoch` fencing mutation during the check. Id `usesDefaultLookup:`
takes the Machine Object and answers Word zero/one by comparing Id's own lookup
function and context. GC `referenceOwnerOf:` takes a behavior Object and answers
the current inherited owner Object or Unit. These are owner-local observations,
not cached dependency entries. Changing Machine lookup or an observed control
method declines the candidate and runs the ordinary baseline. The candidate's
native author remains responsible for semantic guards on its body and preserving
baseline-observable operations; this constructor does not certify optimization.

The production C candidate exercises direct result, decline and both recovery
forms. Tests cover precise GC in recipe/factory callbacks, layout and trace-owner
replacement, custom lookup, duplicate consumption, invalid recipes, exact numeric
identity/bits, and code-owner reclamation after completion, abandonment or failure.
These tests do not exercise the deferred allocation-elision algorithms or their
compiler producers.

## Hot selection and scalar evaluation

`Method optimizeWith:` reads the supplied sequence through live `size`/`at:`
messages before publication. A nonempty sequence must have one plain
`fromNative:owner:` Method per admitted baseline PC. Invalid input preserves the
previous candidate and its hotness. An empty sequence removes it. Aliases share
this optimization metadata through their original registration; their captured
baseline implementation does not change. The candidate uses the baseline step's
Machine receiver, PC operands and StepAction contract.

After 64 eligible step visits, selection can enter the PC's candidate. Every
selection checks current Machine lookup and Execution's own slot/control methods,
fenced by Id's behavior epoch. Guard failure withdraws the candidate for future
calls; restoring controls does not reactivate it without another installation.
Callbacks can replace a candidate, so selection also rechecks candidate identity.
The selected Method has an independent scoped root: withdrawal during an entered
step cannot reclaim its code owner before return. Native candidate authors must
preserve baseline semantics and guard their own body assumptions.

Production Codegen uses ordinary resumable entries; automatic per-PC candidates
were removed in the [optimizer reassessment](../../docs/design/90-optimizer-reassessment.md).
Explicit native candidates still use this guarded contract. Their guards protect
entries that can bypass Machine operations, and entered Methods retain their code.
The 64-visit threshold is a retained local policy, not a language requirement or a
mandate for future producers. Allocation-elision's old accounting/identity protocols
are not requirements to recreate.

Hot synchronous unary calls ask the live Integer owner to evaluate their exact
selected function/context through `evaluate:receiver:argument:with:`. Integer
recognizes only its own arithmetic implementations, validates operands and current
ErrorVt, and borrows Execution's pure checked-i64 visitor. Unknown entries decline
before operand reads and execute the originally selected entry. Integer owns
mixed-number handling, ordinary errors and result construction through current
factories. Execution caches only bounded scalar hotness, never dependency entries
or foreign numeric layouts. The pure arithmetic operator is shared privately with
Codegen; there is no Execution dependency on compiler construction.

Production tests cross both hot gates, replace controls and arithmetic, withdraw
an entered candidate during a collecting callback, and observe owner reclamation.
They prove actual numeric visitor calls under precise GC, arithmetic failure and
live ErrorVt propagation. Repeated registration/collection without intervening
checkpoints covers the native context-address reuse regression. Fixture results
are deliberately distinguishable to prove selection; they are not optimization
equivalence or performance evidence.

## Compiled owners

TCC now uses these same registration and retention paths. A named adapter's code
Unit and implementation Block are separate traced references. Captured aliases
retain the original registration, including its implementation. `retirementReady`
on Execution answers native Word 1 only outside entered drives/Methods and a drain;
TCC asks after `checkpoint` before releasing detached native mappings. Actual
compiled-unit retention and libtcc byte reclamation are covered by the compiler
production test. The first discarded-clone consumer below uses this same lifetime
contract; broader allocation-elision and generated deoptimization remain pending.


## Discarded ordinary clone

A resumable Method accepts
`withDiscardedClone:answer:selector:return:in:` with native Word source-argument
index, Word answer-argument index, Symbol selector, Word baseline return-slot
index and Namespace Object. It returns a captured Method with private inline
metadata, retaining the original Method, selector and Namespace. The compiler
owns the body proof, as a native producer does for `withInline:`; Execution checks
argument/capture/return-slot shape and actual runtime ownership.

For an eligible unbounded inline call, Execution asks the current selected
operation's ObjectModel owner `canDiscardClone:of:`. Id `usesDefaultAllocation`
and Integer `canOmitIndexing` cover omitted construction and compiler-generated
index operations. The current Namespace Integer binding must match the owner used
by Execution. Checks use live messages and a mutation-epoch fence; control checks
are repeated after callbacks. The existing answer argument is already rooted,
so no further message occurs between the final verdict and the inline-return
commit. Failure to admit executes the ordinary call. Existing native inline
candidates retain their original contract.

This first consumer handles only a whole uncaptured callee equivalent to
`[:p :answer | [p clone]; answer]`, with a zero-slot ordinary source. The selector
may be an alias of the actual clone entry. Bounded, tail and protected calls use
the ordinary path, preserving supported stop/Frame inspection behavior. No
allocation is charged for an omitted object and no identity is materialized later.
See [the implementation receipt](../../docs/design/90-discarded-clone.md).
