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
| GC `trace:with:` | behavior Object, owner Object or Unit | Unit. Attach/replace an inherited reference enumerator; Unit removes the exact attachment. The owner receives `referencesOf:do:` with the ordinary object or behavior and a synchronous NativeEntry visitor. |
| GC `referenceOwnerOf:` | behavior Object | Current inherited reference owner Object, or Unit if absent. Validates behavior membership and registration identity. Execution graph recovery uses this to require ObjectModel-owned field storage. |
| GC `finalize:with:` | object Object, owner Object or Unit | Unit. Attach/replace/remove a finalizer. The owner receives `finalize:` with the detached object while its payload is still readable. |
| GC `prepareAllocation` | none | Unit. Enforce the construction phase and budget before an upper mutation, even if it ultimately reuses an object. Caller retains inputs. |
| GC `registerRetirement:` | owner Object | Unit. Register an instance-owned metadata owner implementing `retire:`; duplicates are ignored. |
| GC `budget:` | nonzero Word bytes | Unit. Change the automatic collection threshold while idle. Accounting includes the header and padded payload. |
| GC `rootsDo:` | NativeEntry visitor | Unit. Enumerate GC's own objects, selectors, callback owners and temporary construction roots. Registered with Roots during construction. |
| GC `roots` | none | The Roots Object used by this instance. |
| Id `memoryManager` | none | This GC Object. Installed by GC, not implemented by Id. |

An enumerator sends its visitor one Object operand per reference (null is allowed)
and returns Unit. Emit valid allocated Oops, not integers or interior pointers.
The entry and its context are borrowed for this synchronous invocation only;
retaining or invoking it afterwards violates the native contract. An enumerator
that fails, or ignores a visitor failure, aborts the marking pass without sweeping.
No foreign registry borrow spans a send or visitor invocation.

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
finalizer, sends `finalize:`, then invokes the
one-use release entry transferred by Id. An earlier finalizer may inspect a later
still-attached dead object or replace its finalizer. Finalizers cannot resurrect
dead objects, allocate, suspend, or escape the payload. A release entry is ownership
of that one allocation; it is not a saved service implementation. A failure after
retirement begins makes the collector terminally failed and leaves unreclaimed
storage intact; normal language execution must not resume on that heap.

## Scope and checks

Private marking, phases, accounting, hook resolution, finalizer records and native
capture replace the former Collect/Lifetime/Trace integration and the Native Mark,
Native Stack, Trace Hooks and Finalizers helper libraries. GC depends only on the
common Id ABI. Diagnostic described-graph tracing remains separate pending its
own storage integration.

Run `mise run native-messages`. Its tests build the actual production dylibs and
load through Siphon and Id, exercising Bootstrap activation, the same heap across
failed/successful activation, live dispatch, LIFO traversal, failed-trace safety,
weak retirement, finalizer replacement, native roots and automatic collection.

Execution's suspended roots, deferred materialization, code pins and safe code
retirement algorithms remain in Runtime awaiting their production message
integration. They are not a dependency of this prefix and are not validated by
these tests. Image and worker restoration remain upper-module work. No replacement
execution/retirement framework is introduced here.
