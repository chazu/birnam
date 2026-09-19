# Namespace enumeration v1

Design [81AY](../docs/design/81ay-production-namespace-enumeration.md) moves the
private count/name-at operations into Namespace. IdApi v8 stays 33 fields / 264
bytes. The old `birnam_id_export_count_v1` and `birnam_id_export_name_at_v1`
function exports are removed; no compatibility wrappers remain.

The floor instead exports two data publications, `birnam_namespace_count_v1`
and `birnam_namespace_name_at_v1`. Each is 24 bytes on arm64: version 1 at 0,
extent at 8 and descriptor pointer at 16. The pointer is initially null and
published once, on the owner thread before id_init returns. No concurrent native
access is allowed during publication. Each selected publication and descriptor
must be readable, aligned and pinned throughout its enumeration.

Each immutable descriptor is 32 bytes: version 1, extent, nonnull opaque context
and required operation at offset 24. `count(context)` returns the number of raw
name slots, including null-valued bindings; `name_at(context, index)` returns
the original name Symbol, or raw null past the end. Count and name-at descriptors
are independent selections. The [C mirror](fixtures/namespace-enumeration-v1/namespace_enumeration_v1.h)
checks all layouts. Malformed publications or descriptors are unavailable, with
no invocation of their operations.

[Namespace storage v4](namespace-v4.md) creates a 64-byte pair of descriptors in
one owned service. Creation copies the opaque storage context, selected storage
count/name-at operations and a 32-byte floor access descriptor (version, extent,
context and required preflight). It calls none of them. Null context/operations,
invalid floor version/extent and missing preflight reject creation. A default
operation preflights owner/storage-access checks and then invokes its selected
storage operation, without allocating, collecting, mutating or sending. The
state roots its names and values as before. Destroy the service before state;
code, contexts and publications remain pinned until all callers finish.

Reflect retains its existing selected-floor discovery, including the shared
cached handle, failed initial acquisition and live dlsym callback on every use.
It resolves and snapshots count, then name-at; missing count skips the second
lookup. Missing either skips Vector import and GC changes. After both snapshots,
Reflect imports Vector, rejects raw null/nil, disables auto-GC, calls count once,
calls name-at for each original index and filters raw-null names. It builds the
Vector using the existing new/add sends, then enables auto-GC. Replacements in
later lookup/import/GC callbacks affect subsequent selections while the current
one retains every operation/context it has already captured. Native replacement may return
other valid publications through the live dlsym callback. No aggregate API or
alternate floor-discovery path is added.

Run `mise run namespace` and
`scripts/namespace-enumeration-image-smoke.sh PREFIX [81AX_PREFIX]` for focused
state/selected-storage and native Reflect parity/startup/compiled/image/baked
checks. The optional baseline uses a separately compiled old discovery adapter;
all behavioral assertions and trace generation are shared.
