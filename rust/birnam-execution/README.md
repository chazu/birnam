# Execution over the production Id heap

The current Oop facade uses declared Method objects and the retained Machine
engine. Default compiler execution, suspension/control, frame inspection and
explicit pending-call recovery are connected. Current evidence and remaining
integration work are tracked in the [Oop integration record](../../docs/design/oop-continuation-id.md).
Historical feature descriptions below remain preservation requirements where
migration is unfinished; they are not claims of full runtime acceptance.
No v1 compatibility path is built.

The Oop incoming adapter retains signature metadata in Execution-owned admission
records. `nativeSignatureOf:` accepts an Oop and returns fresh signature Bytes for
an adapted Block, or nil otherwise. Codegen uses that public owner query when
constructing named Methods; it does not inspect Block storage. Native arguments
are validated by the adapter, while ordinary named arity behavior is unchanged.
Deferred `v` results return prepared continuation work after the selector body and
conversion scope have ended. Failure observers preserve the caller's Machine and
do not classify ordinary Error/Fault values as failures. The Oop native-boundary
test covers suspension, GC, replacement and mapped/rebuilt image recovery.

Capture inspection answers nil for uncoded Blocks, unsupported receivers and
invalid indices. `captureWord:of:` validates the target with Execution's own
storage and CodeObject metadata, then uses its current `captureWordAt:` reader.
Synchronous generated code uses this owner entry rather than sending a missing
capture message to an unrelated receiver. Resumable entry validates its required
capture storage before reading through the replaceable reader.

Oop Method invocation preserves native failure returns. A language Machine
converts them at its pending call; there is no additional invocation-result
continuation. The full materialization harness checks native and language callers,
cache eviction, capture ordering and native-unit/byte retirement.

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
| Former Codegen `native.rs` and legacy emitters | Production source compilation, portable materialization and Image recovery emit typed NativeEntry steps and construct owned Methods/Blocks through current owner messages. The old emitters are deleted; historical optimizer implementations remain in Git. |

## Callable contract

Bootstrap loads `libbirnam_execution` with Namespace after the completed values
and collections. It publishes Execution, Method/MethodVt, Block, Machine,
FrameView, Continuation and the existing StepAction prototypes. State belongs to
that initialized Execution instance; repeated loading returns that instance.

| Receiver / selector | Operands | Result |
|---|---|---|
| Method `fromNative:owner:` | NativeEntry, Object owning its code/context | A Method capturing that exact entry and an opaque owner reference. Native authors enumerate their own context references. |
| Object or behavior `methodFor:` | selector Object | An owned Method for current lookup, or canonical nil. A raw native method can be captured only if its resident image can be pinned. Its author still owns its native context. |
| Execution `directMethodFor:in:` | selector Object, behavior Object | Observe only that dictionary through Id. Return nil when absent, the actual installed Method when Execution owns it, or an image-pinned observation of a foreign native entry. |
| Method instance `bindingMatchesIn:as:` | behavior Object, selector Object | Compare its installable entry with the current direct Id binding, returning the ordinary Boolean. This is an explicit observation, not a dispatch guard. |
| Method instance `invokeOn:` | receiver Object | Invoke its captured implementation with no arguments, applying the existing language result conversion. |
| Method instance `invokeOn:arguments:` | receiver Object, argument-sequence Object | Read through live `size`/`at:` messages and invoke the captured implementation. Retain the sequence and each intermediate result across callbacks. |
| Method instance `clone` | none | The same Method identity. |
| behavior `addMethod:as:` | Method Object, selector Object | Install the Method through the current Id `addMethod` operation. Installing `lookup` adapts Method/nil results to NativeEntry/Unit. |
| Execution `checkpoint` | none | Unit. Release retired resident-image pins only outside active invocations and GC callbacks. This is the native retirement checkpoint, not a task scheduler. |

