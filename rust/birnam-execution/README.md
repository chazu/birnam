# Execution over the production Id heap

Design 90 batch 1 now runs native Methods, Blocks and Machines over the production
prefix, including frames, bounded drives, suspension, continuations and native
control boundaries. There is no compiler dependency or alternate heap. The pure
artifact codec is a private implementation dependency. This is a native execution
checkpoint; the remaining migration work below still keeps batch 1 open.

## Traced producers and consumers

The retained implementation establishes these boundaries:

| Source | Existing responsibility and migration |
|---|---|
| Runtime `runtime/method.rs` | Exact `NativeEntry` capture; `invokeOn:` and `invokeOn:arguments:`; `methodFor:` and `addMethod:as:`. The old function-address owner registry and Lifetime `set_edge` are replaced by tracing actual Id dictionary bindings to owned Methods. |
| Runtime `legacy_method.rs` | Language lookup returns Method/nil, while Id consumes NativeEntry/Unit. Keep that adaptation with Execution and retain both function and context; discard the all-Oop calling-convention bridge. |
| Runtime `execution.rs`, `execution/`, `runtime/scheduling.rs` | Preserve frame arenas, generation leases, acquired-entry and frame pins, bounded drives, native barriers, suspension and continuations. The engine and its unit tests now live in this crate. `machine.rs` connects the retained transitions to Id; `control.rs` reconnects the native Block adapters. Compiler-dependent selection remains with the pending compiler migration. |
| Runtime `runtime.rs` drive/recovery and retirement | Selected entries remain pinned across nested calls, waiting frames and recovery. End a managed turn before draining releases, keep the outward result rooted, and never perform asynchronous cleanup from a finalizer. Preserve these mechanics when moving the drive. |
| Runtime `registration.rs` | Validate descriptor bytes, arity, digest, manifest agreement and prepare/abort claims. Keep the codecs and admission algorithm private. Do not send the old Rust/C registration structure through Bytes. The native descriptor constructor below feeds the retained canonical validator and frame registry directly. Portable-profile admission and generated recovery publication still need their real batch 2 producers. |
| TCC `rt_owned_method`, `rt_register_method_owner`, `release_payload`, `release_retired_unit` | TCC owns mappings. Replace registration-then-install with construction of an owned Method followed by `addMethod:as:`. Replace cached retirement exports with owner messages after the existing Execution checkpoint admits release. This is batch 2 integration, not permission to drop the retained code now. |
| Codegen `lib.rs`, `portable_c.rs`, `resumable_registration_c.rs` | Generated method/Block installation and resumable registration currently emit the old TCC symbols. Convert both producers and consumers to NativeEntry and owner Objects in batch 2; retain guarded fallback, materialization and recovery. |

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
reuse and queues resident-image pins for the next safe checkpoint. Active calls
and partially read argument sequences are rooted through Execution `rootsDo:`.
Each synchronous callback borrows its NativeEntry visitor only until return;
no internal mutable borrow crosses a send. Lookup adaptation holds its selected
Method across nested lookup, replacement and collection before returning its
exact entry. Ordinary future sends perform current lookup.

## Machine and registration contract

| Receiver / selector | Operands | Result and ownership |
|---|---|---|
| Method `fromStep:descriptor:selection:owner:` | NativeEntry step, canonical FrameDescriptor-v2 Bytes, ArtifactSelection Bytes containing exactly one matching row, code/context owner Object | Owned resumable Method. Both byte spans are bounded and copied during the call; no Rust carrier crosses the boundary. The owner identity names the generation. |
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

Batch 1 remains open for the mixed Runtime consumer cleanup and execution-time
optimization/recovery reconnection described in Design 90. The old
`runtime/method.rs`, drive/task-control wrappers and compiler-dependent native-tier
consumers still reference the retired aggregate Runtime and are not loaded by the
production prefix. Their remaining callers must move before removing them; this
checkpoint does not claim that deletion obligation is closed. Preserved private
recovery code and its previous compiler probes are not proof of production
generated recovery. Batch 2 must supply generated-code registration, portable
admission, guarded recovery and real compiled-mapping reclamation.

The Store-backed Birnam TaskControl/TaskExecutor policy, image/worker integration
and full CLI startup remain deferred. The native control adapters here do not
substitute a new scheduler for that source policy.

## Validation

`mise exec -- cargo test --offline -p birnam-execution --lib` runs the 18
engine/planner tests: canonical admission, frame validation, parallel-copy/root
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
