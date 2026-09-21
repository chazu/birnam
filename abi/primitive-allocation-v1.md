# Retained Id allocation primitives v1

This temporary Bootstrap-to-Runtime descriptor carries two completed native Id
entries selected through `lookup` before collecting construction methods mount.
It is the final argument of `birnam_runtime_init_v12`. It has no installer or
future callback slots and does not change the Id protocol carrier.

On the supported 64-bit target its C layout is:

```c
struct PrimitiveAllocationV1 {
    uint32_t version;              /* 1, offset 0 */
    size_t byte_size;              /* 48, offset 8 */
    BirnamNativeEntry allocate;    /* offset 16 */
    BirnamNativeEntry intern;      /* offset 32 */
};
```

Both methods must be present. Admission checks the aligned readable header and
exact version/size before reading the entries. Bootstrap copies the descriptor;
Id and the code/contexts of both entries remain alive throughout use.

- `allocate`: registered behavior receiver, one Word byte count; returns a
  zero-initialized Object with that behavior, using Id's ordinary allocation
  extent, alignment and identity rules.
- `intern`: Id receiver, one Bytes spelling; returns the canonical Symbol.

These selected entries never collect, update an upper allocation budget or call
Runtime. Later replacement of their method bindings does not change the selected
callables. Ordinary Runtime constructors surround their calls with existing
admission, rooting, collection and accounting. Deferred identity materialization
can use them during collection because its journal already charged the logical
allocation; it must not recursively collect or charge it again.

Id owns every production allocation from initial construction until detachment
or teardown. Runtime's Heap uses native Id facts and copied coordinates, never an
Id allocation record or a borrow into that registry. `allocationAt:` resolves an
exact Word address to the original Object pointer, or Unit; interior/header,
unknown and detached addresses return Unit without dereferencing the candidate.
`allocationCount` returns the current registry length in constant time.
`detachAllocationAt:` removes membership immediately and returns the existing
one-use release entry, so finalization can read detached bytes before reclamation.

The independent installed native-loading fixture checks exact membership,
counts, snapshot growth and detachment. Its activation fixture saves the primitive
entries before activation and checks they remain non-collecting after activation
and Runtime adoption while saved upper construction entries still collect.
The deferred-identity image workload checks ordinary and forced materialization
through source, compiled and image execution.

This descriptor exists for the temporary aggregate Runtime constructor. It is
not a new language service. Symbol/dictionary ownership and their upper private
interfaces still require migration before Id can be packaged independently.
