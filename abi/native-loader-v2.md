# Native Loader installation v2

Design [81AU](../docs/design/81au-production-native-loader.md) moves System
loadModule: intact from Prelude into `birnam-native-loader`. Text View v1 and
Error Value v2 are its dependencies. It loads before Prelude; initialization
enables its provider without creating Id objects, methods, roots or handles.

Attach its immutable process-pinned descriptor with
`birnam_native_loader_dependency_v2`. The arm64 layout is 24 bytes: version 2 at
0, byte size at 8 and install at 16. Admission rejects null, wrong version/extent,
missing install and retargeting; repeating the same pointer succeeds. The
[independent C mirror](fixtures/native-loader-v2/native_loader_v2.h) checks layout.

Install takes const registration/execution IdApi pointers, an execution presence pointer, the selected System
behavior and already interned loadModule: selector. Prelude retains System
identity/adoption, all selector interns and the original binding position, after
load: and before readLine. Retain Prelude's first execution pointer before the
registration callback, even when registration reenters with a different table.
Reinstallation keeps that pointer. Use behavior/selector synchronously without
retaining or reinterning them.

Wrong arity, null carriers and unavailable exact String paths return current
nil. There is no printing fallback. Copy the borrowed bytes into a CString;
interior NUL produces ioError with "could not load module dylib". No UTF-8 check,
path normalization or cache is added. The owned path survives loader callbacks,
including changes to the original borrowed storage and nested loads.

Read the current dlopen callback and call it with RTLD_LAZY (1). A null handle
uses the same ioError. Then read the current dlsym callback for module_init; a
missing symbol returns notAModule with "dylib has no module_init". Call the
returned initializer with the original execution-table pointer and null state.
Ignore its result and return the original receiver, including null. Preserve
existing handle and CString lifetimes, Error construction order and callbacks.

This direct System loader does not perform the host's dependency admission or
unload handles. Repeated loads still invoke module_init again. Its method,
descriptor and original execution table remain pinned. public protocol
and image encoding are unchanged. The [export inventory](rust-native-loader-dylib-v1.json)
declares five symbols. Run `mise run native-loader` and
`scripts/native-loader-image-smoke.sh PREFIX` for focused debug checks, including
real loads in startup, source-free compiled modules and saved/baked images.

Design [81BB](../docs/design/81bb-production-namespace-presence.md) versions this
installer to v2. Its install function receives the execution API's explicit
`const NamespacePresenceV1 *` immediately after the execution API pointer.
Capture the pair together at the original first-install point, before callbacks;
later registration does not replace either execution selection. The caller pins
the live descriptor, its context and code for the retained method lifetime.
Each presence check selects the current function/context at invocation.
