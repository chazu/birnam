# Native Loader installation v4

Design [81BF](../docs/design/81bf-production-namespace-export.md) adds the
execution API's [export capability](namespace-export-v1.md) to the
[v3 installer](native-loader-v3.md). Obtain `birnam_native_loader_provider_v4`
and attach through `birnam_native_loader_dependency_v4`. The immutable
24-byte descriptor has version 4 at offset 0, extent at 8 and install at 16.
Admission and same-pointer repeat attachment retain the v3 contract. The
[independent C mirror](fixtures/native-loader-v4/native_loader_v4.h) fixes its signature.

Install receives registration API, execution API, execution presence, imports,
exports, then System behavior and loadModule selector. Capture all execution
selections at the original first-install point before registration callbacks.
Prelude supplies its original execution selections even if installation reenters
with a different registration API. Subsequent installation retains that selection.

Direct System loading forwards saved API, null prior state, and the saved
presence/import/export descriptors to five-argument `module_init`. It adds no
dependency query, attachment or handle release. Callback order, path ownership,
errors, roots and image encoding are unchanged. Descriptors, context and code
remain pinned alongside their matching execution API; no API-address registry
or callback-slot loan is used.

See the [native export inventory](rust-native-loader-dylib-v1.json),
`mise run native-loader`, and `scripts/native-loader-image-smoke.sh PREFIX`.
