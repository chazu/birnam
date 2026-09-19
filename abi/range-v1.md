# Range representation and construction v1

> Historical 81AA/81AB checkpoint. Production now uses [Range v2](range-v2.md).
> This definition remains available only for independent baseline comparison.

Design [81AA](../docs/design/81aa-production-range-storage.md) moves canonical
Range identities, the two-i64 RAW payload, construction/layout and Integer `to:`
into the existing Range module. Collection keeps its two Vector-backed method
bodies and uses Range's pure bounds reader instead of knowing its payload.

The host loads Range before Collection, discovers `birnam_range_provider_v1`
and mounts its immutable descriptor through `birnam_range_dependency_v1`.
The descriptor, selected Id table, identities and both modules remain live for
the process. Native embedders must perform the same attachment. There is no
production fallback to the earlier method-only construction dependency.

The arm64 descriptor is 64 bytes: version 1 at offset 0, `size_t byte_size` at 8,
and six function pointers beginning at 16. The independent C definition is
[`fixtures/range-v1/range_v1.h`](fixtures/range-v1/range_v1.h). Null, wrong-version,
wrong-size, missing-operation and different-descriptor attachments are rejected;
reattaching the same descriptor succeeds.

| Operation | Existing construction point / contract |
| --- | --- |
| `prepare(api, parent, adopting)` | Creates behavior after Vector behavior, or imports RangeVt then Range after Vector imports; returns behavior only for the existing null-export validation. First selection is retained. |
| `finish_prototype(api)` | Between Vector and Map prototypes; retains a nonnull adopted value or allocates the default bounds 0,0 with the ordinary live alloc callback. |
| `set_layout(api)` | Between Vector and Map-node layouts; reapplies two RAW words and RAW rest. |
| `install(api, value1, value2, printString, reversed, comma)` | Installs the eight Range selectors in order, followed by Integer `to:`; supplied Vector implementations bind directly. |
| `publish(api)` | Inside the already-selected Vector-absent publication branch, after Vector and before Map; owner association, RangeVt, Range in order, with no new export predicate. |
| `bounds(object)` | Pure 16-byte C result containing start/end; no Id call, allocation, GC, reentry or mutable family-state access. Valid for initialized Range payloads, including inherited receivers. |

All stages run synchronously on Id's owner thread. Range owns the partial and
completed identity roots and cached selectors. Ordinary calls retain the table
selected at initial construction; layout, installation and publication use the
current table supplied by Collection. Reinitializing Collection still reinstalls
methods and layout while preserving cached identities and namespace aliases.
Preparing the library alone does not create or republish the family.

IdApi v7 and Image's two-RAW-word encoding are unchanged. The earlier independent
[method-only descriptor](range-methods-v1.md) remains historical oracle evidence.
Run `mise run range` for native checks and `scripts/range-image-smoke.sh PREFIX`
for installed debug startup, source-free compiled module, saved image and bake.
