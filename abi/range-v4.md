# Range construction v4

Design [81BF](../docs/design/81bf-production-namespace-export.md) adds explicit
exports to [v3 construction](range-v3.md). Obtain `birnam_range_provider_v4`
and attach through `birnam_range_dependency_v4`. The immutable descriptor
remains 56 bytes with 5 operations, version 4 at offset 0 and extent at 8.
Admission rejects missing fields, wrong version/extent and retargeting;
repeat attachment of the same pointer succeeds. The
[independent C mirror](fixtures/range-v4/range_v4.h) fixes its layout.

Only publication changes signature: `publish(api, exports)` receives the
supplied API's `const NamespaceExportV1 *`. It reads each export function/context
at the original publication point, following any owner callback and preceding
the next publication. Results and publication order retain the v3 contract.
Collection passes incoming exports even when method execution retains an older
API. No execution selection is inferred from an API address.

Preparation remains `prepare(api, imports, parent, adopting)`. All other stage
signatures, allocation, partial roots, identities, layouts, callbacks, dispatch,
adoption and reload behavior retain v3 contracts. Native initialization uses
IdApi v12's six arguments. Object representation and image encoding are unchanged.
See the [native export inventory](rust-range-dylib-v1.json).
