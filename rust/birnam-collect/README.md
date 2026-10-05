# Memory management over Id and Roots

`libbirnam_collect` initializes with one Roots Object and returns a GC Object.
Bootstrap loads it after Nil, publishes `GC` in Namespace, and sends `activate`.
All allocations remain in the existing Id heap. There is no second heap, native
service table, captured dependency method, or GC state in Id.

The instance owns phase, accounting, temporary construction roots, traversal,
reference-owner registrations and finalizers. It is owner-thread-affine. Id's
`memoryManager` message finds that instance for repeat loading; another Roots
object is rejected. This adds no process/TLS singleton. Existing Id/Roots module
singletons still limit this prefix to one domain per process.

## Messages

| Receiver / selector | Arguments | Result / contract |
|---|---|---|
| GC `activate` | none | Unit. Validate current roots, Id edges and reference coverage, account for the existing heap, then enable collection. Failure leaves collection disabled and permits construction/registration followed by retry. Repeating successful activation is harmless. |
| GC `collect` | none | Word reclaimed count. Capture the current native caller's registers and stack, trace declared roots, then retire and reclaim unreachable allocations. |
| GC `collectPrecisely` | none | Same collection, with **only declared roots**. Native callers must explicitly retain every live Oop. This is not the old language-level precise-payload switch and is not the ordinary allocation path. |
| GC `trace:with:` | behavior Object, owner Object or Unit | Unit. Attach/replace an inherited reference enumerator; Unit removes the exact attachment. The owner receives `referencesOf:do:` with the ordinary object or behavior and a visitor Object. |
| GC `referenceOwnerOf:` | behavior Object | Current inherited reference owner Object, or Unit if absent. Validates behavior membership and registration identity. Execution graph recovery uses this to require ObjectModel-owned field storage. |
| GC `finalize:with:` | object Object, owner Object or Unit | Unit. Attach/replace/remove a finalizer. The owner receives `finalize:` with the detached object while its payload is still readable. |
| GC `canOmitAllocationFor:` | behavior Object | Word 1 only for this GC's current ordinary allocation path while idle, with Id default storage operations and unchanged lookup/epoch; Word 0 otherwise. No charge, reservation or collection. |
| GC `prepareAllocation` | none | Unit. Enforce the construction phase and budget before an upper mutation, even if it ultimately reuses an object. Caller retains inputs. |
| GC `registerRetirement:` | owner Object | Unit. Register an instance-owned metadata owner implementing `retire:`; duplicates are ignored. |
| GC `automaticCollection:` | Word flag (zero disables, nonzero enables) | Unit. Assign the automatic collection setting; no collection or accounting reset. Initially enabled. |
| GC `precisePayloads:` | Word flag | Unit. Assign payload precision, initially disabled. May change during tracing/finalization. |
| GC `precisePayloads` | none | Word 0/1. Owners query before scanning each object; no collection/allocation. |
| GC `budget:` | nonzero Word bytes | Unit. Change the automatic collection threshold while idle. Accounting includes the header and padded payload. |
| GC `allocationBytes` | none | Word accumulated allocation charge. Readable during tracing/finalization; not a live-storage count. |
| GC `rootsDo:` | Visitor Object | Unit. Enumerate GC's own objects, selectors, callback owners and temporary construction roots. Registered with Roots during construction. |
| GC `roots` | none | The Roots Object used by this instance. |
| Id `memoryManager` | none | This GC Object. Installed by GC, not implemented by Id. |
| Id `allocationChargeFor:` | Word requested payload bytes | Word total storage charge, or Unit if the size cannot be allocated. Id owns header/padding/layout geometry; the query allocates nothing and has no GC dependency. |

