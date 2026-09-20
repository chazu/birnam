# Native Loader installation v6

[81BH](../docs/design/81bh-production-selector-enumeration.md) extends the execution
API's [vtable enumeration capability](vtable-enumeration-v2.md) with selector observation in
the [v5 installer](native-loader-v5.md). The immutable 24-byte descriptor has
version 6 at offset 0, extent at 8 and install at 16. Obtain
`birnam_native_loader_provider_v6` and attach through
`birnam_native_loader_dependency_v6`. The [independent C mirror](fixtures/native-loader-v6/native_loader_v6.h)
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
