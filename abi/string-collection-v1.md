> Historical table contract, retired by Design 90. Current String behavior is documented in
> [the owning module](../rust/birnam-string/README.md). Do not restore this descriptor.

# String Collection: retired native table

The versioned String Collection function table, dependency attachment registry and installer
wrapper have been deleted from production under
[Design 84](../docs/design/84-message-only-native-modules.md). Do not implement or
consume this former ABI. Its historical C mirrors and legacy provider fixtures have also been deleted.

The common native initializer installs the module's behavior and returns Unit.
Repeated successful initialization also returns Unit. There is no separate
installation API for Bootstrap or the host to attach.

Vector dependencies are opaque Object operands. Construction and iteration use
ordinary Vector messages, including Range concatenation and Map iteration.

See [production construction](fixtures/bootstrap-v2/README.md) for the current
initializer operands. Remaining predecessor tables there are migration debt.
