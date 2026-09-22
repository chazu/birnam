# Vector: retired native table

The versioned Vector function table, dependency attachment registry and installer
wrapper have been deleted from production under
[Design 84](../docs/design/84-message-only-native-modules.md). Do not implement or
consume this former ABI. Its historical C mirrors and legacy provider fixtures have also been deleted.

The common native initializer returns the opaque Vector prototype after installing
its methods and exports. Consumers send `new`, `add:`, `do:`, `inject:into:` and
`,` as needed. No consumer receives a Vector storage layout or a concatenation
function pointer. Replacement is observed on subsequent sends.

See [production construction](fixtures/bootstrap-v2/README.md) for the current
initializer operands. Remaining predecessor tables there are migration debt.
