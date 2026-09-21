# Lifetime owner v2

The 160-byte descriptor preserves v1's leading operations and appends `edges`
and `set_edge`. Its version is 2; consumers reject the v1 descriptor before
reading the added fields. See [the independent C declaration](lifetime_v2.h).

`set_edge(context, object, key, target)` records a strong reference held in
native state belonging to `object`. The native key selects one replaceable
reference. Null removes it. Both non-null objects must already be allocated;
registration follows the owner's thread and collection-admission contract.

`edges` returns a pointer/count view of `{ key, target }` records, borrowed until
the next edge mutation. Keys and source objects are bookkeeping, not roots.
Trace follows targets only when visiting the source object, including behavior
dictionaries and permanent objects. Unreachable cycles remain collectable.
Lifetime retirement removes records before source-address reuse.

Runtime uses these edges for generated method bindings: behavior plus selector
identifies the reference to the TCC Unit. Retained Method objects hold that Unit
in their ordinary traced payload. Neither mechanism adds code ownership to Id.

The installed `scripts/id-native-loading-smoke.py PREFIX` exercises native edges
and cycle collection before Runtime exists, through the same shipped Lifetime
and collector entries later used by the language.
