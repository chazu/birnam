# Map construction and methods v2

Design [81AG](../docs/design/81ag-production-map-methods.md) completes native Map
ownership: canonical objects, HAMT storage, thirteen method bindings, traversal
and printing. Collection retains the shared parent and ordered installation.
All native storage consumers now live in Map, so the v1 storage operations are
private; there is no production v1 adapter.

Load Map after collection-value, Integer, String and Vector and attach their
native dependencies before initialization. Obtain `birnam_map_provider_v2` and
mount it through `birnam_map_dependency_v2`. Initialization only enables the
provider; construction remains at the existing Collection points. Descriptor
and code remain immutable and process-pinned; calls run on the runtime owner
thread. Collection no longer needs an Integer dependency.

The immutable arm64 descriptor is 56 bytes: version 2 at offset 0, size at 8,
and five function pointers from offset 16. The independent C mirror is
[map_v2.h](fixtures/map-v2/map_v2.h). Admission rejects null, wrong version/size,
missing operations and retargeting; the same descriptor address is idempotent.

| Operation | Contract |
| --- | --- |
| `prepare(api, parent, adopting)` | After Range preparation: create MapVt then MapNode, or import MapVt, Map, MapNode; return the two behaviors for existing null validation. |
| `finish_prototype(api)` | After Range prototype creation: retain a nonnull adopted prototype or allocate the empty Map. |
| `set_layout(api)` | Original node RAW/RAW header with OOP rest, then Map OOP/RAW with RAW rest. |
| `publish(api)` | After Range publication in the existing Vector-absent branch: set owner, export MapVt, Map, MapNode without another predicate. |
| `install(api, value, value2, print_string)` | After Range methods: register all thirteen Map bindings in the original order using the supplied table. |

The first Collection-selected table in Family remains the method/storage table.
Installation retains the first three selector identities. Reinstallation uses
the supplied table to register methods while retaining the original table,
selectors and canonical objects. Every operation reads the live callbacks at
its original points. Map's root provider marks its three identities during
partial construction and then its three cached selectors.

Projection still allocates the empty Vector before reading the Map root. Each
traversal snapshots its node header and rereads slots at the original points.
Map do:/inject:into: call Vector's native bodies after projecting all values;
public Vector replacements do not intercept those calls. Fallback, block and
printString sends retain fresh lookup and existing nil/null result handling.
Printing keeps each pair's snapshot, borrowed String-byte copy and early returns.
No temporary argument or String-byte borrow is retained past its existing scope.

Payloads, layouts, collision handling, shared added-flag timing, persistent
sharing, IdApi v7 and image encoding remain unchanged. Run `mise run map` for
native debug checks and `scripts/map-image-smoke.sh PREFIX` for installed
startup, a source-free compiled module, a v8 native Code image and a baked app.
