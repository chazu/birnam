# Native Loader installation v9

Design [81BK](../docs/design/81bk-production-gc-settings.md) adds the matching
[GC Settings capability](gc-settings-v1.md) to the saved execution selection.
The immutable descriptor remains 24 bytes: version 9, extent at offset 8 and
install at offset 16. Obtain `birnam_native_loader_provider_v9` and attach
through `birnam_native_loader_dependency_v9`. The
[independent C mirror](fixtures/native-loader-v9/native_loader_v9.h) fixes the
eleven-argument installation signature.

Prelude supplies registration API, saved execution API, saved presence, imports,
exports, vtable enumeration, method observation, root registry and GC settings,
followed by the selected System behavior and selector. Native Loader retains
its first execution selection before registration callbacks; reentry and later
installation do not replace it. Admission retains the v8 rules.

Direct loading forwards `(api, null, presence, imports, exports, vtables,
observation, roots, settings)` to the selected initializer. Path ownership,
dependency handling, handle lifetime, return values, errors and image encoding
retain the [v8 contract](native-loader-v8.md).
