# Native Integer dependency, v1

`libbirnam_integer.dylib` owns Integer's canonical behavior and prototype, signed
64-bit payload, primitive construction/reading/recognition, and thirteen methods.
It depends on IdApi v5. Id boots without Integer; its remaining Machine and
optimizer components explicitly acquire Integer when the host composes the stack.

The host initializes Integer immediately after Id, obtains its table through
`birnam_integer_provider_v1()`, and passes it to each consumer's optional
`birnam_integer_dependency_v1(IntegerApiV1 *) -> int` before `module_init`.
The host also attaches itself and the still-combined Machine library. Success is
1; malformed or different repeated dependencies return 0. The same table can be
reattached. A missing dependency fails mounting before the consumer initializes.
`module_init`'s existing opaque prior-state contract is unchanged.

The [Rust mirror](../rust/birnam-abi/src/integer.rs) and
[independent C header](fixtures/integer-v1/integer_v1.h) define an 80-byte operations
table and a separate 48-byte immutable storage descriptor on arm64 macOS.
The operations table carries version/extent, context, canonical identities,
`make`, `read`, `is_exact`, the storage descriptor, and `allocation_context`.
Consumers copy the live table before calling an operation; instrumentation can
replace entries on the owning thread. Tables, context, and native code remain
live for the existing process lifetime of a mounted module. This introduces no
new unloading policy and no aggregate replacement for IdApi.

`make` preserves direct native allocation with a collection opportunity and a
fresh identity for every result. It does not send `allocate`. `read` requires a
live Integer payload and remains valid after that object's behavior is changed.
`is_exact` checks heap membership and the original canonical behavior, rejecting
null, foreign addresses, and descendants. Image handling deliberately retains
its separate inherited-behavior rule. Rebinding `Integer` or `IntegerVt` does not
change the module's canonical identities; an explicit root provider retains both,
including throughout partially initialized mounting.

The immutable codec describes payload extent/alignment and pure `initialize` and
`read` callbacks. Those callbacks access only the supplied bytes: no Id calls,
allocation, collection, reentry, or mutable module state. The current payload is
one signed 64-bit word, size/alignment 8. Recovery rechecks admission and layouts
after its safepoint, allocates generic storage, initializes it, and immediately
roots the result. Deferred allocations encode opaque bytes in aligned untraced
native storage before charging; Heap settles those bytes without codec callbacks.
Equal values still incur separate identities and charges. Counted slot indices
retain their separate bounded ledger and do not consume the general journal.

Optimization requires the exact admitted identities, context, code and codec,
and the original Id allocator. `allocation_context` is a pure scalar query of the
constructor's current allocator; wrappers withdraw admission before any charge or
Frame write. Native numeric declarations use [v2](native-numeric-semantics-v2.md).
The private closure-elision hook is now `birnam_id_try_closure_elision_v2`, carrying
the producer's actual Integer table alongside its IdApi. Generated code's TCC
bridge names, Method ABI, Frame descriptors and durable image bytes are unchanged.

All native consumers must be rebuilt for IdApi v5. Historical IdApi manifests are
retained as evidence, not compatibility shims. The independent C fixture checks
bare Id, arithmetic with a signed-128-bit oracle, errors, fresh identity, mixed
Float behavior, replacement, reassociation, namespace rebinding, and collection.

Float must be prepared and its explicit dependency attached before Integer
initialization; see the [Float mounting protocol](float-v1.md).
