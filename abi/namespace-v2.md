# Namespace storage v2

Design [81AX](../docs/design/81ax-production-namespace-names.md) extends the
[historical v1 storage](namespace-v1.md) with names-service ownership. The host
prepares `birnam_namespace_provider_v2` through Id's
`birnam_namespace_dependency_v2` before id_init. There is no v1 fallback.

The immutable descriptor is 96 bytes on arm64: version 2, byte size, the original
eight storage operations with unchanged offsets and semantics, then
`create_names` at 80 and `destroy_names` at 88. Every operation is required;
malformed descriptors and retargeting are rejected. The
[independent C header](fixtures/namespace-v2/namespace_v2.h) specifies the layout.

`create_names(namespace_context, selected_reverse, floor_access)` allocates an
independent [names service](namespace-names-v1.md) and returns its stable mutable
API pointer. It copies the selected reverse operation and immutable floor access
without invoking either. Null namespace, reverse or access, incorrect access
version/extent and missing access operations return null without effects. The
namespace context is opaque even to the service: it invokes the selected reverse
operation instead of assuming its representation. Thus independently selected
or instrumented storage preserves its behavior.

Id owns one state and names service per Runtime, created at the original
bootstrap point. `destroy_names(api_pointer)` releases the service using the
stable API pointer, independently of its replaceable live context. It must run
before the namespace state is destroyed. Descriptor, code, access context and
state remain pinned for all calls. Normal module_init does not reset state.
All original raw-storage contracts remain: owner serialization, opaque Oops,
ordered rebinding, first current alias, null handling and synchronous borrowed
views. Service callbacks obey the restricted primitive contract below; raw
storage operations still make no floor calls.

Import, export, presence, enumeration, Symbol storage and namespace roots remain
with their existing floor adapters. No language protocol or image encoding changes.
