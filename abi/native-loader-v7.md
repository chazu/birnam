# Native Loader installation v7

[81BI](../docs/design/81bi-production-method-observation.md) adds the matching
[Method Observation capability](method-observation-v1.md) to the saved execution
selection. The immutable descriptor remains 24 bytes: version 7 at offset 0,
extent at 8 and install at 16. Obtain `birnam_native_loader_provider_v7` and
attach through `birnam_native_loader_dependency_v7`. The [independent C mirror](fixtures/native-loader-v7/native_loader_v7.h)
fixes the nine-argument installation signature.

Prelude passes registration API, saved execution API, saved presence, imports,
exports, vtable enumeration and method observation, then the selected System
behavior and loadModule selector. Native Loader retains its first execution
selection before registration callbacks. Reentry or later registration does not
replace that selection. Same-pointer attachment repeats; invalid descriptors and
retargeting reject.

Direct loading forwards `(api, null, presence, imports, exports, vtables,
observation)` to the selected initializer. It adds no dependency lookup,
attachment, root, handle release or callback. Descriptors, contexts and native
code remain live alongside their matching execution API. Path ownership,
errors, return values and image encoding are unchanged.
