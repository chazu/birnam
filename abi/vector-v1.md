# Vector storage and construction v1

Historical 81AB boundary, superseded in production by [Vector v2](vector-v2.md).
The frozen C mirror remains available for independent baseline tests.

Design [81AB](../docs/design/81ab-production-vector-storage.md) moves Vector's
canonical objects, 32-way trie, five-word payload and native storage operations
into `birnam-vector`. Collection retains the language methods and sorting roots,
plus the String, Map and Range consumers of those operations.

The host loads Vector before Collection, obtains `birnam_vector_provider_v1`
and attaches it through `birnam_vector_dependency_v1`. Native embedders must do
likewise. The immutable descriptor and its code stay pinned for the process.
Module initialization enables the provider without constructing any objects.

The arm64 descriptor is 96 bytes: version 1 at offset 0, `size_t byte_size` at 8,
and ten function pointers from offset 16. The independent definition is
`fixtures/vector-v1/vector_v1.h` (`abi/fixtures/vector-v1/vector_v1.h` at `eb219997`). Attachment
rejects null, wrong version/size, any missing operation or another descriptor;
reattaching the same address succeeds. IdApi v7 does not change.

| Operation | Contract |
| --- | --- |
| `prepare(api, parent, adopting)` | Before Range creation: create VectorNode then VectorVt, or import VectorNode, VectorVt, Vector. Return the two behaviors for existing null validation. |
| `finish_prototype(api)` | Before Range's prototype: retain a nonnull adopted value, otherwise allocate two nodes and the empty Vector. |
| `set_layout(api)` | Before Range's layout: all-OOP nodes, then RAW/RAW/OOP/OOP/RAW Vector with RAW rest. |
| `set_owner(api)` | Inside Collection's Vector-absent branch, before its two exports. |
| `publish(api)` | After Collection's exports: VectorVt, Vector, VectorNode; no repeated predicate or owner operation. |
| `empty()` | Allocate two zero-filled 32-OOP nodes, then initialize the 40-byte canonical Vector payload. |
| `push(vector, value)` | Existing persistent append, including its allocation order and copied paths; null values remain allowed. |
| `count(vector)` | Pure native count read. |
| `at(vector, index)` | Pure native trie access; out-of-bounds/missing values return null. |
| `behavior()` | Return the retained canonical Vector behavior without a namespace lookup. |

All operations run synchronously on the runtime owner thread. Payload reads
require initialized Vector storage, including inherited receivers. Pure reads
perform no Id call, allocation, collection or reentry. Empty/push retain the
first selected Collection IdApi and reread its live allocator on every call.
Construction/layout/publication use the supplied table at the original positions.
Neither storage operations nor direct Collection consumers acquire overridable
Vector sends. Existing language-level sends keep their original dispatch.

Vector roots its node behavior, Vector behavior and prototype, including partial
construction. Collection's shared parent and method/sort roots stay with
Collection. Reload reapplies layouts and methods without replacing canonical
identities or the selected table; nonnull adopted prototypes are retained even
when rebound. There is no finalizer or new image format.

Run `mise run vector` for native debug checks and
`scripts/vector-image-smoke.sh PREFIX` for installed startup, a source-free
compiled module, a v8 native Code image and a baked executable.
