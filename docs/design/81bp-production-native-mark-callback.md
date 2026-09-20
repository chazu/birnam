# 81BP — Native mark-callback adaptation

> **Status:** Production extraction complete after 81BO (`60af88b2`).
> Debug validation passed; release validation remains waived.

[81D](81d-id-api-partition.md) places native tracing integration above Id.
After [81BO](81bo-production-native-stack-anchor.md), `gc.rs` still owns the
thread-local bridge from the legacy `MarkFn(Oop)` callback to the active tracer's
`SinkV1(context, candidate)`. Extract that bridge into `birnam-native-mark`.
This moves active-sink state and emission policy; no IdApi field changes in this step.

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

The immutable 56-byte Native Mark v1 descriptor supplies create/destroy and
begin/end/mark operations over an opaque owned scope. The module allocates one
small native scope per floor/thread on first use and destroys it on thread exit;
this does not allocate Id objects, change GC counters or trigger collection.
Begin/end/mark allocate nothing. The host pins its library and attaches the
descriptor before Id initialization. Validation is inert, repeating the same
descriptor is accepted and retargeting is rejected.

Id retains a thread-local ownership handle and the context-free MarkFn shim.
The shim selects that loaded floor's own thread-local scope and forwards to the
module's mark operation. A shared module-wide TLS slot would incorrectly merge
separately loaded floors: a saved callback from one floor could select another
floor's active sink. Separate owned scopes preserve this boundary while moving
all active-sink state, admission, emission and failure policy into the module.
Runtime instances within one loaded floor still share their thread's scope.

Id also keeps the Rust RAII guard around its existing closure call. The guard
stores the selected context/end operation and clears the scope both on return
and on a Rust unwind. Moving the closure behind a C callback would change
panic/unwind behavior and is excluded. No Rust closure, trait object, Runtime or
private heap state crosses the boundary. The sink/context remain live through
the synchronous scope. No mutable state reference survives any foreign call.

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
including independent loaded-floor callback identity, normal callback order,
duplicates and null marks, nested collection, escaped callbacks, wrong-thread
use and sink rejection where observable.

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

## Completed implementation and evidence

Native Mark owns the copied active sink, scope admission, immediate emission
and exact fail-stop policy. Id retains a floor-local thread ownership handle,
context-free callback shim and unwind guard. The first shared-TLS draft was
replaced during review because it could merge independent floor callback scopes;
the final design was verified with two separately loaded Id binaries sharing the
same upper modules.

The completed 81BO source at `60af88b2` and its installed debug prefix were
preserved before migration. Validation against the final production source:

- ABI, Id and Native Mark library tests: 289 passed, with 15 existing Id tests
  ignored. Tests include copied sink/context, immediate reentrant emission,
  null and duplicate marks, independent owners, simultaneous thread scopes,
  later-scope callback selection, exact fatal diagnostics and Rust unwind reset.
  Workspace build and all-target checks passed.
- Seven independent old/new native consumer transcripts match exact exit status,
  stdout and stderr: normal, escaped null, escaped value, wrong thread, nested
  adapter entry, sink rejection and callback use from another loaded floor.
  The normal path exercises both root providers and trace hooks with an
  independent emission-order observer and nested `gcCollect` suppression.
- The focused Native Mark gate and adjacent live-tracing, Root Registry and
  Trace Hooks gates passed, including the v20 C client, the loaded tracer's
  admission/LIFO oracle, failure before sweep, callback-time provider append,
  and PEG/Store/Portable Code/TCC hook consumers.
- All 13 actual host preparation cases passed: missing library/provider,
  null/malformed descriptors, each missing operation, valid startup, missing
  floor dependency and rejected attachment. Preparation validates without
  entering Id initialization on those failure paths.
- The installed image smoke passed dynamic startup, source-free compiled module
  execution, mapped/rebuilt saved native Code images and baked execution. Actual
  root providers and hooks exercise the adapter, including null/duplicate marks,
  nested collection, aliases and module reload. `RTLD_NOLOAD` confirms that the
  host had already loaded Native Mark, and `dladdr` confirms module ownership of
  its mark operation and the floor's ownership of the context-free shim.
- Both old and new image-backed workers completed 12 exact wire round trips and
  orderly shutdown; their transcripts match byte for byte. The external debug
  harness allows boot time without changing the production handshake deadline.
- Installation contains 51 native libraries and 28 compiled modules. All 79
  export inventories and 35 native manifests match the declared changes; Id and
  Native Mark link only system libraries. All 478 current build inputs and 515
  published artifact hashes match the debug receipt.

Capture/scanning, collection, tracing, source probes, Runtime, Native Loader and
IdApi v20 are unchanged from 81BO. Native initialization still takes twelve
arguments and images remain v8. Local evidence is in `.cargo-target/81bp-*.log`;
`scripts/native-mark-test.sh` and `scripts/native-mark-image-smoke.sh PREFIX`
provide repeatable focused and installed checks.
