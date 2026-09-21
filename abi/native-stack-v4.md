# Native Stack factory v4

Native Stack owns the stack anchor, Darwin arm64 register capture, interval
validation and scanning. The immutable 72-byte factory provides create, destroy,
set, get, scan and `capture(bottom, context, operation)`, then adds the lasting
`thread_bottom()` boundary.
Its provider is `birnam_native_stack_factory_v4`; the legacy Id preparation entry
is `birnam_native_stack_dependency_v4`. These replace the v3 exports. The public
32-byte setter capability and its admission behavior remain version 1.

`capture` saves callee-saved registers with Darwin's `_setjmp` into an aligned
192-byte area. It checks the downward-growing stack interval and the existing
128 MiB distance limit, then calls `operation(context, start, bottom)` exactly
once while that area is live. The callback runs below the capture frame; tracing
and sweep locals therefore do not enter the captured interval. The save area is
kept live after the callback, preventing a tail call or reuse during collection.

A return of 1 means the callback ran. Zero means it did not: the callback was
missing, the anchor was zero/below the save area/too distant, or the platform
has no supported capture implementation. Rejection reads no proposed stack
interval. The operation and its native context must stay live and must not unwind.
The caller supplies a live same-thread anchor; this API cannot validate the
lifetime of an arbitrary native address. Nested synchronous capture is supported.
No captured range may escape its callback.

`scan` retains the [v2 word-scanning contract](native-stack-v2.md): low-to-high,
complete aligned words, immediate emission, no allocation or heap-membership
policy, and the existing fatal diagnostic on sink rejection.

The [independent C mirror](fixtures/native-stack-v4/native_stack_v4.h) fixes every
layout. Production Collect receives this complete factory through Native Stack's
zero-argument native entry before Runtime. Runtime uses the adopted factory for
both collection and source-probe capture. Id contains only a synchronous callback
adapter; it no longer implements `_setjmp`, the save buffer or interval policy.
Legacy initialization mounts the same v4 factory. IdApi and image formats do not
change, and no extra library or new host preparation stage is introduced.

`scripts/fixtures/native-stack-scan.c` checks an actual ARM64 x19 value in the
save area, callback frame ordering, invalid intervals, nested capture and native
code ownership, alongside the scanner's byte-boundary and callback tests.
`scripts/id-native-loading-smoke.py PREFIX` also invokes capture through the
production collection owner before Runtime. The existing Native Stack image
workload covers ordinary, compiled, mapped/rebuilt and baked execution.

Run `scripts/native-stack-test.sh PREFIX` for the focused native and host checks.
The host integration test uses consistent unwind compiler flags to avoid Cargo's
abort/unwind rlib filename collision; the installed runtime remains the debug
abort build.

The last operation, `thread_bottom()`, returns the calling thread's high
stack address using Darwin's `pthread_get_stackaddr_np(pthread_self())`. This
boundary stays live after startup frames return. Collector activation installs
it before enabling automatic collection. A newly created anchor still defaults
to zero; merely constructing Native Stack does not enable GC. Later host-managed
execution can install a narrower live anchor through the existing setter.
