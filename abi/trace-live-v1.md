# Live GC traversal v1

The production collector mounts the live traversal exported by
`libbirnam_trace.dylib`. This is distinct from the scoped diagnostic
[described-graph tracer](trace-v1.md): production root providers and hooks run
at their existing collection boundary and may use ordinary Id reads/mutations
allowed by that boundary. They do not run under storage-observation exclusion.

The [independent C header](fixtures/trace-live-v1/trace_live_v1.h) mirrors
[`birnam-abi::trace_live`](../rust/birnam-abi/src/trace_live.rs). The three
carriers are `SourceV1` (48 bytes), `SinkV1` (16 bytes), and `ApiV1` (24 bytes)
on supported arm64 macOS. Source and API carry version 1 and exact byte sizes.
The original extraction used IdApi v4; current production uses IdApi v20.
This traversal ABI does not change Method, Frame or image bytes.

## Mounting

The tracer exports:

```c
const TraceLiveApiV1 *birnam_trace_provider_v1(void);
int birnam_trace_live_v1(const TraceLiveSourceV1 *source);
```

Id exports `birnam_trace_dependency_v1(const TraceLiveApiV1 *) -> int`.
Current production constructs the [native graph](trace-graph-v1.md) before
Runtime and adopts its traversal entry with the completed graph. Later mounting
repeats this attachment idempotently before mounting the collection coordinator.
Legacy embedders attach the provider explicitly. The canonical tracer module must publish a
provider; missing or rejected dependencies fail startup. Successful image
handles remain pinned for process lifetime.

Each Runtime copies one validated traversal entry. Reattaching that same entry
while idle is idempotent; changing entries or mounting during collection is
rejected. The table is immutable native implementation selection, not a mutable
ordinary method dictionary. It need only remain readable during mounting; its
code must stay pinned for the Runtime lifetime. The normal Id library has no
link dependency on the tracer. Rust unit tests select the real implementation
through a dev-dependency.

Custom native embedders must perform the same mount before a collection can
occur. Bare Id initialization still works without it, and collection without a
native stack anchor retains its existing no-op behavior. An actual collection
without a mounted tracer aborts with an invariant error; there is no fallback
worklist inside Id. The current independent IdApi client uses
[`trace-support.h`](../scripts/fixtures/trace-support.h) to mount explicitly.

## Source and traversal semantics

The source supplies an opaque context and three non-null callbacks:

- `roots(context, sink) -> status`: emit initial candidates in source order.
- `admit(context, address) -> int`: immediately perform exact membership and
  first-mark admission. Return 1 only once per trace for a newly marked address,
  or 0 for a rejected/already marked address. Other values are invalid.
- `edges(context, address, sink) -> status`: enumerate the next reached object's
  outgoing candidates, then run its current hook under source-defined rules.

The tracer calls `roots` once. Each emission synchronously calls `admit` and
pushes newly admitted addresses. It pops the most recently admitted address,
calls `edges`, and repeats until the worklist is empty. Duplicate suppression
happens on emission, not on pop. The source owns marks and their reset; it must
enforce first admission to terminate cycles. The tracer never reads object
payloads or makes a collection-policy decision.

The production source preserves this order: native stack/register candidates;
cache invalidation; core roots; registered providers in registration order;
then each popped object's behavior, strong vtable edges or selected payload
words, and legacy hook. Current layout and hook lookup occurs at the object's
turn, so earlier hooks can change later scans. Hooks on unreached objects never
run. Permanent payloads remain excluded by admission; permanent dependencies
still enter through existing core-root enumeration. Precise layouts and
conservative exact-address payload scans keep their prior policy.

## Borrowing, nesting and errors

Source code/context and storage stay live on the owner thread throughout the
synchronous call. Roots and edges may emit only during their own invocation.
They must release conflicting heap/state borrows before emission because
admission mutates source marks. Admission itself performs only the membership
and first-mark operation; it must not emit or invoke arbitrary callbacks.
All callbacks must return normally without unwinding across the ABI.

Sinks use globally non-reused invocation tokens, not borrowed context addresses.
Expired, cross-invocation and cross-thread sink calls return `BUSY` (2). Nested
live traversal on the same thread also returns `BUSY`. Source implementations
must not use these statuses to hide a missing edge. Errors from valid emissions
are latched, so ignoring a failed admission cannot produce success.

`OK=0` means the walk completed. Malformed carriers or admission results return
`INVALID=1`. Roots/edge callback errors, including negative values, propagate.
Per-call buffers and the active slot are released on every normal/error return.
Failure can leave source marks partially changed; it **never** authorizes sweep.
The production collector treats any traversal failure as a fatal invariant
before metadata retirement, detachment or finalization.

Id retains native stack capture, storage membership/marks, root and payload
policy, metadata operations and backing storage. The separately loaded
[collection coordinator](collect-v1.md) controls phase order, settlement,
retirement, finalizer invocation and reclamation.
The tracer owns all production traversal/worklist state. The separate diagnostic
report's coverage gaps are never used as permission to collect.

`mise run live-tracing` runs the focused debug dylib/IdApi fixtures. Rust checks
are `cargo test -p birnam-trace` and the Id GC/Frame/retirement tests. No release
or installation validation is required.
