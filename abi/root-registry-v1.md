# Native root registry v1

Design [81BJ](../docs/design/81bj-production-root-provider-registry.md) extracts
only the ordered native root-provider list, append policy and copied snapshots.
The [independent C mirror](fixtures/root-registry-v1/root_registry_v1.h) defines
four arm64 layouts: public capability 32 bytes, copied floor 40 bytes, immutable
factory 48 bytes, and owned snapshot 264 bytes.

Id loads `libbirnam_root_registry.dylib` through its common native entry.
The initializer accepts initial Object roots and returns a completed
[root owner](fixtures/root-registry-owner-v1/README.md), including its public
registration capability and immutable factory. Bootstrap adopts its own retained
results into this owner. Runtime later adopts the same owner and supplies its
completed admission callbacks. No preparer, provider export or dependency setter
is involved, and Id does not link the service implementation.

Public registration copies its selected floor context, performs the existing
owner/observation admission, and appends. Duplicates occupy separate entries.
A 33rd entry fails through the floor's nonreturning overflow callback with the
existing `gc: too many root providers` diagnostic. Admission can reenter; inner
registration precedes the outer append, including at the capacity boundary.
No registry borrow crosses admission. No provider is invoked during registration.

The factory's canonical append returns 1 on success and 0 at capacity, without
callbacks. Its snapshot returns count and 32 optional function pointers by value;
only the first count entries are populated. Internal append, snapshot and destroy
use the original owned address regardless of public descriptor instrumentation.
Runtime retains its admission policy and calls snapshot at the existing tracing
and storage-observation points. Live GC invokes the copied providers in order;
callback-time appends wait for a future snapshot. Storage diagnostics publishes
the addresses without invoking them and keeps its existing coverage flags.

Destruction calls no providers. The provider list does not own callback code; registered callbacks remain live.
The native owner also holds explicit Object roots, independently of this list. Stack capture, collector
settings, mark adaptation, collection phases, weak identity and finalization
remain with their existing owners. There is no unregister or deduplication.

Run `scripts/root-registry-test.sh PREFIX`, or
`scripts/runtime-services-test.sh PREFIX` for the combined Root Registry,
GC Settings and Native Stack tests. The independent C oracle receives completed
services from production startup and checks ordered/duplicate registration,
callback-time appends, nested collection, snapshots, overflow and admission.
Its factory probe loads a separate native root owner through Id and destroys it
through that owner's factory. Diagnostic observation runs after the loading
message has returned, at an idle host boundary.
