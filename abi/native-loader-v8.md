# Native Loader installation v8

Design [81BJ](../docs/design/81bj-production-root-provider-registry.md) adds the
matching [Root Registry capability](root-registry-v1.md) to the saved execution
selection. The immutable descriptor remains 24 bytes: version 8, extent at
offset 8 and install at offset 16. Obtain `birnam_native_loader_provider_v8`
and attach through `birnam_native_loader_dependency_v8`. The
[independent C mirror](fixtures/native-loader-v8/native_loader_v8.h) fixes the
ten-argument installation signature.

Prelude supplies registration API, saved execution API, saved presence, imports,
exports, vtable enumeration, method observation and root registry, followed by
the selected System behavior and selector. Native Loader retains its first
execution selection before registration callbacks; reentry and subsequent
installation do not replace it. Admission retains the v7 rules.

Direct loading forwards `(api, null, presence, imports, exports, vtables,
observation, roots)` to the selected initializer. Path ownership, dependency
handling, handle lifetime, return values, errors and image encoding retain the
[v7 contract](native-loader-v7.md).
