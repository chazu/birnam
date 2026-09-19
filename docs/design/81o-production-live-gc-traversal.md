# 81O — Production GC uses the loaded tracer

> **Status:** Implemented. Follows the scoped graph tracer in checkpoint
> `988a1bc6`. This extraction moves actual automatic and explicit collection
> traversal; retirement and physical reclamation remain in Id.

## Boundary

[81N](81n-production-storage-tracer.md) compared a scoped described graph, a
live streaming source and a persistent tracing session. The scoped graph was
selected for the diagnostic service. Its declared coverage cannot replace
production GC: native roots are omitted, and production legacy hooks can use Id
or change the later traversal. Eager snapshots would change those semantics.

This checkpoint implements the previously reviewed live streaming design.
`birnam-trace` owns the worklist and traversal. Id supplies ordered root
candidates, exact-address first-mark admission and the outgoing references of
one reached object. Neither private Rust heap types nor a second object registry
crosses that boundary. Both live and described-graph implementations depend only
on the no-state ABI crate.

The source callbacks preserve immediate first marking and LIFO traversal.
Native stack/register candidates precede core roots and registered providers.
Each reached object's header is processed before payload/vtable references and
its hook. Layout and hook selection stay live until that object's turn. A hook
may therefore alter how a later object is traced, while unreachable hooks remain
uninvoked. Permanent payloads, precise layouts, conservative candidates, weak
Symbols and strong default-vtable owner edges keep their existing treatment.

The old `gc::Marker` type and worklist are removed from Id. Automatic allocation
collections, explicit `gcCollect`, collection during generation retirement and
mark-only unit checks use the same mounted implementation. Heap-only Symbol and
GC tests also use the real upper tracer through a dev-dependency; there is no
production fallback or duplicate test traversal.

The host mounts an immutable versioned traversal capability after loading the
tracer and before Integer or other allocating native modules. Runtime-local
binding copies the entry and rejects replacement or mounting during collection.
Code stays pinned through existing native module lifetime rules. A missing
canonical provider fails startup; attempting actual GC without the dependency
aborts. Bare Id initialization remains possible. IdApi v4 and durable formats are
unchanged. The native mount protocol is documented in
[live traversal v1](../../abi/trace-live-v1.md).

## Borrowing and lifetime

The floor's root and edge adapters release state/heap borrows before entering the
tracer's candidate sink. The sink immediately reenters Id only for its pure
membership/mark operation. Machine, Continuation and scoped execution-root hooks
copy their ordered roots before invoking foreign admission, so no immutable
RuntimeState/Heap reference crosses a mark write. Legacy native hooks still run
with the same ordinary runtime access as before.

The tracer holds no mutable worklist borrow across source callbacks. Its active
slot is thread-local, with globally non-reused tokens for each roots/edges sink
invocation. Escaped sinks cannot write a later traversal's queue. Recursive
admission and ignored emission errors cannot silently succeed. Per-call state
is released on success or callback failure. Id aborts on any failed traversal
before weak-index retirement, storage detachment or finalizers; partial marks
never authorize sweep.

Native root capture remains outside the collector's frames, so the new callback
frames/worklist cannot turn their own transient addresses into extra roots.
Deferred logical allocations remain unmaterialized during tracing. Their prior
settlement rule still runs after marking when observable retirement/finalizers
require physical identities.

## Focused evidence

The existing Id suite passes: 245 tests, no failures, 16 ignored. Those cases
exercise native roots, weak Symbol retirement, default-vtable owners,
Machine/Continuation and Frame roots, allocation-triggered collection, deferred
allocation settlement, finalizers and executable generation lifetime.

A new deterministic floor test records provider/hook order. An earlier hook
replaces a later object's layout and hook; the loaded walk uses both new values,
reaches the newly declared child and never executes an unreachable hook. Interior
pointer candidates and nested collection keep their existing behavior.

The tracer unit tests cover first-mark/LIFO order, duplicate and invalid
candidates, nested traversal, ignored admission failures, expired/cross-thread
sinks, callback errors and carrier geometry. Existing described-graph oracle
tests continue to pass.

The independent C fixture loads the actual Id and tracer dylibs and mounts an
observer around the published live entry. An independent event-order stack checks
every actual first admission and pop. Both explicit and allocation-triggered GC
use that entry. Native-stack-only roots survive, legacy hooks can read Id, and
precise/conservative modes retain their behavior. Subprocesses verify rejection
of missing dependencies and failure before sweep; a deliberately unmarked victim
would report any accidental finalization through a pipe. The current independent IdApi
v4 client also passes with the explicit mount.

Run `mise run live-tracing`. Validation is focused/debug only, with no release or
installation gate and no performance claim.

Final checks also pass for all eight tracer tests, four ABI tests, the new floor
ordering regression and focused Machine/Continuation/generation-root tests.
ABI/tracer Clippy passes with warnings denied; the host compiles. The normal Id
and tracer dylibs match their exact export manifests (28 and five symbols), link
only the system library, and have no implementation link to each other.

## Next extraction

Id still coordinates collection phases, retirement of weak and executable
metadata, conditional deferred settlement, finalization and physical reclamation.
Moving that coordinator is the next collector extraction. It must preserve
retirement-before-address-reuse and native/code lifetime rules; the isolated 81I
collector's simpler pinned-object policy is insufficient by itself. The live
source adapters still reside with their current owners until those owners are
split out. The 81M/81N observation API retains its explicit coverage gaps.
