# Range construction v2

Design [81AC](../docs/design/81ac-production-range-vector-methods.md) completes
Range method ownership: `reversed` and `,` join its other six methods and Integer
`to:`. The host loads Vector before Range, attaches Vector's immutable storage
operations to Range, then mounts `birnam_range_provider_v2` through Collection's
`birnam_range_dependency_v2`. Loading either service constructs no family objects.
Collection invokes Range's construction stages at their original positions.

The descriptor is 56 bytes on arm64: version 2 at offset 0, `size_t byte_size` at
8, then five function pointers. See the independent
[C definition](fixtures/range-v2/range_v2.h). Null, wrong-version, wrong-size,
missing-operation and different-descriptor attachments are rejected. Attaching
the same descriptor again succeeds. The production v1 entry points and external
bounds operation are removed; historical definitions are test oracles only.

| Operation | Construction point / contract |
| --- | --- |
| `prepare(api, parent, adopting)` | Create Range behavior after Vector, or adopt RangeVt then Range in the original import order. Return behavior for existing incomplete-export validation. |
| `finish_prototype(api)` | Between Vector and Map prototypes; retain nonnull adopted values or allocate the default 16-byte bounds 0,0. |
| `set_layout(api)` | After Vector and before Map-node layout; two RAW words and RAW rest. |
| `install(api, value1, value2, printString, vector_comma)` | Install all eight Range selectors in order, then Integer `to:`. Retain the supplied native Vector comma Method from the first installation. |
| `publish(api)` | In Collection's Vector-absent branch after Vector and before Map: owner, RangeVt, Range, without repeating the predicate. |

Stages run synchronously on the owner thread. Descriptor, code, selected table
and the supplied nonnull native comma Method remain pinned for the process.
The supplied Method is the existing Vector implementation: no lookup is added,
and changing Vector's public comma binding does not change this internal call.
Reinstallation keeps the first callback, canonical identities, selectors and
selected IdApi. Layout, installation and publication use the current supplied
table, while ordinary operations continue to observe the selected table's live
callbacks. Range's five Oop roots remain unchanged; the native code pointer is
not an Oop or image payload.

Reversal snapshots both bounds before allocating its Vector. Concatenation first
checks the argument's exact Vector behavior, snapshots Range bounds and builds
its prefix, then invokes the native Vector comma implementation using the
original argument carrier. That implementation performs the existing second
argument/type check and reads the current argument Vector after prefix creation.
The native storage operations remain direct calls throughout.

IdApi v7, Range payload layout and image encoding stay unchanged. Run
`mise run range` for native debug checks and `scripts/range-image-smoke.sh PREFIX`
for installed startup, a source-free compiled module, native Code image and bake.