All sends use current Id lookup. Capturing an actual callable for exact invocation
is the existing Method semantics, not permission to cache dependency services.
NativeEntry context is never guessed to be an Oop or decoded by another owner.
Restoring a direct foreign-native observation with `addMethod:as:` preserves its
raw entry and ABI, including non-Object results. Ordinary `methodFor:` capture and
`withImplementation:` construction keep their existing independent semantics.
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
GC enumeration accepts visitor Objects and sends `visit:` for each reference.
Id sends `binding:value:weak:` to Execution's receiver; a scoped target forwards
owned Method references to the GC visitor. No callback entry crosses that boundary
and no internal mutable borrow crosses a send. Lookup adaptation holds its selected
Method across nested lookup, replacement and collection before returning its
exact entry. Ordinary future sends perform current lookup.

## Machine and registration contract

| Receiver / selector | Operands | Result and ownership |
|---|---|---|
| Method `fromStep:descriptor:selection:owner:` | NativeEntry step, canonical FrameDescriptor-v2 Bytes, ArtifactSelection Bytes containing exactly one matching row, code/context owner Object | Owned resumable Method. Both byte spans are bounded and copied during the call; no Rust carrier crosses the boundary. The owner identity names the generation. |
| Method `fromStep:descriptor:selection:owner:implementation:` | Same executable/descriptor/selection/code-owner operands, plus implementation Object | Owned named-adapter Method. The descriptor must declare an implementation slot; its separate implementation is traced and copied into that slot on entry. |
| Method instance `withImplementation:` | Implementation Object; receiver is an owned resumable Method with an implementation slot | Fresh Method and registration sharing the selected code owner. New Method identity names its generation. Copies admitted shape, resets derived plans, derives the new implementation's native signature, and does not inherit old optimizations or retain a chain of old bindings. See [qualification](../../docs/design/method-binding-constructor.md). |
| Method instance `withInline:at:owner:` | Candidate Method Oop, Integer baseline return slot, candidate owner Object | New owned Method preserving the baseline code and an optional inline candidate. See recovery contract below. |
| Method instance `optimizeWith:` | Sequence of plain native Methods, one per baseline PC; empty sequence removes the candidate | Same Method. Attach candidate metadata to the shared baseline registration; see hot selection below. |
| Machine `recoverAt:with:` / `recoverAt:withGraph:` | Integer baseline PC, recipe sequence Object | Machine Object after one atomic pending-call recovery; typed failure for missing/stale authority or invalid recipe. |
| Block `fromMethod:captures:` | Method Object, capture sequence Object | Block with `value` / `value:…` matching descriptor arity. Reads captures through current `size`/`at:`; validates descriptor capture count. Clone copies this owned representation shallowly. |
| Block `fromMethod:codeObject:captures:` | Method, admitted CodeObject, capture sequence | As above, retaining the supplied CodeObject. Native Word parameter/capture counts are queried from the current CodeObject owner; counts must agree with a resumable descriptor. For native Methods the admitted program supplies arity. |
| Block `fromMethod:captureOperands:` | Native variadic Object operands: admitted Method, then zero or more captures | Copies the ordered references directly into owned Block storage. All inputs are scoped before dependency sends; no intermediate ObjectModel container. Wrong carriers/counts or descriptor mismatch fail before publication. |
| Block instance `codeObject`, `captureCount`, `captureAt:` | none, none, zero-based Integer index | Supplied CodeObject, capture count, captured Object (or nil outside bounds). All return nil for Blocks without a CodeObject. Clone preserves these references. |
| Block `captureAt:of:` | Integer index, Block | Owner projection used by synchronous generated code; reads the same owned captures through a send. |
| Machine `new` | none | New Machine and its own scratch actions, allocated through Id. |
| Machine `runBlock:` / `runBlock:budget:` | Block or any receiver implementing `value`; optional Integer transition budget | Ordinary current `value` dispatch through the retained run adapter. Result, suspended Machine, captured Continuation, or runnable Machine at a committed budget stop. |
| Machine `resumeWith:` / `resumeWith:budget:` | supplied Object, optional Integer budget | Resume a waiting result-bearing edge. Reject runnable and completed Machines. |
| Machine `continueWithBudget:` | Integer budget | Continue a runnable Machine, without inventing a waiting result. |
| Machine `restoreContinuation:with:` | Continuation Object, supplied Object | Reconnects the former host `restore_stackless_continuation` operation: restore into its original suspended Machine, commit its pending edge and drive. Wrong-Machine admission does not consume it; successful restoration is one-shot. |
| Machine `destroy` | none | Abandon runnable, suspended, completed or failed state and release frame pins. No asynchronous task cleanup runs. |
| Machine `oopAt:` / `oopAt:put:` | Integer slot; optional Object value | Checked access to its current committed frame. |
| Machine `continueAt:`, `callSite:`, `tailCallSite:`, `returnAt:` | Integer descriptor PC/site/slot | Publish the existing checked StepAction. |
| Machine `tryPrepareCall:slots:selector:in:tail:` | Word descriptor site, Bytes little-endian u32 source slots (receiver followed by up to 16 arguments), Symbol selector, Namespace Object, Word tail flag | Optional guarded call-window preparation. Unit refuses without frame writes; success publishes the ordinary checked Call/TailCall action. |
| Machine `tryPrepareCall:slots:copies:selector:in:tail:` | Same call arguments with an additional Bytes copy list: up to 32 little-endian u32 source/destination pairs | The same guarded admission includes ordered local copies. All sources are checked before any write; copy aliases are resolved in order and every original slot is written before the normal action is published. |
| Machine `suspendAt:continueAt:` / `captureAt:continueAt:` | Integer result slot and PC | Publish waiting/capture actions for the current delimiter. |
| Machine `restore:with:` / `abortAt:` | Integer continuation/value slots or reason slot | Existing low-level action publications; distinct from host continuation restoration. |
| Machine `protectedBlock:`, `leaveBoundary:`, `park`, `controlEligible` | Existing control-adapter arguments | Retained resumable native adapters, not language scheduling policy. Execution loading installs these completed adapters once; repeated loading and new Machines preserve replacements. There is no separate preparation hook. |
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

