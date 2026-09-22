# Vector storage, construction and methods v2

Design [81AE](../docs/design/81ae-production-vector-methods.md) completes native
Vector ownership: canonical identities, persistent trie, thirteen method
bindings and temporary sorting roots. Collection coordinates construction and
registration; Range and Map keep their direct native Vector operations.

Load collection-value before Vector, then Range and Collection. Attach Integer,
String and collection-value to Vector before initialization. Obtain
`birnam_vector_provider_v2` after initialization and mount it through
`birnam_vector_dependency_v2` on each consumer. Initialization enables the
provider without constructing objects. Descriptor and code stay process-pinned;
all calls run synchronously on the runtime owner thread.

The immutable arm64 descriptor is 104 bytes: version 2 at offset 0, size at 8,
and eleven function pointers from offset 16. The independent C mirror is
vector_v2.h (`abi/fixtures/vector-v2/vector_v2.h` at `eb219997`). Admission rejects null, wrong
version/size, missing operations and retargeting; the same pointer is idempotent.
There is no production v1 adapter or change to IdApi v7.

| Operation | Contract |
| --- | --- |
| `prepare(api, parent, adopting)` | Create VectorNode then VectorVt, or import VectorNode, VectorVt, Vector; return the two behaviors for existing null checks. |
| `finish_prototype(api)` | Before Range prototype creation: retain a nonnull adopted prototype, otherwise allocate the two nodes and empty Vector. |
| `set_layout(api)` | Before Range layout: all-OOP nodes, then RAW/RAW/OOP/OOP/RAW Vector with RAW rest. |
| `set_owner(api)` | Inside Collection's Vector-absent branch, before Collection exports. |
| `publish(api)` | After Collection exports: VectorVt, Vector, VectorNode, without another predicate or owner call. |
| `empty()` | Existing two-node and canonical 40-byte Vector allocation. |
| `push(vector, value)` | Existing persistent append and allocation order; null values remain allowed. |
| `behavior()` | Retained canonical Vector behavior without namespace lookup. |
| `install(api, value1, value2, print_string)` | At the original registration point, install all thirteen bindings in order; return the native comma Method passed directly to Range. |
| `for_each(vector, args, count)` | Original Vector do: body, used directly by Map do:. |
| `inject(vector, args, count)` | Original Vector inject:into: body, used directly by Map inject:into:. |

Count and trie access are now private because their production callers moved
into Vector. Construction stages and registration use the supplied IdApi.
Storage and method execution retain the first Collection-selected table and
read its live callbacks. Installation retains the first selector identities.
Reinstallation reapplies bindings without replacing the selected table, cached
selectors, canonical objects or Range's retained native comma callback.

Vector roots its three identities during partial construction, then the three
method selectors and a scoped thread-local sorting stack. Sort roots preserve
receiver, block, copied elements and keys through reentrant sends and nested
sorts; normal and early returns truncate only their own scope. Stable eager-key
sorting, fresh comparator lookup and exact-true comparison remain unchanged.
The payload, layout, image encoding and public method behavior do not change.

Run `mise run vector` for native debug checks and
`scripts/vector-image-smoke.sh PREFIX` for dynamic startup, a source-free
compiled module, a v8 native Code image and a baked executable.
