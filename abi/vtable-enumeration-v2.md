# Vtable enumeration v2

Object Model supplies a native capability for counting registered behaviors,
selecting one by index, and selecting a behavior's ordered method keys. Id owns
those registries and dictionaries. Object Model is fully constructed before
Runtime and selects pinned Id entries through `lookup` at construction:
`behaviorCount`, `behaviorAt:` and `selectorAt:in:`. It invokes those exact entries
for native enumeration. Replacing application lookup does not change raw
reflection or cause enumeration to execute application code.

The [independent C mirror](fixtures/vtable-enumeration-v2/vtable_enumeration_v2.h)
defines the 48-byte capability: version 2, native extent, context at offset 16,
`count` at 24, `at` at 32 and `selector_at` at 40. Each operation copies the
selected callback and context before invoking them. Descriptor copies borrow the
original owner's lifetime. Replacing a shared context requires callbacks that
accept that context; an active call retains its selected pair.

Views are transient, unrooted borrows, not snapshots. Selection observes the
current registry or dictionary on every call. Bounds misses return null.
Replacement retains key position; removal shifts successors; re-addition appends.
Parent dictionaries and custom lookup are not consulted. Collection removes dead
behaviors while preserving survivor order. Callers must root retained objects.

Native enumeration follows Design 83's trusted native-call contract: the caller
supplies owner-thread access and, for `selector_at`, a registered behavior. There
is no future Runtime admission hook in Object Model. These read-only operations
are available during root collection and diagnostic storage observation. This is
distinct from Runtime-facing namespace operations, which retain their existing
thread and observation checks. The diagnostic observer itself retains its
scoped, non-reentrant contract.

Typed Id requests validate operands and behavior membership before selecting a
key: a live instance that is not a behavior yields `InvalidRequest` even for
`SIZE_MAX`; a non-Word index also yields `InvalidRequest`. This checked request
path is separate from the trusted native capability's calling convention.

Production construction returns the enumeration capability within
[Object Model's owner](fixtures/object-model-owner-v1/README.md). Bootstrap passes that completed owner
to Runtime; later clients select `vtables` through
[Runtime service requests](runtime-construction-v15.md#completed-service-selection).
The old provider/factory exports, dependency setter and aggregate module
initializer are removed. The historical Factory/Floor descriptors remain in the
frozen C fixture and internal factory unit tests; they do not describe the
production construction path.

Run `mise run reflection-boundaries`, or with a debug installation:

```sh
bash scripts/runtime-services-test.sh PREFIX reflection
python3 scripts/id-native-loading-smoke.py PREFIX
```

These exercise ordered growth, replacement, removal/re-addition, bounds, weak
membership and survivor order, GC callbacks, observation-time reads, and typed
request failures. Native misuse probes that supplied foreign threads or arbitrary
and stale pointers to the trusted capability are retired. The older consumer
and image fixtures in `vtable-enumeration-test.sh`, `selector-enumeration-test.sh`
and their image runners still require migration from aggregate initialization;
the focused reflection matrix does not claim those consumer suites pass.
