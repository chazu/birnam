# Native Loader installation v5

[81BG](../docs/design/81bg-production-vtable-enumeration.md) adds the execution
API's [vtable enumeration capability](vtable-enumeration-v1.md) after exports in
the [v4 installer](native-loader-v4.md). The immutable 24-byte descriptor has
version 5 at offset 0, extent at 8 and install at 16. Obtain
`birnam_native_loader_provider_v5` and attach through
`birnam_native_loader_dependency_v5`. The [independent C mirror](fixtures/native-loader-v5/native_loader_v5.h)
fixes the eight-argument installation signature.

Prelude passes registration API, saved execution API, saved presence, imports,
exports and vtable enumeration, followed by the selected System behavior and
loadModule selector. Native Loader retains its first execution selection before
registration callbacks. Reentry or later registration with other arguments does
not replace that selection. Same-pointer dependency attachment is repeatable;
invalid descriptors and retargeting reject as before.

Direct loading forwards `(api, null, presence, imports, exports, vtables)` to
the selected native initializer. No dependency query, attachment, root, handle
release or new callback is added. All retained descriptors, context and native
code remain live alongside their matching execution API. Path ownership,
errors, return values and image encoding are unchanged.
