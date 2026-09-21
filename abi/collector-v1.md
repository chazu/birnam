# Collector v1

Historical public descriptor; its layout remains the scoped base interface used
by [Collector v2 extensions](collector-v2.md). New production collection owners
publish v2. The description below records the original per-call contract.

Collect now implements heap tracing and collection using completed Id storage,
Lifetime, Object Model, Trace and collection-state owners. It needs no Runtime,
Machine or compiler. The six-argument native constructor returns
[collection state v2](collection-state-v2.md), including this 40-byte capability.
No additional library or future provider slot is introduced.

`trace(context, input)` marks through the live Trace graph; it does not clear
existing marks or establish a new collection phase. Its caller owns that scope. `collect(context,
input, out_count)` performs the production coordination sequence: clear marks,
trace, settle upper deferred work when required, retire metadata and Id indexes,
detach one allocation at a time, invoke its current finalizer, release its storage,
reclaim upper deferred storage, reset accounting and return to Idle. Lifetime's
finalizer entry and the mark record are removed before the callback; Id storage
remains readable until finalization returns and the detached release entry runs.
No Id registry borrow crosses an upper callback.

The collector selects `allocationsDo:`, `retireIndexesFor:` and
`detachAllocationAt:` through ordinary Id lookup at construction. It retains those
exact entries. Before tracing it reconciles allocations made since the previous
record snapshot, preserving existing flags and charging only missing records.
Native Stack scans the supplied interval; extra roots follow, then permanent Id
edges and the completed graph's roots. Precision is read for each object's edges,
preserving changes made by earlier native root providers or hooks.

The 40-byte input supplies the captured native interval and an optional scoped
root callback. An empty interval is valid for trusted native callers that supply
all roots explicitly. Such callers must keep every owned reference live; this
is not a language-level way to bypass native stack roots.

A caller with upper retirement obligations also supplies the complete 48-byte
retirement table: inspect, settle, retire, reclaim and prune with a live context.
The collector copies it for this call and retains nothing afterward. Before
Execution exists there are no such records, so the table can be absent. Once
Execution exists its roots and retirement operations are mandatory for collection.
Runtime supplies those complete scoped services, including current compatibility
roots, dispatch-cache invalidation, deferred materialization and executable-owner
bookkeeping. Physical allocation facts, marking, Lifetime/Object Model retirement,
finalizer selection and release are implemented by Collect itself.

This is a trusted native collection capability. It does not implement the
remaining disabled/adopting/active startup transition or GC-aware construction
wrappers. The caller must establish the required roots, descriptions, collection
phase and native boundary before using it. Errors preserve the output count;
a nested collection returns BUSY. An error after effects begin is fatal to the
enclosing execution and does not authorize retry or partial continuation. In
particular a failed trace never retires metadata or reclaims storage.

The [independent C mirror](fixtures/collector-v1/collector_v1.h) fixes the layouts.
The native loading harness runs the actual collector with only its twenty-two
predecessor libraries present, then exercises the same production collector after
Runtime adoption. A separate fresh process verifies trace failure stops before
retirement and reclamation. Existing collection/image workloads cover the upper
retirement callbacks, deferred accounting and executable lifetimes.
