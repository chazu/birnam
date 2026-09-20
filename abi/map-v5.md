# Map construction v5

Design [81BJ](../docs/design/81bj-production-root-provider-registry.md) adds an
explicit incoming [Root Registry capability](root-registry-v1.md) to preparation:
`prepare(api, imports, roots, parent, adopting)`. Collection supplies the same
selection as its incoming API, even when execution retains an older API.
Registration preserves its existing Once guard and position before construction
or adoption. No capability is inferred from an API address.

Obtain `birnam_map_provider_v5` and attach through
`birnam_map_dependency_v5`. The [independent C mirror](fixtures/map-v5/map_v5.h)
keeps the v4 descriptor size and operation offsets, with version 5. Invalid
versions/extents or missing operations reject, repeat attachment succeeds, and
retargeting rejects. All other stages, callbacks, allocation, identities,
publication, reload behavior and image encoding retain the [v4 contract](map-v4.md).
Native initialization uses IdApi v15's eight arguments.
