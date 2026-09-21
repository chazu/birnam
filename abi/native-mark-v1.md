# Native Mark v1

Design [81BP](../docs/design/81bp-production-native-mark-callback.md) moves the
active native mark scope and its emission policy into `libbirnam_native_mark`.
Current production constructs the scope in the early native Trace graph. Trace
owns the context-free callback shim and scope guard; roots and hooks receive the
same Trace callback identity. The legacy Runtime adapter retains its old shim
for legacy initialization and unit consumers. No IdApi layout changes.

The immutable 56-byte descriptor is returned by `birnam_native_mark_provider_v1`.
It contains `version: u32`, `byte_size: size_t`, then five function pointers:
`create() -> context`, `destroy(context)`, `begin(context, sink)`, `end(context)`
and `mark(context, object)`. `begin` copies the unchanged 16-byte Trace Live v1
sink. The module owns the opaque context, which must remain on its owning
thread; each live graph owns a separate context. Creation/destruction invoke
no callbacks. They allocate/free only native scope storage, with no Id accounting.

The host pins the descriptor/code. Only legacy Runtime preparation passes it to
Id's `birnam_native_mark_dependency_v1` before upper initialization. Validation invokes no
operations, accepts repeated attachment of the same descriptor, and rejects
retargeting. All five operations must be present. The module mounts through the
unchanged twelve-argument initializer without publishing language objects.

`begin` rejects an active scope before reading the supplied sink. `end` clears
that scope. `mark` requires an active scope even for a null object; a non-null
object emits its exact address immediately through the copied sink. It retains
no state borrow across emission. Null marks, duplicate marks, synchronous
emission reentry and callback use in a later scope retain their prior meaning.
Nested scope entry, escaped callbacks and rejected emission preserve their
historical abort diagnostics. Scope lifetime is synchronous and borrowed sink
context/code must remain valid throughout it.

Independent C mirror: [native_mark_v1.h](fixtures/native-mark-v1/native_mark_v1.h).
The implementation descriptor is not a replacement public IdApi entry.

The same complete descriptor is available through
`libbirnam_native_mark.dylib:birnam_native_module_v1` with zero operands. Its Word
result is an input to the [early live Trace graph](trace-graph-v2.md), before any
Runtime is constructed. Existing factory/provider consumers remain supported.
