# Map storage and construction v1

Design [81AF](../docs/design/81af-production-map-storage.md) moves canonical Map
objects, the two-word payload, variable-sized HAMT nodes, native lookup and
persistent update into `birnam-map`. Collection retains the Map methods and
traversal/printing helpers and uses the native storage operations.

Load Map after collection-value and before Collection. Attach
`birnam_collection_value_dependency_v1` before initialization, obtain
`birnam_map_provider_v1`, then attach it to Collection through
`birnam_map_dependency_v1`. Module initialization only enables the provider;
construction stays at the original Collection points. Descriptor/code remain
immutable and process-pinned; all calls run on the runtime owner thread.

The arm64 descriptor is 112 bytes: version 1 at offset 0, size at 8 and twelve
function pointers starting at 16. See the independent
[map_v1.h](fixtures/map-v1/map_v1.h). Admission rejects null, wrong version/size,
any missing operation and retargeting; the same pointer is idempotent.

| Operation | Contract |
| --- | --- |
| `prepare(api, parent, adopting)` | After Range preparation: create MapVt then MapNode, or import MapVt, Map, MapNode. Return the two behaviors for existing null validation. |
| `finish_prototype(api)` | After Range prototype creation: retain a nonnull adopted prototype, otherwise allocate and initialize the empty Map. |
| `set_layout(api)` | Node RAW/RAW header with OOP rest; Map OOP/RAW with RAW rest. |
| `publish(api)` | After Range publication inside the existing Vector-absent branch: set owner, export MapVt, Map, MapNode; no repeated predicate. |
| `empty()` | Allocate and initialize the 16-byte Map with null root and zero count using the first selected Collection table's live allocator. |
| `put(api, map, key, value)` | Nonnull key/value and initialized Map storage. Original persistent association and identity return, including shared flag timing and final count initialization. |
| `find(api, map, key)` | Snapshot root before hash/equality callbacks; return the stored Oop or null for missing. |
| `count(map)` | Pure count read from initialized Map storage, including inherited receivers. |
| `root(map)` | Pure root read at the caller's existing snapshot point. |
| `slot_count(node)` | Snapshot an initialized nonnull node header and compute its existing slot count. |
| `slot(node, index)` | Pure unchecked read of an in-range slot in an initialized nonnull node. |
| `behavior()` | Retained canonical Map behavior without namespace lookup. |

Node readers let Collection's three existing traversal helpers keep their
header snapshot and slot/callback order. No layout definition is duplicated in
Collection. Map's header and payload stay 16 bytes each; nodes retain their
bitmap/collision tags, child sentinels and persistent sharing. Pure reads make
no Id call, allocation, collection or reentry.

Lookup/update use each caller's selected live IdApi and shared value policy.
The update still resets the shared entry-added flag before hashing, reads the
old count after association, then allocates and reads that flag to initialize
the final payload. Nested updates can change the flag; its existing behavior is
preserved. Methods retain their argument validation and nil/boolean handling.

Map roots its three identities throughout partial construction and reload.
Collection keeps the parent and selector roots. Adopted nonnull prototypes stay
unchanged, and reinstallation does not replace canonical identities or the
first empty-allocation table. IdApi v7 and image encoding remain unchanged.

Run `mise run map` for native debug checks and
`scripts/map-image-smoke.sh PREFIX` for dynamic startup, a source-free compiled
module, a v8 native Code image and a baked executable.
