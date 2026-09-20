# 81BP — Native mark-callback adaptation

> **Status:** Audited next extraction after completed 81BO (`60af88b2`).
> Production migration pending. Preserve behavior; debug validation only.

[81D](81d-id-api-partition.md) places native tracing integration above Id.
After [81BO](81bo-production-native-stack-anchor.md), `gc.rs` still owns the
thread-local bridge from the legacy `MarkFn(Oop)` callback to the active tracer's
`SinkV1(context, candidate)`. Extract that bridge into `birnam-native-mark`.
This moves actual state and callback code; no IdApi field changes in this step.

## Ownership and exact behavior

The existing thread-local slot holds one copied `SinkV1` per loaded implementation
and thread. Both registered root providers and native trace hooks use it. Preserve
this shared scope across Runtime instances on the same thread; it must not become
per-Runtime state or a stack that admits nested marking.

Entering a scope rejects an already-active slot before copying the selected sink.
While active, the callback first obtains that copied sink, ignores a null object,
and otherwise immediately emits the exact pointer bits. It does not deduplicate,
check heap membership, enqueue, allocate, collect or root anything itself. The
tracer retains all reachability and first-mark decisions.

The sink selection is copied once per callback scope, including its context.
Emitting may synchronously call the same mark callback again; the adapter must
hold no TLS/state borrow across emission. A retained callback used outside any
scope aborts, including a null mark. Used during a later valid scope, that same
function selects the later sink, as it does today. It is not a revocable token.
Different threads have independent active slots, so foreign-thread use without
a scope fails through the existing escaped-callback diagnostic.

Preserve the exact fail-stop diagnostics and abort status:

- `birnam-id invariant failed: GC mark callback reentered`
- `birnam-id invariant failed: GC mark callback escaped its invocation`
- `birnam-id invariant failed: GC tracer rejected a candidate`

These strings retain their historical prefix despite the new code owner.
The collector still suppresses nested `gcCollect` before entering this adapter.
Root-provider snapshots, callback ordering, live hook lookup, layout selection,
weak Symbols, finalizers and retirement ordering remain in their existing owners.

## Boundary and lifetime

The immutable Native Mark v1 descriptor has version, byte size, `begin(sink)`
returning `MarkFn`, and `end()`. It is a 32-byte native implementation dependency,
not a language object or another public aggregate API. The host pins its library
and attaches the descriptor before Id initialization. Validation is inert,
repeating the same descriptor is accepted and retargeting is rejected.

The module owns TLS, begin/end and the returned callback. Id keeps a small Rust
RAII guard around its existing closure call. The guard stores the selected end
operation and clears the scope both on return and on a Rust unwind. Moving the
closure itself behind a C callback would change panic/unwind behavior and is
therefore excluded. No Rust closure, trait object, Runtime or private heap state
crosses the boundary. The sink and its context remain live through the entire
synchronous scope. No mutable state reference survives any foreign call.

Physical register capture, scan bounds and scanner stay byte-for-byte unchanged.
The bridge runs below the captured native-root interval, as before. Source-probe
outer ranges and collection adapters stay in Id. Native Stack's anchor ownership
is separate from this callback lifetime.

Siphon mounts Native Mark as a regular native module. Its initializer introduces
no methods, language objects or additional initializer arguments. IdApi remains
v20 (18 fields / 144 bytes), ModuleInit remains twelve arguments, Native Loader
remains v12 and images remain v8. Host/installer inventories and standalone
fixture preparation must explicitly include the new library.

## Independent proof and completion gates

Archive completed 81BO source and its debug prefix before migration. Compare
actual root-provider and trace-hook consumers against that independent baseline,
including normal callback order, duplicates and null marks, nested collection,
escaped callbacks, wrong-thread use and sink rejection where observable.

Unit checks exercise copied sink selection, immediate emission and reentrant
emission, independent thread slots, later-scope selection by a saved callback,
nested-begin rejection, escaped null/non-null marks and sink rejection. The Id
RAII check must demonstrate reset after an ordinary Rust closure unwind; proving
only the module's begin/end operations would miss this requirement.

Run focused tracer/root/hook checks and Id/ABI unit tests. Verify dynamic startup,
source-free compiled calling, mapped/rebuilt saved native Code images and baked
execution with actual provider and hook calls. Compare the real image-backed
worker path. Check exports, system-only linkage, install inventory and source/
artifact receipts. Record completed evidence before committing the extraction.