An enumerator sends `visit:` to the supplied Object for every reference (null is
allowed). Each send performs current Id lookup. Valid references are exact
allocated Oops, never integers or interior pointers. Enumeration is synchronous:
providers must not retain the visitor for later use and must propagate failed
sends. The GC receiver handles marking and allocation inventory; one additional
receiver buffers Id's core edges to preserve ordering. Both exist before
activation and are rooted. Their handlers reject use outside the active pass;
there is no per-edge language allocation or native callback argument. Validation
failures are sticky even if a provider ignores them. No registry borrow spans a
send; marking failure prevents sweeping.

| Id selector | Message contract |
|---|---|
| `rootsDo:` | Visitor Object; send `visit:` with each root. |
| `referencesOf:do:` | Object, visitor Object; send `visit:` for each core edge and answer native Word kind (0 ordinary, 1 behavior, 2 Symbol), or Unit for an unknown object. |
| `allocationsDo:` | Visitor Object; send `allocation:bytes:identity:` with Object, payload-size Word, allocation-identity Word. Enumeration uses a snapshot. |
| `bindingsIn:do:` | Behavior, visitor Object; send `binding:value:weak:` with selector Object, typed binding operand, weak-key Word. Execution forwards its owned Method edges through `visit:`. |
| `detachAllocationAt:` | Address Word; remove membership and answer its unique allocation-identity Word as a release handle, or Unit when absent. Id retains the storage privately. |
| `releaseDetachedAllocation:` | Handle Word; release exactly that detached storage and answer Unit. Invalid/already-released handles fail; identities are never reused. |

NativeEntry visitor operands are rejected. Native entries remain the format for
installing native method implementations in Id; they are not GC callbacks.

Id enumerates its own headers, behaviors, dictionary keys and Symbols. GC never
reads those layouts. For behavior objects, GC admits header and parent, then sends
the upper enumerator (when installed), then admits dictionary keys. Object Model
uses this to enumerate behavior-owner links. Every ordinary nonempty native payload needs an enumeration
owner before activation or before being reached by collection. That owner knows
its own payload and native state, including which words are references. A zero-byte
ordinary object needs an owner only if it has additional references. The existing
Roots/Namespace/Bootstrap/Boolean/Nil prefix declares its private references through
Roots. Object Model owns ordinary slot metadata; it is not a GC service or a shared map
of all native representations.

## Construction and collection phases

Activation installs complete implementations of behavior `allocate`, `delegated`,
and Id `intern:`. They apply the phase check, retain construction inputs, optionally
collect, then send Id's non-collecting storage messages:

- Id `allocate:bytes:` takes a behavior Object and payload-size Word.
- Id `newBehaviorWithParent:` takes the existing native behavior-address Word.
- Id `internBytes:` takes borrowed Bytes; GC copies input bytes before any send.

These operations are ordinary fresh sends, so replacing an Id implementation is
observable. They are trusted storage primitives, not an alternative language
allocation policy. Native consumers use the guarded construction messages; directly
invoking storage primitives must obey the same phase/lifetime contract. Id needs
no collector callback, dependency table or knowledge of upper modules. Interning an
existing Symbol remains permitted during collection because it does not allocate.

## Allocation accounting and admission

Activation accounts for the existing Id heap. Each subsequent guarded construction
charges its actual requested size through current Id `allocationChargeFor:`; GC
does not duplicate Id's storage geometry. The object stays temporarily rooted
across this query. A replacement may collect or construct another object, so the
charge adds to the current total after the query returns. Freeing an allocation
does not refund accumulated work. Only a successful collection resets the total;
failed activation does not publish a partial inventory charge, and failed tracing
does not reset it.

Committed charges use checked addition and preserve the fatal diagnostic
`birnam-id invariant failed: GC allocation byte counter overflow`. When automatic collection is enabled, the existing
construction/`prepareAllocation` threshold check compares the accumulated charge
with the budget privately. The speculative public `canAllocate:` preview was
removed because no production consumer needed it. There is no separate Allocation
Budget module, descriptor, setter or native service API.

