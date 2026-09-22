# System GC: retired native table

The versioned System GC function table, dependency attachment registry and installer
wrapper have been deleted from production under
[Design 84](../docs/design/84-message-only-native-modules.md). Do not implement or
consume this former ABI. Its historical C mirrors and legacy provider fixtures have also been deleted.

The common native initializer installs the module's behavior and returns Unit.
Repeated successful initialization also returns Unit. There is no separate
installation API for Bootstrap or the host to attach.

See [production construction](fixtures/bootstrap-v2/README.md) for the current
initializer operands. Remaining predecessor tables there are migration debt.
