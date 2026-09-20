# Namespace factory v5

The selected floor requires `birnam_namespace_dependency_v5` before id_init.
The host pins `libbirnam_namespace.dylib`, obtains
`birnam_namespace_provider_v5` and attaches its immutable 136-byte descriptor.
All fifteen operations are required. Null, wrong version/extent, missing
operations and retargeting reject; repeating the same attachment succeeds.
The [independent C mirror](fixtures/namespace-v5/namespace_v5.h) defines the layout.

The first thirteen operations retain the offsets and contracts of
[factory v4](namespace-v4.md), including lookup at 112. New operations are
create_presence at 120 and destroy_presence at 128.

`create_presence(storage, selected_find, selected_lookup, floor)` creates a
[Presence v1](namespace-presence-v1.md) service. It rejects null storage, missing
callbacks and a null/invalid FloorV1 without invoking callbacks. FloorV1 is a
40-byte immutable descriptor with version 1, byte size, opaque interner context
`access(context) -> interner_context`, and
`intern(interner_context, bytes, length) -> Oop`. Access performs owner/phase
admission after C-string scanning and before the selected lookup. The returned
interner context is borrowed only for that synchronous lookup, never retained. The service copies the floor and
selected callbacks. Storage and restricted interner code/context must outlive it.
The caller retains owner-thread responsibility. Construction has no Id callbacks,
object roots, language sends or Symbol interning.

Each Runtime owns its raw namespace, names, enumeration and presence services.
Presence uses the selected lookup/find with that same raw storage. Destruction
uses the service descriptor address rather than its mutable public context,
then the owner destroys the other services and storage. Native module_init
never recreates existing Runtime state.

IdApi v9 removes isExported and native module_init receives explicit presence
as its third argument. Names/enumeration contracts, binding order, aliases,
null semantics, object identity, roots and image encoding retain their behavior.
Historical factory versions are not accepted as fallback.
