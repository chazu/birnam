# Execution over the production Id heap

Work in progress for Design 90 batch 1. The first converted function is owned
Methods: exact invocation, language lookup adaptation and binding reachability.
Machine/frame/Block integration is still required before batch 1 is complete.
There is no compiler dependency or alternate heap in this owner.

## Traced producers and consumers

The retained implementation establishes these boundaries:

| Source | Existing responsibility and migration |
|---|---|
| Runtime `runtime/method.rs` | Exact `NativeEntry` capture; `invokeOn:` and `invokeOn:arguments:`; `methodFor:` and `addMethod:as:`. The old function-address owner registry and Lifetime `set_edge` are replaced by tracing actual Id dictionary bindings to owned Methods. |
| Runtime `legacy_method.rs` | Language lookup returns Method/nil, while Id consumes NativeEntry/Unit. Keep that adaptation with Execution and retain both function and context; discard the all-Oop calling-convention bridge. |
| Runtime `execution.rs`, `execution/`, `runtime/scheduling.rs` | Preserve frame arenas, generation leases, acquired-entry and frame pins, bounded drives, native barriers, suspension and continuations. Their migration is pending; synchronous Methods are not Machine acceptance. |
| Runtime `runtime.rs` drive/recovery and retirement | Selected entries remain pinned across nested calls, waiting frames and recovery. End a managed turn before draining releases, keep the outward result rooted, and never perform asynchronous cleanup from a finalizer. Preserve these mechanics when moving the drive. |
| Runtime `registration.rs` | Validate descriptor bytes, arity, digest, manifest agreement and prepare/abort claims. Keep the codecs and admission algorithm private. Do not send the old Rust/C registration structure through Bytes. The typed registration messages must be settled with the actual frame consumer when it moves. |
| TCC `rt_owned_method`, `rt_register_method_owner`, `release_payload`, `release_retired_unit` | TCC owns mappings. Replace registration-then-install with construction of an owned Method followed by `addMethod:as:`. Replace cached retirement exports with owner messages after the existing Execution checkpoint admits release. This is batch 2 integration, not permission to drop the retained code now. |
| Codegen `lib.rs`, `portable_c.rs`, `resumable_registration_c.rs` | Generated method/Block installation and resumable registration currently emit the old TCC symbols. Convert both producers and consumers to NativeEntry and owner Objects in batch 2; retain guarded fallback, materialization and recovery. |

## Callable contract

Bootstrap loads `libbirnam_execution` with Namespace after the completed values
and collections. It publishes Execution, Method and MethodVt. State belongs to
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

## Remaining batch 1 acceptance

Move the retained Machine/frame/Block, continuation and recovery implementations
into this same owner; reconcile executable descriptor registration with their
actual readers. Preserve bounded drive versus waiting resumption, two-Machine
interleaving, critical-phase/native barriers, suspended roots and code pins.
Use the retained scheduling tests with production native executable fixtures.
Then prove entered/suspended replacement and final release through the existing
checkpoint. Do not substitute a new state machine or claim source task policy:
the latter still depends on Store. Actual compiled mapping reclamation and
compiler-generated deoptimization belong to batch 2.

The old Runtime Method code is still retained with the unconverted generation
lease/driver callers. Its removal is pending that migration; it is not loaded by
this production prefix. This initial checkpoint does not close the batch's
plumbing-deletion obligation.

## Validation

`mise run native-messages` audits the actual dylib exports and dependencies and
runs `execution_methods` with the existing prefix tests. The test includes an
independently compiled C native module loaded by Bootstrap. It proves binding-only
ownership, native replacement during invocation, exact captured Methods, distinct
native contexts through nested lookup/GC, argument survival and failure cleanup,
language result conversion, and owner reclamation after the last reference. It
does not claim actual compiled mapping reclamation or suspended-code acceptance.
