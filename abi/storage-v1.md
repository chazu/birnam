# Production storage observation v1

Id exports `birnam_id_observe_storage_v1(const StorageRequestV1 *) -> int`.
It requires the owner thread, no active dispatch/Machine scope, no collection,
no staged publication and no pending retirement. It grants one observation permit
and sends the current `withStorageView:` method to Vtable. That method is
replaceable; a wrapper can call its saved default during the same send. A direct
ordinary send without the host permit, a nested host entry or a nested dispatch
returns `BUSY` without invoking the observer. IdApi remains v4, 45 fields/360 bytes.

The request and visitor code/data must survive ordinary lookup and the complete
synchronous call. Use existing native root discipline if lookup can allocate.
The default method consumes the permit, copies storage facts, ends its runtime
borrows, then invokes the visitor under observation exclusion. It performs no
Birnam allocation, marking, settlement, finalization or release. Native scratch
allocation is allowed. Normal and error returns restore access. Status values
are `OK=0`, `INVALID=1`, `BUSY=2`, `INCOMPLETE=3`; visitor errors propagate.

During the visitor/provider extent, use the supplied view and provider-owned
native state. Id entry points, including captured direct mutators, are prohibited
and use the existing fatal-invariant behavior because their ABI often has no
recoverable failure result. The observation host entry itself returns `BUSY` on
recursive entry. Raw payload readers do not enforce this phase; native callers
must not write payloads or other observed state. This is not a memory sandbox.

The C-layout definitions are independently mirrored in
[storage_v1.h](fixtures/storage-v1/storage_v1.h); the Rust declarations are in
[`birnam-abi::storage`](../rust/birnam-abi/src/storage.rs).

## Storage facts and scope

`StorageViewV1` contains runtime/observation identities and callback-scoped arrays:

- Physical allocation records: exact Oop, monotonically assigned allocation
  identity, requested extent, padded extent, charged block bytes, current behavior,
  permanence, core-payload classification, finalizer presence and legacy hook.
- Core roots selected by the same enumeration used by production GC, including
  compatibility-only Symbol retention, Environment and registration roots.
- Core edges: object headers and registered-vtable parents, strong owners and
  strong selector keys. Weak opaque dictionary keys and Method addresses are
  not strong object edges. Permanent payloads remain outside ordinary GC scans.
- Registered legacy root-provider addresses, reported without invocation.
- Deferred logical allocation count and charged bytes, separate from physical
  Oops. Observation does not turn journal records into identities or trace their
  pointer-shaped payload bits.

Requested size is the actual request used by `Heap::allocate`. Existing clone
continues to request a padded copy, so its requested extent can differ from its
source. `objectPayloadSize`, object headers/payloads and accounting are unchanged.
Allocation identity never repeats within a Runtime, including after address
reuse. Its 59-bit counter fails before exhaustion; it shares the existing
metadata word with mark, permanence and padding bits. The four-word Block
metadata bound is unchanged. Runtime identities are scoped to the selected process-lifetime Id image;
handles cannot be used after their Runtime/module lifetime ends.

Coverage gaps explicitly name excluded native roots, uninvoked legacy root
providers/hooks and deferred identities. A current GC layout/hook is a policy
fact, not proof of representation ownership. Observation can succeed while the
view lacks enough information to trace or collect the production heap.

## Description validation above Id

The stateless `libbirnam_storage.dylib` loads after Id. It depends only on
`birnam-abi` and exports:

```c
int birnam_storage_describe_v1(
    const StorageViewV1 *view,
    const StorageDescribeRequestV1 *request);
```

Call it inside the storage visitor with that exact live view. The request lists
prepared provider function/context pairs and a result visitor. No provider lookup
occurs under exclusion. Each provider may emit roots and one complete description
per object it owns. Descriptions use `(runtime_identity, allocation_identity,
object)` plus the exact requested extent and byte offsets of strong Oop slots.
Other payload bytes are raw. Zero reference slots mean absence. The validator
copies offset arrays before the provider returns; all other inputs remain live
through the enclosing call.

Providers must know what they initialized. Current behavior, initial behavior,
a current GC layout or finding a new identity at a remembered address cannot
establish that ownership. Reassociation leaves a valid claim unchanged; a stale
allocation identity rejects a claim even at the same address. This version adds
no persistent description registry or finalizer ownership.

Validation rejects malformed/foreign identities, wrong extents, claims over core
payloads, multiple owners, misaligned/duplicate/out-of-bounds offsets and foreign
reference targets. Sink failures latch even if a provider ignores the status.
Sinks use non-reused invocation tokens: a copied sink cannot mutate a later
provider's state, and an expired sink returns `BUSY`. Never dereference expired
view/array pointers; their lifetime is a trusted native precondition.

The result contains combined core/provider roots, copied descriptions, payload
edges and missing-description count. Every nonempty noncore allocation needs a
description to clear `PAYLOAD_DESCRIPTIONS_MISSING`, including unreachable objects.
`reject_gaps` selects coverage gaps that return `INCOMPLETE` before the result
visitor runs. Other gaps remain visible. Supplied providers do not silently clear
legacy/native-root gaps. There is no reachability calculation or authorization
to reclaim storage in this interface.

Production-wide representation providers, native/root-hook integration, tracing
and reclamation are subsequent work. The fixture supplies independently known
Pair/Integer allocations and demonstrates explicit incomplete coverage for the
remaining production payloads.

Run focused debug checks with `scripts/storage-observation-test.sh` and the
`runtime::storage` / `birnam-storage` Rust tests. No release validation is needed
for this extraction.