Block's canonical prototype owns its shared behavior through Object Model's
`ownerOf:put:` protocol. Reflect method replacement therefore reaches existing
and future compiled Blocks. Bootstrap installs the separate [Block Loop
module](../../docs/design/authored-small-modules.md#block-loops) after Lang; later stackless controls
replace its authored synchronous entries through normal MOP publication.

A direct native call to a Block preserves its body's result, including a native
failure return. Language Machines convert failed native sends into Error data at
pending-call completion. Block entry does not insert another answer continuation
or convert failures on behalf of a native caller.

`Execution evaluatePreservingFailures:` retains the explicit scoped policy for
evaluating language code with native failures propagated unchanged. The previous
policy is restored on success or failure; ordinary language evaluation still
converts failed native sends into Errors.

## Remaining migration and acceptance

The canonical frame engine, boundaries, scheduling, tail-entry and recovery
algorithms have moved out of Runtime. The obsolete all-Oop wrappers in those
files are deleted. The pure tail layout plan now belongs here; Codegen temporarily
shares that private source without becoming an Execution dependency.

The aggregate Runtime crate and unused dispatch-cache helper have been removed.
Execution retains their frame, scheduling, boundary, recovery and code-lifetime
mechanics; production tests exercise those mechanics through current Id messages.
TCC owns mappings, Store owns tracked mutation, and Id owns dictionaries and
collection-visible allocation. No aggregate Runtime construction remains.

The retired allocation-journal, deferred-identity and raw-layout optimizer glue
was discarded under the [revised optimization contract](../../docs/design/90-functional-modules.md#optimization-preserves-language-behavior-not-allocation-history).
Reusable closure, numeric-chain, region, virtual-aggregate and recovery analyses
remain in Codegen. They do not establish support for the retired optimizers.
The current discarded-clone and numeric-predicate producers are described below.

Generation publication/rollback remains Codegen/Store integration work. Image,
workers and Host/CLI already use the production prefix. Native control adapters
remain separate from the Store-backed Birnam TaskControl/TaskExecutor policy.

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
owned Method preserving that baseline code: candidate Method Oop,
Integer baseline return-slot, Object candidate owner. Captures snapshot this
choice. The candidate runs only for an eligible non-tail, unprotected registered
Call, with Machine as receiver and the original receiver followed by arguments
as Oop arguments. Non-null answers commit an inline result; a null Oop declines
to the baseline. A candidate can instead consume its scoped request through
Machine `recoverAt:with:` or `recoverAt:withGraph:` and then answer a null Oop.
Rejection or error releases the acquisition; a consumed request transfers it to the frame.
No recovery authority or Rust structure is passed through messages.

Recipe sequences retain their existing size/at: encoding; graph construction
uses Integer, Float and ObjectModel messages. Admission is rechecked after
callbacks and before publication. The graph validator is a private pure
algorithm within Execution. Native fixtures supply executable candidates now; compiler construction and emitted candidates remain
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

Execution checks its installed Machine slot/control Methods with Id's existing
`nativeBindingsMatch:` message. Id validates current default lookup and the current
bindings together, without callbacks between observations. GC `referenceOwnerOf:`
takes a vtable Oop and answers its current inherited owner Oop or null. These
observations stay with their owners; Execution reads no foreign payload.
Changing Machine lookup or an observed control method declines the candidate and runs the ordinary baseline. The candidate's
native author remains responsible for semantic guards on its body and preserving
baseline-observable operations; this constructor does not certify optimization.

The Oop C fixture exercises direct result, decline, both recovery forms, balanced
reentry and rejection at a native suspension boundary. The current native test
also covers collection between graph nodes, layout/reference-owner rejection,
custom lookup, duplicate consumption, invalid recipes, exact numeric identity/bits,
and candidate data retained only by the returned Method. Full migration of the
older compiler and lifetime harnesses remains separate work.

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
were removed in the [optimizer reassessment](../../docs/archive/design/90-optimizer-reassessment.md).
Explicit native candidates still use this guarded contract. Their guards protect
entries that can bypass Machine operations, and entered Methods retain their code.
The 64-visit threshold is a retained local policy, not a language requirement or a
mandate for future producers. Allocation-elision's old accounting/identity protocols
are not requirements to recreate.

Hot synchronous unary calls ask the live Integer owner to evaluate their exact
selected function/context through `evaluate:receiver:argument:with:`. Integer
recognizes only its own arithmetic implementations, validates operands and current
ErrorVt, and sends `numericOperation:left:right:` to the Execution receiver. Unknown entries decline
before operand reads and execute the originally selected entry. Integer owns
mixed-number handling, ordinary errors and result construction through current
factories. Execution caches only bounded scalar hotness, never dependency entries
or foreign numeric layouts. The pure arithmetic operator is shared privately with
Codegen; there is no Execution dependency on compiler construction.

Production tests cross both hot gates, replace controls and arithmetic, withdraw
an entered candidate during a collecting callback, and observe owner reclamation.
They prove numeric receiver sends under precise GC, arithmetic failure and
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


## Numeric predicate

`Method withNumericPredicate:then:left:right:limit:return:in:` takes two Symbol
selectors, three native Word argument indexes, Word return-slot index and
Namespace Object. The returned captured Method traces this metadata and its
baseline owner. Codegen supplies the body proof for one arithmetic call, one
comparison and a final return. Extra intermediate bindings and captures decline.

On an eligible unbounded call, Execution sends Integer
`evaluatePredicate:then:left:right:limit:in:epoch:with:` with the selectors,
actual input Objects, compilation Namespace, checked epoch and the Execution
receiver. Integer sends it `commitNumericPredicate:` before releasing roots.
That method uses the existing pending-call record's target and authority, commits
the return, releases the acquisition and marks the request consumed. Inactive or
repeated commits fail. The scalar evaluator similarly receives
`numericOperation:left:right:` through fresh dispatch. A refusal leaves the ordinary call pending;
bounded, tail and protected calls use the baseline. No new virtual Frame or
identity representation is introduced. See [scope and tests](../../docs/design/90-numeric-predicate.md).

### Capture borrowing admission

`binding_proof.rs` sends one canonical Id `nativeBindingsMatch:` query for capture
borrowing and Word-helper reader admission. It checks the current reader/grant and
five lookup/query service meanings, including opaque context equality. Unknown or
changed services decline; there is no reentrant separate-query fallback. Inputs
remain in the existing Execution scopes and actual reads remain Id messages.
The former outbound-ABI proof batching toggle and helper guard path are removed.

## Compact closure construction (2026-10-07)

Generated synchronous and resumable literals use `fromMethod:codeObject:captureOperands:`.
The native operand count determines capture count, independent of the selector's
colon count. This uses the existing Id ABI; it adds no floor API or foreign layout.
Language/materialization callers still use their live capture-sequence constructor.
`imageShellFromMethod:codeObject:` creates pending nil captures directly in this
owner, and restoration continues through the existing owner protocol.

CodeObject's current `parameterCountOf:` and `captureCountOf:` messages return
native Words from admitted immutable state. Blocks no longer export/reparse the
portable program to obtain shape. The obsolete `referenceSlots:` constructor,
compiler-only `ObjectModel allocateReferenceSlots:`, copied/borrowed shape parser
and `programShapeBorrowing:` diagnostic switch are removed. Existing compiled
artifacts containing those removed calls must be explicitly rebuilt; no shim is
installed and image startup does not silently recompile supplied machine code.

## Direct capture reads (2026-10-07)

`captureWordAt:` receives a native Word index on a Block and answers the captured
Object (nil for an invalid index). It is an ordinary Id send reading only this
owner's storage, with no allocation admission. The factory `captureWord:of:`
remains for inspection/type fallback and also needs no allocation admission.

`block captureReadMode` returns native Word 0 for
ordinary protected reads, 1 for canonical pure borrowing, or 2 for an unsupported
nonportable receiver. Generated synchronous bodies query and read through the
actual Block receiver without a Namespace factory lookup. If the direct query is
unbound, the current factory `captureReadModeOf:` query supplies inspection/type
fallback and still checks the current direct reader for initialized Blocks. Mode
2 uses `captureWord:of:` inspection. Other query failures propagate unchanged.
Mode 1 requires an atomic current-binding check
of the reader, the mode query and canonical Id query services (including that
check's own binding). It covers only the initialized Block's immutable capture
references while the caller keeps the Block live. Custom/replaced readers use
normal result roots; native grant/query providers must honor their contracts.
No selector lookup or arbitrary callback occurs between positive admission and
these pure projections. Word-helper admission tracks the new direct reader.

See [measurement and proof limits](../../docs/design/capture-receiver-query.md).

## Fixed one-argument native calls (2026-10-07)

`optionalWordResultOf:selector:argument:resultFactory:` sends one Object argument
and wraps a native Word through the supplied factory's current `fromBits:`; native
Unit answers language nil. `objectResultOf:selector:wordArgument:` obtains native
bits through the argument's current `bits` message, sends one native Word argument,
and accepts an Object result (nullable Object becomes language nil).

All source-facing operands are Objects. These methods scope inputs before any
callback; they perform no signature-string or argument-
container permission work. Target/getter/factory lookup and failures stay live
through ordinary Id messages. The general outbound adapter has been retired; the fixed Byte/discard forms
below cover its remaining production consumers. See [the measured slice](../../docs/design/fixed-abi-simplification.md).

## Outbound bridge retirement

All production outbound calls now use fixed shapes. Additional messages are
`discardResultOf:selector:wordArgument:`, `objectResultOf:selector:byteArgument:`
and `bytesResultOf:selector:resultFactory:`. Word/Bytes getters, actual targets
and supplied result factories use current Id messages. Byte arguments are copied
before the target callback; byte results are copied before the factory callback.

The runtime signature interpreter, arbitrary argument-count loop, Vector/string
permission helpers, `tryNativeArguments:` and the compiler's literal-ABI elision
pass are removed. The old general selector is not installed. Incoming authored
method signatures and Alien foreign-function signatures remain distinct live
interfaces. Existing old compiled outbound calls require explicit rebuilding.
See [retirement and evidence](../../docs/design/outbound-bridge-retirement.md).

## Method code admission (2026-10-07)

`Method admitCodeObject:` obtains checked counts through current CodeObject-owner
messages and binds the code/counts once, validating a resumable descriptor if one
exists. Same-code admission is idempotent; different-code rebinding fails. Failed
admission publishes no binding. Method GC references and captured aliases retain
the admitted CodeObject.

Generated literals use `Block fromMethod:captureOperands:` with a bound Method and
zero or more ordered Object captures. No per-literal CodeObject/count query occurs.
The retired generated constructor accepting Method plus CodeObject is removed.
Collection-based external code pairings and image shells validate a different
supplied code and preserve its identity. All actual construction/invocation still
uses Id; per-instance behaviors remain unchanged. See
[measurement and contracts](../../docs/design/method-code-admission.md).

## Bulk capture handoff

`tryBorrowCaptureOperands:` accepts a native Word count and returns native Unit
(decline) or native Bytes of ordered native-endian opaque Oop identities. The
owner admits only initialized Blocks with enough slots and current canonical
bulk/mode/reader/Id-query bindings. It reads only Execution-owned storage, with no
callback after admission, and uses a private reused byte buffer. Copy it before
another Id call. The live receiver's immutable edges must cover every returned
Object throughout consumption; successful custom providers owe the same contract
and must decline arbitrary fresh references. Later queries may overwrite the
buffer, not the caller's copied references. Unit/unbound uses the existing
protected reader path; other failures propagate. No foreign payload reads or
capture containers are introduced. See [contract and proof limits](../../docs/design/bulk-capture-handoff.md).

## Guarded generated call preparation

The native backend can omit transient Integer indices while copying a call's
receiver/arguments into the descriptor's existing call window. Execution checks
its current read/write/call entries, both compiler and runtime Namespace lookups
and Integer bindings, and Integer's existing `canOmitIndexing` permission. A final
behavior epoch and factory-binding fence follows all guard callbacks; a frame
handle/PC check prevents committing into a different activation. Overlapping
source/window slots decline. Input borrows are copied before callbacks and all
Objects remain rooted. No foreign payload is inspected or foreign entry invoked.

Admission precedes every write. On refusal, generated code performs the original
ordered reads, index construction, writes and action publication at that point;
it does not restart the source method or replay prior effects. Successful
preparation keeps every logical frame, source slot, continuation edge and budget
stop. The driver still selects the actual target through current Id lookup.
There is no cached permission, virtual source value or deferred materialization.
The compiler first checks optional-protocol presence, so a failure (including
UNBOUND) from an entered implementation propagates rather than causing replay.

The compiler's copy extension admits only a contiguous suffix of local bindings
immediately before an ordinary/tail call. It never moves bindings across literal
construction, a global lookup, another send or persistent top-level publication.
The existing read/write/call and allocation guards cover the complete batch;
refusal executes the original copies and call preparation once. No source value,
Block, logical frame, budget stop or continuation is removed. A 729-case oracle
compares copy planning with sequential storage, including chains, overwrites and
self-copies. The native and image tests also exercise withdrawal of either
optional protocol, current replacement and collecting fallback.


## Admitted generated call preparation

The current Execution factory supports `fromStep:descriptor:selection:owner:callPlans:`.
The historical `implementation:` variant remains pending migration. The extra Bytes input contains little-endian u32
words: version 1, site count, then one record per descriptor site: source PC,
source count, copy count, receiver/argument source slots, and ordered
(source,destination) pairs. It is copied before callbacks and fully admitted
before the Method is published. Plans are derived immutable execution data,
cloned with bindings and regenerated on portable materialization; they do not
change canonical descriptor or image formats.

`Machine prepareCall:selector:` accepts an immediate Word Oop site and selector.
The default body checks the current PC against the admitted site, performs ordered
copies through current `oopAt:`/`oopAt:put:` messages, fills the call window and
sends current `callSite:` or `tailCallSite:`. No engine borrow spans these sends;
collecting and authored replacements remain visible. Source method lookup still
occurs in the driver. Preparation is itself replaceable, and an entered failure
is not retried through a second preparation path.

`Machine callPlan:field:` takes Integer site/field indices and returns Integer
metadata for the current site. Fields 0–5 are receiver destination, selector
destination, argument start, argument count, tail flag and copy count. The next
1+argument-count fields are source slots; the remaining fields are ordered
copy pairs. Invalid fields/sites/PCs fail without frame writes. This inspection
surface supports the [authored implementation fixture](../../tests/fixtures/execution/prepared-call-reference.birnam).
It executes the target transfer in Birnam, while the test's metacircular adapter
uses native preparation for its own ABI/helper calls. See the
[measurement and scope notes](../../docs/design/runtime-simplification-results.md).
