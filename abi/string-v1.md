# Native String v1

`birnam-string` owns canonical String objects, their two-word pointer/length
payload, and an address-keyed map of owned byte buffers. Bootstrap constructs it
through Id before Error or numbers; Id has no normal Rust dependency on the
implementation. The [production entries](fixtures/string-v1/README.md) provide
byte storage first, then a native text/numeric extension once Integer and Float
exist. Both stages complete before Runtime. Payload and image formats are unchanged.

The old String preparer, provider, dependency setters and aggregate `module_init`
entry are removed. Bootstrap retains the native constructor result and passes
its table explicitly to consumers. Native handles remain pinned after entry,
including failure. Historical C clients using the old initialization path still
require migration.

String split: lives in the separately loaded [String collection adapter](string-collection-v1.md),
which loads after Vector. Collection installs it after String parenting, retaining
its original execution table and native Vector construction. The early String
module has no Vector dependency.

The 64-byte `StringApiV1` has version, extent, opaque context, behavior, prototype,
live `make` and `bytes` operations, and an immutable storage descriptor. Consumers
copy current operations before invoking callbacks. Upper Runtime's storage consumers use
its admitted storage descriptor instead. No callback or state reference is held
across an ordinary callback that can collect or reenter.

The 80-byte `StringStorageV1` supplies payload extent/alignment, allocating
`construct` and `clone`, pure `borrow` and `release`, the default cleanup function,
and a pure `cleanup_owner` query. Borrowing uses the owner map regardless of raw
payload mutation or behavior reassociation. It returns null for an unowned object
and a stable non-null pointer for an owned empty string. Ordinary `bytes` rejects
unowned objects. Release removes only private native ownership. Borrowed bytes
remain valid while the buffer remains owned; callers must root objects across GC.

Native clone/new validate arity and require owned String bytes.
Reinitialization does not overwrite clone/new replacements.
Retirement classifies cleanup as nonobserving only when its exact function reports
the matching String instance and the object still owns bytes. Wrappers, foreign
runtimes, replaced finalizers and unowned payloads retain ordinary settlement.

The old exported Native Allocation provider is removed. Runtime's internal Rust
storage harness still uses the allocation-plan helper; it is not a production
constructor or a service provided by Id.

The [independent C mirror](fixtures/string-v1/string_v1.h) asserts each extent.
The production native suite is `scripts/id-native-loading-smoke.py PREFIX`;
`scripts/string-image-smoke.sh PREFIX` covers image execution. Runtime's focused
unit suite retains the default/wrapped/wrong-runtime cleanup differential oracle.
See [Design 81X](../docs/design/81x-production-string-storage.md).

Upper consumers receive the matching export capability explicitly. Each
publication reads the live function/context after the owner callback. See
[Namespace export](namespace-export-v1.md).
