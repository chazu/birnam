# Representation validation and remaining lifetime work

The starting trace has 242 sends inside successful positive Integer addition.
41 read allocation size or behavior. Another four obtain allocation identity.
The lifetime category contains 60 sends, with additional protection inside other
operation categories. The aim is to preserve Id as the dispatch floor, current
getters/allocators, identity, GC coverage, errors and cold bootstrap support.

## Atomic allocation facts

Id implements the ordinary message
`tryRepresentationIdentityOf:behavior:bytes:` with Object value, Object expected
behavior and native Word byte count. It returns native Unit to decline, Word zero
for mismatch, or the allocation's nonzero native Word identity for an exact match.
An identity that cannot be represented as nonzero declines. This carries only
Id-owned metadata; upper owners still read only their own payloads.

The canonical query checks its own current entry/context and the current
`behaviorOf:`, `allocationSizeOf:` and `identityOf:` entries/context, plus the
actual standard Id lookup and self-metabehavior. It uses Id's dictionaries and
allocation inventory without callbacks, allocation or a saved executable call.
Changed meanings decline. No epoch/shape verdict is reused across invocations.
`usesDefaultAllocation` includes the new query in its meaning check, so guarded
Word allocation omission cannot ignore replacement of a new dependency.

Integer and Word use this query before payload access and post-allocation writes.
ObjectModel uses its returned identity instead of three separate metadata sends
when admitting an allocation. A positive reply is consumed before another Id
call. Caller/owner scopes keep the actual Object alive for that interval.

Unit or UNBOUND executes the previous getter sequence. Numeric reads still ask
behavior then size, with the same short-circuit; they do not acquire a new legacy
identity dependency. ObjectModel still asks identity, behavior, then size.
Other failures and malformed replies fail. The original allocator is still
called and an incorrect allocation is still rejected. Replacement native query
providers must honor the certificate contract; arbitrary dishonest metadata
providers are not sandboxed by this API, just as with the original getters.

This removes 21 sends in the measured default path: 13 two-getter checks become
one query, and four three-getter allocation admissions become one query. It
retains 17 fresh representation queries rather than assuming prior checks remain
valid across callbacks or ownership boundaries. Decline adds a query attempt
before the original calls; the optimization is for canonical meanings.

## Lifetime audit and changes

| Location | Decision and reason |
| --- | --- |
| Map receiver/arguments | Borrow the native caller's protection; avoid duplicate registrations. New nodes, internal key/value snapshots and temporary results still use the existing Scope. |
| Map's captured root node | Keep its local root. A collecting hash callback can rehash the same Map and replace its root; the old node is still needed for the lookup. |
| Symbol `collectionHash` receiver | Borrow the caller-held Symbol. Consume owner-returned bytes immediately, with no later callback or cleanup. |
| Reflect `vtableOf:` receiver/value | Forward with caller-held inputs and no cleanup after the returned result. Other reflection operations keep their scopes; the existing nil lookup and behavior query remain live. |
| ObjectModel allocation-permission argument | Borrow the caller-held behavior for this fixed native Word query. Preserve all permission/getter calls and failures. Other clone/metadata paths remain unchanged. |
| Generated named-method adapter result | Use the existing `return_call` empty-cleanup rule. Resolve the selector first; a cold cache or nonempty scope still uses the protected cleanup path. This removes immediate retain/release round trips around pure forwarding adapters. |
| Generated locals and intermediate results | Retain values needed across later sends or cleanup callbacks. Captures/input aliases/Unit-owned Symbols already reuse coverage. Do not infer protection for an arbitrary fresh result merely from its producing receiver. |

These change private protection inventories and internal token/callback histories,
as in the preceding input-borrowing slices. Explicit language Root operations
retain their current Id behavior. No new Root lease, global duplicate coalescing,
foreign context/payload access, virtual frame or frame-elision machinery is added.

## Validation

Lean checks certificate/result selection and the current-meaning requirements.
TLA+ checks stale/changed certification, no callback between positive certification
and reading, fallback order and Map snapshot coverage after mutation. The existing
root-free final handoff and caller-input coverage proofs are reused. These models
are not verification of the Rust/C implementations or arbitrary native providers.

Native tests check matches/mismatches, reassociation, malformed requests,
forwarding queries, custom lookup, getter changes, collecting/failing fallback,
caller-held inputs, reentrant Map rehash and snapshot cleanup. Standard adjacent
value, ABI, allocator, generation, task and image checks cover integration.

## Measurement and acceptance

[Receipt](../measurements/representation-validation-and-lifetime-20261009.json)
records the hashed traces, libraries, timing qualifications and validation logs.
The temporary release build and saved-image startup gate pass. No user installation
or compiled-code loading policy changed.

| Inside successful `+` | Before | After |
| --- | ---: | ---: |
| Total sends | 242 | 195 |
| ABI conversion subtree work | 45 | 33 |
| Word arithmetic/guard subtree work | 42 | 31 |
| Other lifetime/cleanup work | 60 | 36 |
| Behavior/size getters | 41 | 7 |
| Allocation identity getters | 4 | 0 |
| Atomic representation queries | 0 | 17 |
| All individual retains | 36 | 23 |
| All individual releases | 24 | 11 |
| Allocated Objects | 4 | 4 |

The saving is 47 sends (19.4%): 21 from metadata, 12 from borrowing native inputs
(Map 4, Reflect 4, Symbol hashing 2, permission queries 2), and 14 from named
adapter result handoff. The benchmark including fixture work measures 243.375
versus 196.25 sends per addition; the fixture also benefits from adapter handoff.
Intermediate prefixes with some older authored code measured 199 and 197; the
fully rebuilt prefix measures 195. Actual operation counts exclude fixture roots.

The remaining lifetime category is 21 retains, 11 individual releases and four
batch-cleanup/current-binding sends. Two additional retains sit in Word operation
subtrees. Most remaining values are held through later helper, lookup, validation
or cleanup calls. The node snapshot is specifically necessary in the rehash
experiment. No claim is made that all other registrations form a theoretical
minimum. Eliminating further result protection needs a separate covering scope
or callback-free handoff proof; arbitrary fresh results cannot be treated as
permanently reachable aliases of their producers.

Separate four-second CPU samples observed 38.55 versus 37.62 microseconds per
addition, about 2.4% lower. This small change is directional only on a shared
machine. Send reduction is deterministic; it is not a CPU-share estimate, and the
new query still performs current-binding checks internally. The paired profiles
within each prefix both exercise production defaults and are not this change's
before/after comparison. Source/library stability checks passed.

Acceptance passed 43 focused Rust/C-harness tests, 22 native checks across 17
binaries, 14 task assertions at quanta 128 and 1, 128 Lean theorem checks, and all
78 TLC configurations including required counterexamples. Native checks include
collection, failure, changed providers, rehash snapshots, numeric boundaries,
ABI, captures, generation/code lifetime and ordinary collection behavior. The
exhaustive project matrix and a startup speed benchmark were not rerun.
