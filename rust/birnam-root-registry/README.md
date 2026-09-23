# Roots

Roots keeps references alive for the collector. Its initializer needs only Id and
returns the Roots Object, before the collector or upper value objects exist.

| Message | Arguments | Result |
|---|---|---|
| `retain:` | Object | Nonzero native Word token |
| `release:` | Token Word | Word 1 when released, otherwise 0 |
| `register:` | Root-enumerating Object | Unit |
| `rootsDo:` | Synchronous NativeEntry visitor | Unit |

A registered object implements `rootsDo:` and reports each GC root as the visitor's
single Object argument. Namespace, for example, reports bindings stored in native
memory. Registration keeps the enumerator itself alive. The collector traces the
objects reported by these enumerators using the normal reference-owner protocol.

Registration grows with the existing vector; there is no fixed enumerator ceiling.
Order and duplicate registrations are preserved. Enumeration snapshots registered
objects and retained tokens before callbacks, and no registry borrow spans a send.
Changes made during visitation appear in the next snapshot. Retention tokens are
unique and never reused; token exhaustion fails without changing existing roots.

Run `mise run native-messages` for production registration, visitation, GC and
retirement checks. The process-owned initial Roots object still follows the current
single-domain hosting model; this change does not introduce thread-hosted domains.
