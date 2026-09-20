# Trace Hooks v1

This native module owns hook registration and bounded inherited-hook selection.
See [81BM](../docs/design/81bm-production-trace-hooks.md) for the ownership audit
and validation status. The existing Trace module owns traversal; Id invokes
selected hooks at the existing post-payload GC call site.

`birnam_trace_hooks_factory_v1` supplies an immutable 48-byte factory with
version, extent, capability creation/destruction, inline slot construction and
inherited selection. The host pins its image and attaches it through Id's
`birnam_trace_hooks_dependency_v1` before `id_init`. Attachment invokes no
callbacks, accepts the same pointer again and rejects retargeting. Ordinary
module initialization publishes no methods or objects.

The 40-byte floor descriptor contains version, extent, context, ordinary admission
and exact-record replacement callbacks. Creation copies it without invoking it.
The 32-byte public capability contains version, extent, context and `set`.
Id publishes it through `birnam_trace_hooks_provider_v1`; native initializers
receive it as argument eleven, paired with their execution API. Registration
admits before exact membership lookup, replaces the hook in separate per-behavior metadata. Design 83 removes the raw
payload mirror. Registration adds no clear operation, root or mutation-epoch
change.

An eight-byte inline slot contains the optional hook. An entry copies a parent
and slot into 16 bytes. A 24-byte synchronous source carries context, the copied
record-count budget and an exact-entry reader. Selection checks the supplied
vtable first and follows current parent facts until finding a hook, missing a
record, reaching null or exhausting the budget. The reader returns one for
membership and zero for absence, invokes no user code, and cannot allocate or
mutate the registry. No source is retained. Id ends the registry borrow before
invoking the returned hook. Public descriptor instrumentation cannot redirect
canonical selection or internal slot construction.

Descriptors, contexts and code must outlive retaining consumers; hook code must
outlive its registration. Id keeps record membership and retirement authority.
There is no per-record allocation, duplicate registry or image-format change.

The [C header](fixtures/trace-hooks-v1/trace_hooks_v1.h) and
[Rust declaration](../rust/birnam-abi/src/trace_hooks.rs) assert sizes and offsets.
Run `mise run trace-hooks` for the focused debug gate.
