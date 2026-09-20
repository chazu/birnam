# Namespace factory v6

The selected floor requires `birnam_namespace_dependency_v6` before id_init.
The host pins `libbirnam_namespace.dylib`, obtains
`birnam_namespace_provider_v6` and attaches its immutable 144-byte descriptor.
All sixteen operations are required. Invalid descriptors and retargeting reject;
repeating the same attachment succeeds. Historical factories are not fallbacks.
The [independent C mirror](fixtures/namespace-v6/namespace_v6.h) defines the layout.

The first fifteen operations retain [v5](namespace-v5.md)'s offsets and contracts.
`publish` is added at offset 136:

```c
int publish(void *storage, BindV1 bind, void *floor, InternV1 intern,
            MirrorV1 mirror, const uint8_t *bytes, size_t length,
            Oop value, Oop *result);
```

The caller supplies its selected raw bind, restricted Symbol interner and weak
dictionary mirror. Intern receives the borrowed bytes/extent and floor context.
If it returns null, publish returns zero without invoking bind/mirror or writing
result. Otherwise bind receives storage, Symbol and value; mirror then receives
floor, the same Symbol and the original value. Publish writes bind's result and
returns one. Both a null input value and a null successful bind result are valid.
The mirror's input is independent of bind's return value.

Id's mirror primitive updates its existing weak environment dictionary when that
dictionary exists, retaining its mutation accounting. It is a no-op before the
internal dictionaries are installed. The interner retains its existing Symbol
allocation, compatibility limits and weak Symbol-dictionary mirror.

The caller performs owner/observation admission before publish. All callbacks
are restricted primitives: no language sends, collection, reentry or retained
borrows. No RuntimeState/Environment reference may span the interner or mirror.
Storage and floor contexts may be distinct; no context, byte pointer, Oop or output
address is retained. The operation adds no service state or collection safepoint.

This moves the canonical publication sequence and its actual Runtime consumer.
IdApi v9, live public export/import slots, namespace capabilities, native
initializer transport and image encoding retain their existing contracts.
