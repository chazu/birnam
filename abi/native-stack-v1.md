# Native Stack v1

[81BO](../docs/design/81bo-production-native-stack-anchor.md) moves each Runtime's
native stack-anchor state into `libbirnam_native_stack.dylib`. The host prepares
and pins `birnam_native_stack_factory_v1` before Id initialization, attaching it
through `birnam_native_stack_dependency_v1`. The floor exposes the resulting
capability through `birnam_native_stack_provider_v1` for host execution setup.

The 48-byte immutable factory provides create, destroy, canonical set and get.
The 32-byte floor descriptor is copied and supplies only ordinary owner-thread
and storage-observation admission. The 32-byte public capability supplies live
`set(context, address)` selection. Admission precedes assignment and holds no
service borrow; canonical operations bypass instrumentation of that public
descriptor. They do not invoke the floor, capture registers, scan memory or
collect. Native Stack owns a separate allocation per Runtime and starts at zero.

Assignment preserves every address bit, including zero and unaligned values.
It performs no address validation and does not collect or reset accounting.
Id retains register capture, interval validation, scanning, readiness read
points and collection-phase ordering. The host keeps its selected boundary live
throughout the synchronous operation. Changing the anchor during collection
affects later collection attempts; the already-copied interval stays unchanged.
The anchor is process state and is not serialized into an image.

IdApi v20 removes only `gcSetStackBottom`: 18 fields / 144 bytes. Native modules
must rebuild, but initialization still has twelve arguments and Native Loader
stays v12: no ordinary production module consumes the anchor setter. Host
loading obtains the capability directly from the selected floor.

See the independent [C declaration](fixtures/native-stack-v1/native_stack_v1.h)
and [Rust declaration](../rust/birnam-abi/src/native_stack.rs). Run
`mise run native-stack`, or `scripts/native-stack-test.sh PREFIX` to also check
the real host's preparation sequence against an installed debug prefix.
`scripts/native-stack-image-smoke.sh PREFIX` checks installed startup,
source-free compiled callers, mapped/rebuilt images and baked execution.
