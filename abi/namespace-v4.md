# Namespace storage v4

Design [81AZ](../docs/design/81az-production-namespace-lookup.md) adds interned
lookup to the [v3 factory](namespace-v3.md). The host prepares
`birnam_namespace_provider_v4` through `birnam_namespace_dependency_v4` before
id_init. The immutable descriptor is 120 bytes on arm64; version is 4. Existing
operations retain their offsets and contracts. All thirteen operations are
required, including `lookup` at offset 112. Null, wrong version/extent, missing
operations and retargeting reject; repeated attachment succeeds.

The [C mirror](fixtures/namespace-v4/namespace_v4.h) specifies exact signatures.
`lookup(storage, selected_find, interner_context, intern, bytes, length, output)`
borrows every argument synchronously. Required callbacks and output must be
valid; bytes are readable for length bytes (null is allowed only for zero length).
Contexts are opaque and may differ. Neither callback may send, collect, reenter
or retain any argument. The caller supplies owner/phase admission before entry.

`intern` is called exactly once with the unchanged byte pointer and length. It
returns the canonical Symbol, or null for embedded-NUL rejection. Rejection
returns zero without invoking find or touching output. Success calls the selected
find once with storage and Symbol, writes its result and returns one. A null
binding is a successful lookup. Empty/non-UTF-8 names and absent-name interning
are controlled by the unchanged Id primitive; Namespace does not scan, allocate
a copy, cache or retain the name. Id retains the interner's compatibility limits
and weak dictionary mirror. The selected raw find remains callback-free.

Runtime copies operation/context selection before the call and lends its state
only to the interner callback. No Environment or RuntimeState reference crosses
that mutable borrow. Namespace adds no service state, destructor or global
registry. Names and enumeration services retain their existing ownership.

Factory v3 is historical; current hosts and fixtures select v4 without fallback.
IdApi v8, numeric declarations, module_init, language protocols and image encoding
are unchanged. Migrating the live public import/presence slots remains separate.
