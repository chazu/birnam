# Vector construction v5

Design [81BJ](../docs/design/81bj-production-root-provider-registry.md) adds an
explicit incoming [Root Registry capability](root-registry-v1.md) to preparation:
`prepare(api, imports, roots, parent, adopting)`. Collection supplies the same
selection as its incoming API, even when execution retains an older API.
Registration preserves its existing Once guard and position before construction
or adoption. No capability is inferred from an API address.

Obtain `birnam_vector_provider_v5` and attach through
`birnam_vector_dependency_v5`. The independent C mirror (`abi/fixtures/vector-v5/vector_v5.h` at `eb219997`)
keeps the v4 descriptor size and operation offsets, with version 5. Invalid
versions/extents or missing operations reject, repeat attachment succeeds, and
retargeting rejects. All other stages, callbacks, allocation, identities,
publication, reload behavior and image encoding retain the [v4 contract](vector-v4.md).
Native initialization uses IdApi v15's eight arguments.
