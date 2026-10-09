# Roots

Roots keeps references alive for the collector. Its initializer needs only Id and
returns the Roots Object, before the collector or upper value objects exist.

| Message | Arguments | Result |
|---|---|---|
| `retain:` | Object | Nonzero native Word token |
| `release:` | Token Word | Word 1 when released, otherwise 0 |
| `releaseMany:protecting:batch:` | Borrowed native token bytes, result Object, batch Word 0/1 | Word 1 after cleanup; first release failure after all cleanup attempts |
| `register:` | Root-enumerating Object | Unit |
| `rootsDo:` | Visitor Object | Unit |

A registered object implements `rootsDo:` and reports each GC root by sending
`visit:` with an Object argument to the visitor. Each emission uses current lookup. Namespace, for example, reports bindings stored in native
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

`releaseMany:protecting:batch:` copies the token request before callbacks and reports
the result through an owner-local scope until the complete cleanup returns. It
preserves reverse release order, including current custom release callbacks. A
canonical Id binding proof admits callback-free owner batching; other cases send
each current `release:`. The scope drains on success/failure and nested calls
restore the parent suffix. Replacements must honor this cleanup/handoff contract.
No extra public retention token is allocated for the overlap.

Batch Word 0 retains the diagnostic compiler control: every ordinary release
is dispatched individually while the result still has owner coverage.
