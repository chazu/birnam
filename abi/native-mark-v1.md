# Native Mark v1

Design [81BP](../docs/design/81bp-production-native-mark-callback.md) moves the
active native mark scope and its emission policy into `libbirnam_native_mark`.
Id retains only per-floor/thread ownership, its context-free callback shim and
the Rust unwind guard. No IdApi or native initializer layout changes.

The immutable 56-byte descriptor is returned by `birnam_native_mark_provider_v1`.
It contains `version: u32`, `byte_size: size_t`, then five function pointers:
`create() -> context`, `destroy(context)`, `begin(context, sink)`, `end(context)`
and `mark(context, object)`. `begin` copies the unchanged 16-byte Trace Live v1
sink. The module owns the opaque context, which must remain on its owning
thread; each floor/thread owns a separate context. Creation/destruction invoke
no callbacks. They allocate/free only native scope storage, with no Id accounting.

The host pins the descriptor/code and passes it to Id's
`birnam_native_mark_dependency_v1` before initialization. Validation invokes no
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
