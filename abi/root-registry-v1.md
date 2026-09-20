# Native root registry v1

Design [81BJ](../docs/design/81bj-production-root-provider-registry.md) extracts
only the ordered native root-provider list, append policy and copied snapshots.
The [independent C mirror](fixtures/root-registry-v1/root_registry_v1.h) defines
four arm64 layouts: public capability 32 bytes, copied floor 40 bytes, immutable
factory 48 bytes, and owned snapshot 264 bytes.

The host pins `libbirnam_root_registry.dylib` and attaches
`birnam_root_registry_factory_v1` through Id's
`birnam_root_registry_dependency_v1` before `id_init`. Validation and creation
invoke no floor operations. Invalid descriptors reject; repeat attachment of
the same factory succeeds and retargeting rejects. Each Runtime owns an
independent service. Id publishes its capability through
`birnam_root_registry_provider_v1`. Ordinary module mounting creates no language
objects or methods. Production Id does not link the service implementation.

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
Id retains direct Runtime admission and calls snapshot at the existing tracing
and storage-observation points. Live GC invokes the copied providers in order;
callback-time appends wait for a future snapshot. Storage diagnostics publishes
the addresses without invoking them and keeps its existing coverage flags.

Destruction calls no providers. The registry owns neither roots nor callback
code; existing native code lifetime rules remain. Stack capture, collector
settings, mark adaptation, collection phases, weak identity and finalization
remain with their existing owners. There is no unregister or deduplication.

IdApi v15 has 25 fields and 200 bytes. Native initialization takes eight
arguments, adding the matching root registry after method observation.
[Native Loader v8](native-loader-v8.md) forwards the saved execution selection.
String preparation v3, Float/Word preparation v5 and Range/Vector/Map construction
v5 receive explicit incoming capabilities at their early registration points.
Image encoding and all provider/mark signatures remain unchanged.
