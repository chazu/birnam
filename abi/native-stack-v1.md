# Native Stack v1

[81BO](../docs/design/81bo-production-native-stack-anchor.md) introduced the
per-Runtime native root anchor. Production loads `libbirnam_native_stack.dylib`
through Id with no operands and receives the completed
[v4 factory](fixtures/native-stack-v4/native_stack_v4.h), which also owns scanning,
register capture and the platform thread-stack boundary. The early collector
creates the anchor and sets that boundary before Runtime adopts it. The host
selects its public setter through the completed `nativeStack`
[Runtime service](runtime-construction-v15.md#completed-service-selection).
No factory/provider export or dependency setter is used.

The v4 immutable factory provides create, destroy, canonical set/get, scan,
capture and thread-bottom operations.
The 32-byte floor descriptor is copied and supplies only ordinary owner-thread
and storage-observation admission. The 32-byte public capability supplies live
`set(context, address)` selection. Admission precedes assignment and holds no
service borrow; canonical operations bypass instrumentation of that public
descriptor. They do not invoke the floor, capture registers, scan memory or
collect. A newly created anchor starts at zero; production collector activation sets it
to the current thread boundary before publishing the active collector.

Assignment preserves every address bit, including zero and unaligned values.
It performs no address validation and does not collect or reset accounting.
Native Stack implements capture, interval validation and scanning. The collector
and Runtime own readiness checks and collection-phase ordering. The host keeps its selected boundary live
throughout the synchronous operation. Changing the anchor during collection
affects later collection attempts; the already-copied interval stays unchanged.
The anchor is process state and is not serialized into an image.

See the independent [setter declaration](fixtures/native-stack-v1/native_stack_v1.h)
and [Rust declaration](../rust/birnam-abi/src/native_stack.rs). Run
`scripts/native-stack-test.sh PREFIX` for the native C oracles, Runtime anchor
unit test and actual installed host preparation test. The C oracles load through
Id, explicitly clear the already initialized anchor for suppression tests, and
retain checks for callback/finalizer reassignment, native roots, allocation
budgets, all scan byte boundaries, register capture and exact rejection output.
`scripts/runtime-services-test.sh PREFIX` combines these C cases with Root
Registry and GC Settings. The separate `scripts/native-stack-image-smoke.sh PREFIX`
image gate still has legacy fixtures tracked for migration in Design 83.
