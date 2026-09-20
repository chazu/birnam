# 81BN — Native finalizer registration

> **Status:** Audited against completed 81BM (`67373979`). Implementation pending.
> Debug validation only.

[81D](81d-id-api-partition.md) places finalizer registration in the reclamation
protocol above Id. Extract that registration policy and its inline callback
attachment into `birnam-finalizers`. Collect already owns the detach/call/reclaim
sequence from [81P](81p-production-collection-coordinator.md); that sequence stays
there. Id keeps exact allocation membership, permanence facts, detachment and
raw storage reclamation.

## Existing laws

`Runtime::set_finalizer` performs ordinary owner-thread/storage admission first.
It calls `Heap::set_finalizer` and discards the Boolean result. Exact live
collectable allocation starts accept a replacement callback. Null, unregistered,
interior and permanent addresses are silently ignored after admission. Their
payload/header must not be dereferenced before membership is known. The setter
takes a nonnull function pointer; no public clearing operation exists.

Each heap record stores an optional callback. Allocation starts with none;
shallow cloning does not copy the original's finalizer. Replacing a callback
neither invokes the old callback nor changes collection accounting or object
identity. Registration adds no object root and does not extend callback-code
lifetime. Presence checks distinguish missing records from present records with
no callback. Storage observation and recovery/optimization refusal checks retain
their existing meanings and read points.

Registration remains allowed during collection, including finalization. Collect
snapshots dead addresses, then detaches one object, copies its current callback,
invokes it without a heap/RuntimeState borrow, and reclaims that object. Later
dead objects remain registered until their turn. Thus a callback can replace a
later object's finalizer, but registering again on the already detached current
object is ignored. Do not snapshot all finalizers together with dead addresses.
Nested collection and allocation retain their existing collection-phase rules.

`Heap::drop` deallocates harness/runtime storage without invoking finalizers.
The production Runtime is process-owned. Native String construction installs the
plan's callback directly through the canonical heap path after initialization;
it must remain independent of the replaceable public setter. String cleanup and
native callable ownership are not moved by this extraction.

## Selected boundary

Use an eight-byte C slot containing the optional Finalizer pointer. The module
owns the existing eligibility decision: construct a populated attachment for a
collectable record and an empty, non-applicable result for a permanent record.
An empty result means leave the attachment unchanged, not clear it. Id supplies
the record's permanence fact and applies accepted attachments to that exact
record. Missing membership is reported as a fact before reading any header.
Internal construction uses this same canonical operation and retains the
existing Boolean result where Native Allocation requires successful attachment.

The immutable factory provides capability creation/destruction and canonical
attachment construction. The public capability exposes only
`set(context, object, finalizer)`. Its copied floor descriptor supplies ordinary
admission, an exact permanence reader with an explicit missing-record result,
and an exact attachment writer. After admission the module reads the fact,
applies its policy, and writes only an accepted slot. These fact/attachment
callbacks do not allocate, collect or invoke user code. No RuntimeState borrow
crosses public admission. Canonical construction is inert with respect to Id and
can run within the existing native-allocation construction scope.

Id retains the attachment with its allocation; no second object-address map,
per-record allocation, root list, callback queue or ownership token is added.
Detached storage carries the same optional function pointer to Collect. Public
descriptor instrumentation cannot redirect internal native allocation or the
callback already selected by detachment. The capability/factory image stays
pinned until all retaining state is gone. Repeat factory attachment accepts the
same pointer and rejects retargeting before Id initialization.

This moves the permanent-record rejection and attachment construction out of Id;
a module that merely forwards to the entire old Id setter would leave the
responsibility behind. Collect's execution order and the payload owners' actual
cleanup implementations remain unchanged.

## Callers and timing

| Caller | Required selection and registration point |
|---|---|
| Id Machine and Continuation allocation | Canonical attachment after the existing object initialization, with no public capability lookup |
| Native Allocation / String | Canonical attachment after `prepare`, allocation, behavior assignment and `initialize`; retain the required-success invariant and the existing collecting checkpoint before construction |
| PEG Pattern, Grammar and CST allocation | Matching first saved execution capability, after each payload initialization |
| TCC `disposable` | Matching saved execution capability; preserve rejection of invalid/installed/disposable/released units and change the Unit phase before registration |
| StringBuilder construction | Matching saved execution capability after the empty native buffer payload is initialized |
| Portable Code program/statement/node construction | Matching saved execution capability at each existing ownership transfer, retaining temporary roots and failure cleanup |
| Isolate handle construction | Matching saved execution capability after token registration, allocation and payload initialization; retain failed-allocation worker termination |
| Alien buffer allocation | Match its first API/capability tuple; a different API remains an error. Preserve malloc/zeroing, ForeignAddress allocation, failure free and explicit-free idempotence |
| Codegen Runtime mapped image owner | Match its saved execution capability; register only after successful staged-unit commit, at the existing point before publishing committed stage state |
| Prelude / Native Loader | Retain and forward the first execution capability on later native initialization |
| Collect and storage/recovery consumers | Read the current inline attachment at their old call points; retain detachment, callback and reclamation order |

Remove only `setFinalizer` from IdApi: v19 will have 19 fields / 152 bytes.
Native initialization gains a twelfth explicit capability; Native Loader becomes
v12. Early value preparation and Native Allocation's existing finalizer-bearing
plan need no new argument. Keep Finalizer's signature and the existing Collect
detached carrier, object header, heap-record footprint and image v8 format.
Migrate current callers and fixtures together; keep historical contracts frozen.
Consumers must rebuild. No API-address lookup or callback-slot loan is introduced.

## Adversarial checks and completion evidence

Archive the completed 81BM source and installed debug prefix before coding.
Compare old/new independent native transcripts for exact collectables, permanent
objects, null/interior/foreign addresses, replacement, cloning, callback presence
and detachment. Compare owner-thread and storage-observation diagnostics with an
invalid target too, proving that admission still precedes ignored membership.

Test canonical policy and inline layout independently. Instrument the public
descriptor and prove that internal Machine/Continuation and native String
construction still attach cleanup through the canonical operation. Cover moved
and independent Runtimes, weak allocation lifetime, registration on the current
detached object, replacement on a later dead object, and teardown without
callback execution. Do not accept callback-count-only tests that miss ordering
or double cleanup of native owners.

Use distinct incoming and saved selections for every production caller. Alien
must still reject API replacement; same-API reinitialization must keep its first
capability while observing live callback/context changes there. Exercise actual
String/StringBuilder, PEG/CST, TCC disposable units, portable owners, Alien
buffers and Isolate handles. Preserve ordered finalizer replacement, fail-stop
behavior and nesting in the existing loaded Collect oracle.

Verify installed startup, source-free compiled callers, native Code image reload,
mapped and rebuilt owners, baked execution and worker replies. Check dynamic
symbol ownership, system-only links, export deltas, the current ABI manifest and
source/artifact agreement. Commit this extraction only after these checks pass;
then select the next responsibility. Do not run release gates.
