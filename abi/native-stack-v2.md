# Native Stack factory v2

Historical contract; current production uses [factory v3](native-stack-v3.md),
which also owns register capture. The v2 factory exports have been removed.

[81BQ](../docs/design/81bq-production-native-stack-scanning.md) moves native word
scanning into Native Stack. The immutable 56-byte factory adds a synchronous
`scan(sink, start, bottom)` operation after create/destroy/canonical set/get.
Its provider is `birnam_native_stack_factory_v2`; Id mounts it through
`birnam_native_stack_dependency_v2` before initialization. All five operations
must be present. Validation is inert, repeats the same pointer and rejects
retargeting. These replace the current v1 factory/dependency exports.

The [v1 public setter capability and copied floor](native-stack-v1.md) retain
exactly their existing layouts and behavior. The public capability provider
remains `birnam_native_stack_provider_v1`. Canonical create/destroy/set/get are
callback-free. The scanner uses the unchanged 16-byte Trace Live v1 sink, with
one synchronous callback for each complete word in the supplied live interval.

Scanning rounds the start upward to word alignment, checks each exclusive end,
reads low to high and emits immediately. It preserves zeros, duplicates and all
pointer bits. A callback can affect later reads; the scanner does not copy the
interval eagerly, allocate, query the anchor, perform Id admission or decide
heap membership. A rejected emission aborts before the next word with the exact
historical `GC tracer rejected a candidate` invariant diagnostic.

The caller provides a non-null readable sink, live context/code and a readable
interval with representable start rounding. Empty `(0, 0)` is valid. Physical
register capture, downward-distance/128 MiB validation, source-probe ranges and
collection ordering remain in Id. The scanner must run below the captured
native-root interval; it does not manufacture a new capture boundary.

IdApi stays v20, initialization stays at twelve arguments, Native Loader stays
v12 and images stay v8. Native Stack remains a regular siphon module; no new
library is added. Historical [v1 C declarations](fixtures/native-stack-v1/native_stack_v1.h)
remain frozen. Current [v2 C declarations](fixtures/native-stack-v2/native_stack_v2.h)
reuse the unchanged v1 public capability and floor.

Run `scripts/native-stack-test.sh PREFIX` for focused scanner/anchor and actual
host preparation checks; `scripts/native-stack-image-smoke.sh PREFIX` verifies
startup, compiled callers, mapped/rebuilt native Code images and baked execution.

Current production constructs and owns these controls in the early
[collection-state owner](collection-state-v1.md), then Runtime adopts the same
context. Normal startup no longer prepares this dependency inside Id. The
existing factory ABI remains available for legacy clients.