Under Design 90's
[optimization contract](../../docs/design/90-functional-modules.md#optimization-preserves-language-behavior-not-allocation-history),
only actual construction is charged. Eliminated allocations require no synthetic
charges, deferred identities or dead-payload journal. Recovery construction uses
ordinary accounting. Optimization may change allocation diagnostics, automatic
collection timing and reclaimed counts; the actual heap's root and finalization
obligations remain.

Marking and finalization reject guarded allocation, delegation and new Symbols.
Nested collection returns zero. The default native capture retains the existing
Darwin arm64 register-save frame and stack bounds. Conservative native words are
admitted only when they identify an exact allocation. Object edges use immediate
first-mark admission and a LIFO worklist; hooks are selected through current parent
and owner messages at each object's turn. No dependency entry is saved for later
calls. Selector identities may be retained.

Attachments weakly name targets, fence them with allocation identity, and strongly
retain callback owners. A tracing callback can change a later object's owner;
attaching an owner during marking admits it immediately. During finalization,
a replacement owner must already be marked: callbacks cannot resurrect an owner
from the dead snapshot. New attachment/removal does not hold a map borrow across
any dependency call.

After successful marking, dead allocations are snapshotted in allocation identity
order. GC first sends each registered metadata owner `retire:` with the dead-address
Bytes, then retires Id's Symbol, behavior and weak-key indexes and GC's dead trace
attachments before any storage can be reused. Registration is permitted only while
idle/inactive; GC roots the registered owner objects and holds no registry borrow
across their retirement messages. For each dead object, GC detaches the allocation, selects its **current**
finalizer, sends `finalize:`, then sends Id `releaseDetachedAllocation:` with the
handle returned by detachment. An earlier finalizer may inspect a later attached
dead object or replace its finalizer. Finalizers cannot resurrect dead objects,
allocate, suspend, or escape the payload. Id's detached-storage map is private
allocation ownership, with no GC policy or foreign layout exposed. Failure after
retirement begins makes GC terminally failed and leaves unreclaimed storage
intact; normal language execution must not resume on that heap.

## Scope and checks

Private marking, phases, accounting, hook resolution, finalizer records and native
capture replace the former Collect/Lifetime/Trace integration and the Native Mark,
Native Stack, Trace Hooks and Finalizers helper libraries. GC depends only on the
common Id ABI. Diagnostic described-graph tracing is owned separately by
[Storage](../birnam-storage/README.md) and [Trace](../birnam-trace/README.md);
it does not authorize collection.

Run `mise run native-messages`. Its tests build the actual production dylibs and
load through Siphon and Id, exercising Bootstrap activation, the same heap across
failed/successful activation, live dispatch, LIFO traversal, failed-trace safety,
weak retirement, finalizer replacement, native roots and automatic collection.
Tests also replace `visit:` between two emissions, propagate exact failed visitor
results without sweeping, reject inactive/native-entry visitors and repeated
release, and retain detached payloads after a failed release send.

Execution's suspended roots, deferred materialization, code pins and safe code
retirement algorithms remain in Runtime awaiting their production message
integration. They are not a dependency of this prefix and are not validated by
these tests. Image and worker restoration remain upper-module work. No replacement
execution/retirement framework is introduced here.


The discarded-clone consumer uses `canOmitAllocationFor:` as a semantic query,
not a budget preview. It checks this owner's actual `allocate` and
`prepareAllocation` entries and asks Id `usesDefaultAllocation` for Id-owned
facts. It does not promise a collection schedule or authorize a later use after
callbacks/mutation. Execution fences the complete admission and commits directly;
see [the consumer](../../docs/design/90-discarded-clone.md).

System's existing GC controls now use these messages; see [System GC](../../docs/design/authored-small-modules.md#system-gc).
Payload precision does not change native-root capture. Reference owners query the
current precision flag by send; conservative candidates are filtered through Id
`allocationAt:` before sending `visit:`. GC never decodes another owner's payload.
The default threshold remains one MiB. Turning automatic collection back on does
not collect until a subsequent allocation admission; explicit collection is
available while it is off.
