# Vector construction v3

Design [81BE](../docs/design/81be-production-namespace-import.md) adds explicit
imports to [v2 construction](vector-v2.md). Obtain
`birnam_vector_provider_v3` and attach through `birnam_vector_dependency_v3`.
The immutable descriptor remains 104 bytes with 11 operations, version 3
at offset 0 and byte size at 8. Admission still rejects missing fields, wrong
version/extent and retargeting; repeat attachment of the same pointer succeeds.
The [independent C mirror](fixtures/vector-v3/vector_v3.h) fixes the layout.

`prepare(api, imports, parent, adopting)` receives the incoming API's explicit
`const NamespaceImportV1 *` immediately after the API. The adoption branch
imports VectorNode, VectorVt, Vector in that order through this incoming live descriptor.
Each call snapshots its current function/context and retains its result through
later callbacks. Preparation uses the supplied API at the same original points;
method/storage execution still retains the original selected API.

All other operation signatures and construction positions retain their v2
contracts, including partial roots, null/adopted prototypes, layouts, selector
and callback retention, registration, publication, and reload behavior. Import
is not added to ordinary storage operations, and no new lookup, allocation or
collection point is introduced. V2 remains historical without a production
adapter. Native modules use IdApi v10 and four-argument initialization; object
representations and image encoding are unchanged.

See the [native export inventory](rust-vector-dylib-v1.json). Run
`mise run vector` and `scripts/vector-image-smoke.sh PREFIX` for focused debug
and dynamic startup/source-free/saved-image/baked execution checks.
