# Collection coordination v1

`libbirnam_collect.dylib` exports `birnam_collect_v1(source, out_count)` and
`birnam_collect_provider_v1()`. Its native entry constructs complete
[collection state](collection-state-v1.md), including the coordinator, before
Runtime. Runtime adopts that same coordinator at construction. The later legacy
mount through Id's `birnam_collect_dependency_v1(api)` verifies the identical
entry; it does not supply a missing production dependency. Legacy `id_init`
clients still require that mount. Missing/rejected dependencies fail startup,
and actual collection without a coordinator aborts. Bare Id initialization and
collection without a native stack anchor retain their prior behavior. There is
no production implementation link or fallback in Id.

The [independent C mirror](fixtures/collect-v1/collect_v1.h) describes
[`birnam-abi::collect`](../rust/birnam-abi/src/collect.rs). Version is 1; carriers
require exact sizes: Source 120, API 24, dead view 16, detached view 16 bytes on
arm64 macOS. IdApi v4 remains 45 fields / 360 bytes. Method, Frame and durable
image formats are unchanged.

## Ordered effects

The source carries an opaque context and twelve non-null owner operations.
They are scoped services, not an expanded permanent IdApi. The coordinator
copies the carrier and executes this complete sequence:

1. `phase(MARKING=1)`, `clear_marks`, `trace`. Nonzero trace status stops here.
2. `inspect_retirement`: snapshot dead binding records and return exactly 0 or
   1 for whether deferred settlement is required. The owner retains the exact
   existing predicate, including short-circuiting and String/Machine exemptions.
3. `settle_deferred` only when inspection answered 1.
4. `retire_metadata` using that same captured binding list. The current owner
   retires executable references, dead vtables/catalog entries and enabled weak
   indexes in their existing order. No dead storage is reusable yet.
5. `snapshot_dead` returns a borrowed address array in existing heap order.
6. `phase(FINALIZING=2)`, including an empty array.
7. For each address: `detach`, invoke its returned current finalizer if present,
   then `reclaim` that same address. Finalizers are never prefetched for later
   objects and dead storage is never detached or reclaimed as a batch.
8. Add `reclaim_deferred` to the physical reclaimed count; `clear_marks`,
   `reset_accounting`, `phase(IDLE=0)`, `prune`; publish the final count.

The source owns its copied binding/address snapshots and at most one detached
block. An address serves as the release token only for its outstanding detached
block within this synchronous invocation. The payload remains valid through
finalization while membership already excludes it. Later dead objects stay
registered and retain mutable finalizers until their own detachment. Reclaim
consumes the sole detached owner. Private Rust allocation types never cross ABI.

Native stack capture and pending executable-generation retirement precede entry
into the driver, preserving the native scan range. Mark-only paths bypass this
coordinator and use the [live tracer](trace-live-v1.md) directly. The floor phase
guard still enforces allocation exclusion and nested public GC answering zero.

## Lifetime and failure

This is an unsafe native implementation contract, not a sandbox or an ordinary
replaceable method protocol. Source/context, borrowed snapshots, output storage
and callback code stay valid for the complete synchronous owner-thread call.
The address array must stay unchanged until return. Callbacks cannot escape,
unwind, switch threads, suspend collection or call operations out of sequence.
No conflicting Rust storage/metadata borrow may cross a foreign callback.

The mounted entry is copied per Runtime. Mounting is allowed only while idle;
reattaching the identical function is idempotent, replacement is rejected. Its
code stays pinned under existing native-module lifetime rules. The provider
table need only remain readable during mounting. The module retains no IdApi or
source state; its only execution guard is a thread-local active bit.

`OK=0` writes the completed reclaimed count. `INVALID=1` rejects malformed
carriers/results; `BUSY=2` rejects nested direct coordinator entry. Trace,
detach and reclaim errors propagate unchanged. Required callback geometry is
checked before any effects. Every error leaves the output count untouched and
releases the module's active guard, but does not roll back owner effects or
authorize continuing a partly completed collection. Id treats any nonzero
result as a fatal invariant. In particular a failed trace never retires metadata,
detaches storage or invokes finalizers.

Run `mise run collection` for the focused debug native probe and
`scripts/collection-image-smoke.sh PREFIX` for startup, compiled module, saved
compiled image and baked-image checks. The former observes the actual loaded
driver with an independent C ordering oracle. No release validation is required.
