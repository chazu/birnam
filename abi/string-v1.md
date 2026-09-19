# Native String v1

`birnam-string` owns canonical String objects, their two-word pointer/length
payload, and an address-keyed map of owned byte buffers. It loads through the
siphon; Id has no normal Rust dependency on the implementation. IdApi v7 removes
`string_vt`, `mkString` and `stringBytes` (34 fields / 272 bytes). Rebuild native
consumers. The payload and image formats are unchanged.

`birnam_string_prepare_v1(IdApi *, const NativeAllocationV1 *)` registers partial
roots, creates the canonical behavior and empty prototype, declares RAW layout,
and installs clone/new once. The host prepares String after the collector is
mounted and before Word, Float and Integer. It attaches `StringApiV1` to itself,
the selected floor, and each declaring consumer before calling `module_init`.
Activation stays immediately before Collection: publish owner/exports once and
reinstall the fourteen existing String methods on every activation. Repeated
preparation must use the same IdApi. Native handles remain pinned on failure.

The 64-byte `StringApiV1` has version, extent, opaque context, behavior, prototype,
live `make` and `bytes` operations, and an immutable storage descriptor. Consumers
copy current operations before invoking callbacks. Id's internal consumers use
its admitted storage descriptor instead. No callback or state reference is held
across an ordinary callback that can collect or reenter.

The 80-byte `StringStorageV1` supplies payload extent/alignment, allocating
`construct` and `clone`, pure `borrow` and `release`, the default cleanup function,
and a pure `cleanup_owner` query. Borrowing uses the owner map regardless of raw
payload mutation or behavior reassociation. It returns null for an unowned object
and a stable non-null pointer for an owned empty string. Ordinary `bytes` rejects
unowned objects. Release removes only private native ownership. Borrowed bytes
remain valid while the buffer remains owned; callers must root objects across GC.

Clone/new ignore arity, accept null, copy the source's owned bytes twice as before,
canonicalize behavior and install default cleanup instead of inheriting a changed
source finalizer. Reinitialization does not overwrite clone/new replacements.
Retirement classifies cleanup as nonobserving only when its exact function reports
the matching String instance and the object still owns bytes. Wrappers, foreign
runtimes, replaced finalizers and unowned payloads retain ordinary settlement.

`birnam_native_allocation_provider_v1` on Id supplies a separate immutable,
32-byte `NativeAllocationV1`: version, extent, runtime context and allocator.
Its stack-scoped 48-byte plan has private context, behavior, payload size, optional
finalizer, prepare and initialize callbacks. The floor checks for GC first,
invokes prepare, allocates/associates a collectable block, invokes initialize,
and installs finalizer metadata directly. Both callbacks may copy/allocate private
native memory but must not call Id, collect, reenter, invoke arbitrary callbacks,
or retain the plan. This preserves String's original allocation schedule without
calling replaceable `IdApi.alloc` or `setFinalizer`. It contains no String policy.

The [independent C mirror](fixtures/string-v1/string_v1.h) asserts each extent.
Run `mise run string` and `scripts/string-image-smoke.sh PREFIX`; Id's focused
unit suite retains the default/wrapped/wrong-runtime cleanup differential oracle.
See [Design 81X](../docs/design/81x-production-string-storage.md).
