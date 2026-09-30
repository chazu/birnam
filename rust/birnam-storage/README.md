# Storage diagnostics

Storage is a production native module using only Id messages. Bootstrap publishes
`Storage` in the namespace. `observe: visitor` gathers physical allocations,
Id/Roots roots, core references and references from GC's current registered owners.
It then sends the visitor:

- `allocation:bytes:identity:` — Object, Word requested bytes, Word allocation identity;
- `root:` — Object;
- `edgeFrom:to:` — two Objects;
- `complete:` — Word coverage gaps; its result becomes the observation result.

Allocation records are ordered by identity. Every root and edge endpoint must
belong to the initial inventory. A provider emitting a foreign endpoint fails the
observation even if it ignores the callback's error. References are enumerated by
their current owners; Storage never reads another module's payload or reconstructs
old layout/provider tables. Owner errors propagate unchanged.

Gap bit `1` means native stack roots are unobserved; bit `2` means a nonempty
ordinary allocation has no reference owner. Coverage is of the current declared
reference graph, including an owner's conservative policy where applicable.
These are sequential observations, not an atomic heap snapshot or a claim of
representation ownership independent of behavior reassociation.

The inventory is rooted throughout owner and report callbacks, including calls
that collect. Those temporary roots are excluded from the diagnostic root stream.
No GC setting is changed. Root and reference providers must not mutate the
observed graph; allocations outside the inventory cannot become its endpoints.
Each observation allocates a private callback receiver, included in its inventory.
Retained callback receivers fail closed after return and cannot write into a later
observation. Nested observations fail with native INVALID_REQUEST detail `2`;
malformed facts use detail `1`. Callback failures retain their original Outcome.

Object arguments are borrowed for the synchronous observation. Retain explicitly
before keeping an Object afterwards. Allocation identities are scoped to this Id
instance and never authorize dereferencing a stale Object or reclaiming storage.

The retired description-offset ABI, observation permit service and deferred
identity/index bookkeeping are removed. Collection remains in `birnam-collect`.
Run `bash scripts/native-message-test.sh` for the production `storage_trace` test.
