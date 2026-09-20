# Live Namespace presence v1

Design [81BB](../docs/design/81bb-production-namespace-presence.md) moves the
canonical presence operation out of Id. The [C mirror](fixtures/namespace-presence-v1/namespace_presence_v1.h)
defines a 32-byte arm64 descriptor: version 1 at 0, extent at 8, opaque context
at 16 and `contains(context, c_name) -> int` at 24.

Namespace factory v5 creates one owned service per Runtime. After id_init the
selected floor's `birnam_namespace_presence_provider_v1()` publishes that mutable
descriptor. The host validates its version, extent and nonnull operation without
calling it. Native initialization receives it as a third argument, after API and
prior state. The direct Native Loader forwards its original execution selection
at its existing module_init call without further symbol queries.

Callers may also supply independent descriptors with independent API selections.
A caller guarantees a valid readable descriptor, code, context and NUL-terminated
name for each call. Context may be null if its callback accepts it. A retaining
consumer requires these to remain pinned for its execution lifetime. Synchronous
borrowers, including Error Value, retain nothing. Access is owner-thread
serialized. Recipients copy function and context together at the actual call and
retain no Rust reference across it. Callbacks may reenter, collect and replace
the descriptor for subsequent calls. Zero means absent; all nonzero values,
including negative values, mean present.

The default operation preserves the former IdApi.isExported behavior: reject a
null name with `birnam-id invariant failed: IdApi.import name is null`, scan the
C string before floor owner/phase admission, then invoke the selected Namespace
lookup using Id's unchanged restricted Symbol interner. Missing names are still
interned. Empty and non-UTF8 names work. A raw null binding is absent; a nil object
binding is present. The default never invokes replaced public import/intern
slots. Neither restricted interning nor raw lookup sends, collects or reenters.

IdApi v9 has no presence slot and there is no callback-slot bridge or API-address
registry in production. The floor retains the original function/context pair
separately for pure numeric admission; live replacement withdraws admission
without invoking the replacement. Owned services are destroyed by descriptor
address even if the public operation/context was replaced.
