# Collection value policy v1

Design [81AD](../docs/design/81ad-production-collection-value.md) moves the private
comparison shared by Vector `indexOf:` and Map keys, plus Map's hash functions,
into `birnam-collection-value`. This introduces no language selectors.

The host loads the service after Integer/Float/String and before Vector and Map,
attaches those live dependencies, obtains `birnam_collection_value_provider_v1`
and mounts it through `birnam_collection_value_dependency_v1`. The immutable
descriptor and code remain pinned for the process. Initialization only enables
the provider; it owns no objects, roots, layouts or selected Id table.

The arm64 descriptor is 32 bytes: version 1 at offset 0, `size_t byte_size` at 8,
and two function pointers at 16 and 24. See the independent
[C definition](fixtures/collection-value-v1/collection_value_v1.h). Null, wrong
version/size, missing operations and retargeted descriptors are rejected;
reattaching the same descriptor succeeds.

| Operation | Existing semantics |
| --- | --- |
| `same_value(api, left, right)` | C int 0/1. Identity first, including null/null; then null rejection, exact Integer values, exact Float values, and exact String byte equality. Other values only compare equal through identity. |
| `key_hash(api, key)` | Existing 32-bit hash: Integer bits through the mixer; all String bytes or Symbol spelling through FNV-1a; Float bits with either zero normalized to zero; other object addresses through the mixer. |

Both operations run synchronously on the owner thread and retain neither the
supplied table nor their operands. They reread the current Integer, Float and
String callbacks at the original points, and use the caller's current vtable,
Symbol behavior and Symbol-name operations. Those callbacks can reenter or
collect; existing operand rooting and borrowed byte/name contracts still apply.
There is no public equality/hash dispatch or new built-in allocation.

Keep the original short-circuit order and pointer/extent handling. Identical
NaN objects compare equal through identity; distinct NaN objects do not. Positive
and negative Float zero compare equal and hash equally. Integer and Float values
remain distinct, even when their hash bits coincide. Symbols and Strings may
hash the same spelling while remaining unequal. String hashes handle empty
extents before null pointers; a nonempty null extent or null Symbol name hashes
to zero. Equality preserves the existing String byte contract.

IdApi v7 and image encoding are unchanged. Run `mise run collection-value` for
the native consumer/scalar oracle and
`scripts/collection-value-image-smoke.sh PREFIX` for installed debug startup,
a source-free compiled module, a v8 native Code image and a baked executable.
