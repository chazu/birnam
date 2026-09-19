# Namespace storage v3

Design [81AY](../docs/design/81ay-production-namespace-enumeration.md) adds
[enumeration-service ownership](namespace-enumeration-v1.md) to the
[previous v2 factory](namespace-v2.md). The host prepares
`birnam_namespace_provider_v3` through `birnam_namespace_dependency_v3` before
id_init. Each Runtime owns independent state and services; normal module_init
never resets them. Id has no normal dependency on the Namespace implementation.

The immutable arm64 descriptor is 112 bytes. Version is 3; the previous ten
operations retain their offsets and semantics. `create_enumeration` is at 96,
`destroy_enumeration` at 104. All twelve operations are required. Null, wrong
version/extent, missing operations and retargeting reject; repeat attachment of
the same descriptor succeeds. The [C mirror](fixtures/namespace-v3/namespace_v3.h)
specifies the exact signatures.

`create_enumeration(storage, selected_count, selected_name_at, floor_access)`
returns the Namespace-owned descriptor pair. It copies only opaque state and
selected callbacks; it does not assume the storage representation or invoke a
callback during construction. `destroy_enumeration(pair)` releases it in its
owning module. Enumeration and names services are destroyed before their state.
All raw-storage and [reverse-name](namespace-names-v1.md) semantics remain intact.

Factory v2 is historical; native hosts and current test helpers must select v3.
No IdApi, Oop, language protocol or image encoding changes are made here.
