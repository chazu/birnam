# Roots

Roots owns the collector's retained references and ordered root providers. Its
initializer needs only Id and returns the Roots object before the collector or
upper value objects exist. Every message argument and result is an Oop.

| Message | Arguments | Result |
|---|---|---|
| `retain:` | Oop | Nonzero immediate-size token |
| `release:` | Token Oop | Immediate 1 when released, otherwise 0 |
| `releaseTokensProtecting` | Protected result followed by token Oops | Immediate 1 after cleanup; first failure after all cleanup attempts |
| `register:` | Root-provider Oop | The provider |
| `rootsDo:` | Visitor Oop | Null Oop |
| `referencesOf:do:` | Registry object and visitor | Null Oop |

A registered object implements `rootsDo:` and reports each root with `visit:`.
Registration keeps the provider itself alive. Order and duplicate registrations
are preserved, with no fixed provider ceiling. Enumeration snapshots registered
objects and retained references before callbacks; no registry borrow spans a send.
Tokens are unique and never reused, and exhaustion does not change existing roots.

`rootsDo:` reports the owned inventory and then invokes providers in order.
`referencesOf:do:` reports only the registry's owned references. Tracing the
registry object must not invoke providers again: Collect already requested their
roots explicitly. Each reported object is traced through its own reference owner.

`releaseTokensProtecting` keeps its result in the entered call while releasing
all supplied tokens in reverse order, including calls to a replaced `release:`.
The first failure is protected until the remaining cleanup attempts finish.
No native token-byte carrier or compatibility release interface is provided.

The Oop `roots` test exercises growth, order, duplicate registration, provider-only
child retention and reclamation after removing an edge. The core Oop gate also
covers Roots, Collect and owner retirement. The broader native-message harness is
still being migrated; it is not a completed acceptance claim. This module retains
the existing single-domain hosting model.
