# Finalizers v1

Native finalizer registration and inline attachment construction move from Id
into `libbirnam_finalizers.dylib` in [81BN](../docs/design/81bn-production-finalizers.md).
Collect retains detach/call/reclaim ordering; Id retains exact allocation facts
and storage. Native payload owners retain their cleanup functions.

`birnam_finalizers_factory_v1` supplies an immutable 40-byte factory with
`create`, `destroy` and canonical `slot(permanent, callback)` operations. The host
pins the image and attaches the factory through
`birnam_finalizers_dependency_v1` before `id_init`. Attachment validates without
calling operations, accepts the same pointer again and rejects retargeting.

The 48-byte floor descriptor is copied into each capability. Its ordinary
admission callback runs before exact permanence lookup. The reader reports a
missing record without writing output or dereferencing the object. Present
records report zero for collectable or one for permanent. The module constructs
an attachment only for collectable records and writes it synchronously through
the floor. Fact and attachment callbacks do not allocate, collect or run user
code. No floor borrow crosses admission.

The public 32-byte capability exposes `set(context, object, callback)`.
Id publishes it through `birnam_finalizers_provider_v1` as the twelfth native
initializer argument. Consumers retain the capability matching their saved
execution API, reading its live callback and context on each registration.
Descriptors, callback code and context must outlive retaining consumers.

Each allocation retains an eight-byte slot. A canonical empty result means
registration does not apply; it is never a request to clear an existing
callback. There is no public clearing operation. Replacement neither calls the
old callback nor adds a root or changes object identity/accounting. Cloning
does not copy a finalizer. Presence retains missing/present-without/present-with
distinctions. Permanent, null, interior and foreign addresses remain silent
no-ops after admission.

Id's internal Machine, Continuation and native-allocation construction use the
immutable factory operation, independent of public descriptor instrumentation.
Collect selects each current callback when detaching its object, then invokes
it after releasing floor borrows. Re-registration of that detached object is
ignored; later dead objects can still receive replacement callbacks. Runtime
teardown deallocates raw storage without running finalizers.

The [independent C header](fixtures/finalizers-v1/finalizers_v1.h) and
[Rust declaration](../rust/birnam-abi/src/finalizers.rs) assert sizes and offsets.
IdApi v19 removes only `setFinalizer`; object headers, heap-record size and image
v8 encoding are unchanged. Consumers must rebuild.

Run `mise run finalizers` for focused debug registration, admission and loaded
Collect checks. The design document tracks the broader completion evidence.
